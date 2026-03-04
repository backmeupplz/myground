use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, BackupConfig, BackupJob, GlobalConfig};
use crate::error::AppError;
use crate::registry::AppDefinition;
use crate::state::BackupJobProgress;

pub const RESTIC_IMAGE: &str = "restic/restic:latest";

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Snapshot {
    pub id: String,
    pub time: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub hostname: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SnapshotFile {
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mtime: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BackupResult {
    pub snapshot_id: String,
    pub files_new: u64,
    pub bytes_added: u64,
}

/// Restic's JSON summary line from `backup --json`.
#[derive(Deserialize)]
struct ResticSummary {
    #[serde(default)]
    message_type: String,
    #[serde(default)]
    snapshot_id: String,
    #[serde(default)]
    files_new: u64,
    #[serde(default)]
    data_added: u64,
}

fn backup_err(msg: impl Into<String>) -> AppError {
    AppError::Backup(msg.into())
}

fn require_config(config: &BackupConfig) -> Result<(), AppError> {
    if config.repository.is_none() {
        return Err(backup_err("No backup repository configured"));
    }
    if config.password.is_none() {
        return Err(backup_err("No backup password configured"));
    }
    Ok(())
}

// ── Docker command helpers ──────────────────────────────────────────────────

/// Run a docker command, returning (stdout, stderr, success) without failing.
pub async fn run_docker_raw(args: &[&str]) -> Result<(String, String, bool), AppError> {
    let output = tokio::process::Command::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| backup_err(format!("Docker command failed: {e}")))?;

    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    ))
}

/// Run a docker command and return stdout, or an error with stderr.
async fn run_docker(args: &[&str], context: &str) -> Result<String, AppError> {
    let (stdout, stderr, success) = run_docker_raw(args).await?;
    if !success {
        return Err(backup_err(format!("{context}: {stderr}")));
    }
    Ok(stdout)
}

// ── Restic command building ─────────────────────────────────────────────────

/// Build the docker run command args for a restic invocation.
/// Sensitive env vars (passwords, secret keys) are returned separately
/// so the caller can pass them via `--env-file` instead of `-e`.
fn build_restic_args(
    restic_args: &[&str],
    config: &BackupConfig,
    volume_mounts: &[(String, String)],
) -> (Vec<String>, Vec<(String, String)>) {
    let repo = config.repository.as_deref().unwrap_or("");
    let password = config.password.as_deref().unwrap_or("");

    let mut cmd = vec!["run".to_string(), "--rm".to_string()];
    let mut secrets = Vec::new();

    // Repository: S3 vs local
    if repo.starts_with("s3:") {
        cmd.extend(["-e".to_string(), format!("RESTIC_REPOSITORY={repo}")]);
        if let Some(ref key) = config.s3_access_key {
            secrets.push(("AWS_ACCESS_KEY_ID".to_string(), key.clone()));
        }
        if let Some(ref secret) = config.s3_secret_key {
            secrets.push(("AWS_SECRET_ACCESS_KEY".to_string(), secret.clone()));
        }
    } else {
        cmd.extend([
            "-v".to_string(),
            format!("{repo}:/repo"),
            "-e".to_string(),
            "RESTIC_REPOSITORY=/repo".to_string(),
        ]);
    }

    secrets.push(("RESTIC_PASSWORD".to_string(), password.to_string()));

    for (host, container) in volume_mounts {
        cmd.extend(["-v".to_string(), format!("{host}:{container}")]);
    }

    cmd.push(RESTIC_IMAGE.to_string());
    cmd.extend(restic_args.iter().map(|s| s.to_string()));

    (cmd, secrets)
}

/// Write secrets to a temporary env-file and return its path.
/// The file has restricted permissions (0o600).
fn write_env_file(secrets: &[(String, String)]) -> Result<std::path::PathBuf, AppError> {
    let tmp_dir = std::env::temp_dir();
    let env_path = tmp_dir.join(format!("myground-restic-{}.env", std::process::id()));
    let content: String = secrets
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&env_path, &content)
        .map_err(|e| backup_err(format!("Write env-file: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(env_path)
}

/// Build docker args for a restic command and write secrets to a temp env-file.
/// Returns (args, env_file_path). Caller must clean up env_file_path when done.
pub fn prepare_restic_cmd(
    restic_args: &[&str],
    config: &BackupConfig,
    volume_mounts: &[(String, String)],
) -> Result<(Vec<String>, std::path::PathBuf), AppError> {
    let (mut args, secrets) = build_restic_args(restic_args, config, volume_mounts);
    let env_file = write_env_file(&secrets)?;
    args.insert(2, format!("--env-file={}", env_file.to_string_lossy()));
    Ok((args, env_file))
}

/// Run a restic command via `docker run`.
/// Sensitive env vars are passed via a temporary `--env-file` (not visible in /proc).
pub async fn run_restic(
    restic_args: &[&str],
    config: &BackupConfig,
    volume_mounts: &[(String, String)],
) -> Result<String, AppError> {
    let (args, env_file) = prepare_restic_cmd(restic_args, config, volume_mounts)?;
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let result = run_docker(&str_args, "Restic command failed").await;
    let _ = std::fs::remove_file(&env_file);
    result
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Pull the restic Docker image.
pub async fn ensure_restic_image() -> Result<(), AppError> {
    run_docker(&["pull", RESTIC_IMAGE], "Docker pull failed").await?;
    Ok(())
}

/// Initialize a restic repository (idempotent).
pub async fn init_repo(config: &BackupConfig) -> Result<String, AppError> {
    require_config(config)?;

    // For local repos, ensure the directory exists
    if let Some(ref repo) = config.repository {
        if !repo.starts_with("s3:") {
            std::fs::create_dir_all(repo)
                .map_err(|e| backup_err(format!("Failed to create repo dir: {e}")))?;
        }
    }

    let (args, env_file) = prepare_restic_cmd(&["init"], config, &[])?;
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (stdout, stderr, success) = run_docker_raw(&str_args).await?;
    let _ = std::fs::remove_file(&env_file);

    if !success {
        if stderr.contains("already initialized") || stdout.contains("already initialized") {
            return Ok("Repository already initialized".to_string());
        }
        return Err(backup_err(format!("Restic init failed: {stderr}")));
    }

    Ok(stdout)
}

/// Parse the JSON output from `restic backup --json` to extract the summary.
fn parse_backup_result(output: &str) -> Result<BackupResult, AppError> {
    for line in output.lines() {
        if let Ok(summary) = serde_json::from_str::<ResticSummary>(line) {
            if summary.message_type == "summary" {
                return Ok(BackupResult {
                    snapshot_id: summary.snapshot_id,
                    files_new: summary.files_new,
                    bytes_added: summary.data_added,
                });
            }
        }
    }

    Err(backup_err("No backup summary found in restic output"))
}

/// Dump a database from a running container to a host directory.
pub async fn dump_database(
    container: &str,
    command: &str,
    dump_file: &str,
    dump_dir: &str,
) -> Result<String, AppError> {
    std::fs::create_dir_all(dump_dir)
        .map_err(|e| backup_err(format!("Failed to create dump dir: {e}")))?;

    // 1. Run dump inside container
    run_docker(
        &["exec", container, "sh", "-c", &format!("{command} > /tmp/{dump_file}")],
        "Database dump failed",
    )
    .await?;

    // 2. Copy dump to host
    run_docker(
        &["cp", &format!("{container}:/tmp/{dump_file}"), &format!("{dump_dir}/{dump_file}")],
        "Docker cp failed",
    )
    .await?;

    // 3. Clean up inside container (best-effort)
    let _ = tokio::process::Command::new("docker")
        .args(["exec", container, "rm", &format!("/tmp/{dump_file}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    Ok(format!("{dump_dir}/{dump_file}"))
}

// ── Job-based backup ────────────────────────────────────────────────────────

/// Resolve a BackupJob to a concrete BackupConfig using defaults from GlobalConfig.
pub fn resolve_job_destination(
    job: &BackupJob,
    app_id: &str,
    global_config: &GlobalConfig,
    backup_password: Option<&str>,
) -> BackupConfig {
    let default = match job.destination_type.as_str() {
        "local" => global_config.default_local_destination.clone(),
        _ => global_config.default_remote_destination.clone(),
    };

    let mut repo = job.repository.clone().or_else(|| default.as_ref().and_then(|d| d.repository.clone()));
    // Expand ~ and append /{app_id} to repository path
    if let Some(ref mut r) = repo {
        *r = config::expand_tilde(r);
        if !r.ends_with(&format!("/{app_id}")) {
            if r.starts_with("s3:") {
                *r = format!("{r}/{app_id}");
            } else {
                let r_trimmed = r.trim_end_matches('/');
                *r = format!("{r_trimmed}/{app_id}");
            }
        }
    }

    BackupConfig {
        repository: repo,
        password: job.password.clone()
            .or_else(|| backup_password.map(|s| s.to_string()))
            .or_else(|| default.as_ref().and_then(|d| d.password.clone())),
        s3_access_key: job.s3_access_key.clone()
            .or_else(|| default.as_ref().and_then(|d| d.s3_access_key.clone())),
        s3_secret_key: job.s3_secret_key.clone()
            .or_else(|| default.as_ref().and_then(|d| d.s3_secret_key.clone())),
    }
}

/// Insert a "running" entry into the progress map.
fn init_job_progress(
    progress_map: &Arc<RwLock<HashMap<String, BackupJobProgress>>>,
    job_id: &str,
    app_id: &str,
) {
    let mut map = progress_map.write().unwrap();
    map.insert(job_id.to_string(), BackupJobProgress {
        job_id: job_id.to_string(),
        app_id: app_id.to_string(),
        status: "running".to_string(),
        percent_done: 0.0,
        seconds_remaining: None,
        bytes_done: 0,
        bytes_total: 0,
        current_file: None,
        error: None,
        log_lines: Vec::new(),
        started_at: chrono::Utc::now().to_rfc3339(),
    });
}

/// Persist job outcome (last_run_at, last_status, last_error, last_log_lines) to the app state file.
fn persist_job_status(
    base: &Path,
    app_id: &str,
    job_id: &str,
    error: Option<&str>,
    progress_map: &Arc<RwLock<HashMap<String, BackupJobProgress>>>,
) {
    let now = chrono::Utc::now().to_rfc3339();
    // Grab log lines from the in-memory progress before it gets cleaned up.
    let log_lines: Vec<String> = {
        let map = progress_map.read().unwrap();
        map.get(job_id)
            .map(|p| p.log_lines.iter().rev().take(200).rev().cloned().collect())
            .unwrap_or_default()
    };
    if let Ok(mut st) = config::load_app_state(base, app_id) {
        if let Some(j) = st.backup_jobs.iter_mut().find(|j| j.id == job_id) {
            j.last_run_at = Some(now.clone());
            j.last_log_lines = log_lines;
            if let Some(e) = error {
                j.last_status = Some("failed".to_string());
                j.last_error = Some(e.to_string());
            } else {
                j.last_status = Some("succeeded".to_string());
                j.last_error = None;
            }
        }
        st.last_backup_at = Some(now);
        let _ = config::save_app_state(base, app_id, &st);
    }
}

/// Update progress to final state and schedule cleanup after 30s.
fn finalize_job_progress(
    progress_map: &Arc<RwLock<HashMap<String, BackupJobProgress>>>,
    job_id: &str,
    error: Option<&str>,
) {
    {
        let mut map = progress_map.write().unwrap();
        if let Some(p) = map.get_mut(job_id) {
            if let Some(e) = error {
                p.status = "failed".to_string();
                p.error = Some(e.to_string());
            } else {
                p.status = "succeeded".to_string();
                p.percent_done = 1.0;
            }
        }
    }
    let progress_map_clone = progress_map.clone();
    let job_id_owned = job_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        progress_map_clone.write().unwrap().remove(&job_id_owned);
    });
}

/// Run a single backup job for an app. Updates progress and persists job status.
pub async fn backup_job_run(
    base: &Path,
    app_id: &str,
    job_id: &str,
    registry: &HashMap<String, AppDefinition>,
    global_config: &GlobalConfig,
    progress_map: &Arc<RwLock<HashMap<String, BackupJobProgress>>>,
) -> Result<Vec<BackupResult>, AppError> {
    let svc_state = config::load_app_state(base, app_id)?;
    if !svc_state.installed {
        return Err(AppError::NotInstalled(app_id.to_string()));
    }

    let job = svc_state.backup_jobs.iter().find(|j| j.id == job_id)
        .ok_or_else(|| backup_err(format!("Backup job {job_id} not found")))?
        .clone();

    let def = crate::apps::lookup_definition(app_id, registry, base)?;
    let storage_paths = config::resolve_storage_paths(base, app_id, def, global_config, &svc_state);
    let dump_dir = base.join("apps").join(app_id).join("dumps");
    let dump_dir_str = dump_dir.to_string_lossy().to_string();

    let cfg = resolve_job_destination(&job, app_id, global_config, svc_state.backup_password.as_deref());

    init_job_progress(progress_map, job_id, app_id);

    // Init repo (best-effort, idempotent)
    let _ = init_repo(&cfg).await;

    let mut results = Vec::new();
    let mut error_msg: Option<String> = None;

    for vol in &def.storage {
        let Some(host_path) = storage_paths.get(&format!("STORAGE_{}", vol.name)) else {
            continue;
        };

        let tag = format!("{app_id}/{}", vol.name);

        let res = if let Some(ref db_dump) = vol.db_dump {
            match dump_database(&db_dump.container, &db_dump.command, &db_dump.dump_file, &dump_dir_str).await {
                Ok(_) => {
                    let r = backup_path_streaming(&dump_dir_str, &tag, &cfg, job_id, progress_map).await;
                    let _ = std::fs::remove_file(dump_dir.join(&db_dump.dump_file));
                    r
                }
                Err(e) => Err(e),
            }
        } else {
            backup_path_streaming(host_path, &tag, &cfg, job_id, progress_map).await
        };

        match res {
            Ok(r) => results.push(r),
            Err(e) => {
                error_msg = Some(e.to_string());
                break;
            }
        }
    }

    persist_job_status(base, app_id, job_id, error_msg.as_deref(), progress_map);
    finalize_job_progress(progress_map, job_id, error_msg.as_deref());

    if let Some(e) = error_msg {
        return Err(backup_err(e));
    }

    Ok(results)
}

/// Backup a path with streaming progress updates.
async fn backup_path_streaming(
    host_path: &str,
    tag: &str,
    config: &BackupConfig,
    job_id: &str,
    progress_map: &Arc<RwLock<HashMap<String, BackupJobProgress>>>,
) -> Result<BackupResult, AppError> {
    require_config(config)?;

    let mounts = vec![(host_path.to_string(), "/data:ro".to_string())];
    let (str_args, env_file) = prepare_restic_cmd(&["backup", "/data", "--tag", tag, "--json"], config, &mounts)?;
    let mut child = tokio::process::Command::new("docker")
        .args(&str_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| backup_err(format!("Failed to spawn docker: {e}")))?;

    let stdout = child.stdout.take().unwrap();
    let mut reader = tokio::io::BufReader::new(stdout);
    let mut output = String::new();

    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                output.push_str(&line);
                // Try to parse as restic status JSON
                if let Ok(status) = serde_json::from_str::<ResticStatus>(&line) {
                    if status.message_type == "status" {
                        let mut map = progress_map.write().unwrap();
                        if let Some(p) = map.get_mut(job_id) {
                            p.percent_done = status.percent_done;
                            p.seconds_remaining = status.seconds_remaining;
                            p.bytes_done = status.bytes_done;
                            p.bytes_total = status.total_bytes;
                            p.current_file = status.current_files.first().cloned();
                            if p.log_lines.len() < 200 {
                                p.log_lines.push(line.trim().to_string());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&env_file);
                return Err(backup_err(format!("Read stdout: {e}")));
            }
        }
    }

    let status = child.wait().await.map_err(|e| backup_err(format!("Wait: {e}")))?;
    let _ = std::fs::remove_file(&env_file);

    if !status.success() {
        // Read stderr for details
        let stderr_output = if let Some(mut stderr) = child.stderr.take() {
            let mut buf = String::new();
            use tokio::io::AsyncReadExt;
            let _ = stderr.read_to_string(&mut buf).await;
            buf
        } else {
            String::new()
        };
        let detail = stderr_output.trim();
        // Append stderr lines to progress log for persistence
        if !detail.is_empty() {
            let mut map = progress_map.write().unwrap();
            if let Some(p) = map.get_mut(job_id) {
                for line in detail.lines().take(50) {
                    p.log_lines.push(format!("[stderr] {line}"));
                }
            }
        }
        let msg = if detail.is_empty() {
            format!("Backup failed (exit code {})", status.code().unwrap_or(-1))
        } else {
            let trimmed = if detail.len() > 500 { &detail[detail.len()-500..] } else { detail };
            format!("Backup failed: {trimmed}")
        };
        return Err(backup_err(msg));
    }

    parse_backup_result(&output)
}

/// Restic JSON status line from `backup --json`.
#[derive(Deserialize)]
struct ResticStatus {
    #[serde(default)]
    message_type: String,
    #[serde(default)]
    percent_done: f64,
    #[serde(default)]
    seconds_remaining: Option<i64>,
    #[serde(default)]
    bytes_done: u64,
    #[serde(default)]
    total_bytes: u64,
    #[serde(default)]
    current_files: Vec<String>,
}

/// Verify a backup repository is accessible.
#[derive(Debug, Serialize, ToSchema)]
pub struct VerifyResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn verify_repo(config: &BackupConfig) -> VerifyResult {
    if config.repository.is_none() {
        return VerifyResult { ok: false, snapshot_count: None, error: Some("No repository configured".to_string()) };
    }
    if config.password.is_none() {
        return VerifyResult { ok: false, snapshot_count: None, error: Some("No password configured".to_string()) };
    }
    // Local repos are mounted as Docker volumes — validate against sensitive paths.
    if let Some(ref repo) = config.repository {
        if !repo.starts_with("s3:") {
            if let Err(e) = crate::config::validate_storage_path(repo) {
                return VerifyResult { ok: false, snapshot_count: None, error: Some(e.to_string()) };
            }
        }
    }

    let (args, env_file) = match prepare_restic_cmd(&["snapshots", "--json"], config, &[]) {
        Ok(v) => v,
        Err(e) => return VerifyResult { ok: false, snapshot_count: None, error: Some(e.to_string()) },
    };
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let result = run_docker_raw(&str_args).await;
    let _ = std::fs::remove_file(&env_file);

    match result {
        Ok((stdout, stderr, success)) => {
            if success {
                let count = serde_json::from_str::<Vec<serde_json::Value>>(&stdout)
                    .map(|v| v.len())
                    .unwrap_or(0);
                VerifyResult { ok: true, snapshot_count: Some(count), error: None }
            } else if stderr.contains("wrong password") || stderr.contains("incorrect password") {
                VerifyResult { ok: false, snapshot_count: None, error: Some("Incorrect encryption key".to_string()) }
            } else if stderr.contains("not found") || stderr.contains("does not exist") || stderr.contains("Is there a repository") {
                VerifyResult { ok: false, snapshot_count: None, error: Some("Repository not found. Would you like to initialize it?".to_string()) }
            } else {
                VerifyResult { ok: false, snapshot_count: None, error: Some(stderr.trim().to_string()) }
            }
        }
        Err(e) => VerifyResult { ok: false, snapshot_count: None, error: Some(e.to_string()) },
    }
}

/// Restore a database from a backup dump file.
pub async fn restore_database(
    container: &str,
    restore_command: &str,
    dump_file: &str,
    dump_path: &str,
) -> Result<(), AppError> {
    // 1. Copy dump into container
    run_docker(
        &["cp", dump_path, &format!("{container}:/tmp/{dump_file}")],
        "Docker cp for restore failed",
    )
    .await?;

    // 2. Run restore command
    run_docker(
        &["exec", container, "sh", "-c", restore_command],
        "Database restore failed",
    )
    .await?;

    // 3. Clean up inside container (best-effort)
    let _ = tokio::process::Command::new("docker")
        .args(["exec", container, "rm", &format!("/tmp/{dump_file}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    Ok(())
}

/// Backward-compat: Backup a single app using its backup_jobs.
pub async fn backup_app(
    base: &Path,
    app_id: &str,
    registry: &HashMap<String, AppDefinition>,
    global_config: &GlobalConfig,
) -> Result<Vec<BackupResult>, AppError> {
    let svc_state = config::load_app_state(base, app_id)?;
    if !svc_state.installed {
        return Err(AppError::NotInstalled(app_id.to_string()));
    }

    if svc_state.backup_jobs.is_empty() {
        return Ok(Vec::new());
    }

    let progress_map = Arc::new(RwLock::new(HashMap::new()));
    let mut all_results = Vec::new();

    for job in &svc_state.backup_jobs {
        match backup_job_run(base, app_id, &job.id, registry, global_config, &progress_map).await {
            Ok(results) => all_results.extend(results),
            Err(e) => {
                tracing::error!("Backup job {} for {app_id} failed: {e}", job.id);
            }
        }
    }

    Ok(all_results)
}

/// Backward-compat: Backup all installed apps.
pub async fn backup_all(
    base: &Path,
    registry: &HashMap<String, AppDefinition>,
    global_config: &GlobalConfig,
) -> Result<Vec<BackupResult>, AppError> {
    let mut all_results = Vec::new();
    for app_id in &config::list_installed_apps(base) {
        all_results.extend(
            backup_app(base, app_id, registry, global_config).await?,
        );
    }

    Ok(all_results)
}

/// List snapshots in the repository.
pub async fn list_snapshots(config: &BackupConfig) -> Result<Vec<Snapshot>, AppError> {
    require_config(config)?;

    let output = run_restic(&["snapshots", "--json"], config, &[]).await?;
    serde_json::from_str(&output)
        .map_err(|e| backup_err(format!("Failed to parse snapshots: {e}")))
}

/// List files in a snapshot using `restic ls`.
pub async fn list_snapshot_files(
    snapshot_id: &str,
    path_prefix: Option<&str>,
    config: &BackupConfig,
) -> Result<Vec<SnapshotFile>, AppError> {
    require_config(config)?;

    let output = run_restic(&["ls", snapshot_id, "--json"], config, &[]).await?;

    let mut files = Vec::new();
    for line in output.lines() {
        // restic ls --json outputs one JSON object per line; skip the first "snapshot" line
        if let Ok(entry) = serde_json::from_str::<SnapshotFile>(line) {
            if entry.file_type == "snapshot" {
                continue;
            }
            if let Some(prefix) = path_prefix {
                if !entry.path.starts_with(prefix) {
                    continue;
                }
            }
            files.push(entry);
        }
    }

    Ok(files)
}

/// Delete a snapshot using `restic forget`.
pub async fn forget_snapshot(
    snapshot_id: &str,
    config: &BackupConfig,
) -> Result<String, AppError> {
    require_config(config)?;
    run_restic(&["forget", snapshot_id], config, &[]).await
}

/// Restore a snapshot to a target path.
pub async fn restore_snapshot(
    target_path: &str,
    snapshot_id: &str,
    config: &BackupConfig,
) -> Result<String, AppError> {
    require_config(config)?;

    std::fs::create_dir_all(target_path)
        .map_err(|e| backup_err(format!("Failed to create restore target: {e}")))?;

    let mounts = vec![(target_path.to_string(), "/restore".to_string())];
    run_restic(&["restore", snapshot_id, "--target", "/restore"], config, &mounts).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_snapshot_json() {
        let json = r#"[{
            "id": "abc123",
            "time": "2026-02-27T10:00:00Z",
            "paths": ["/data"],
            "tags": ["immich/upload"],
            "hostname": "test"
        }]"#;

        let snapshots: Vec<Snapshot> = serde_json::from_str(json).unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, "abc123");
        assert_eq!(snapshots[0].paths, vec!["/data"]);
        assert_eq!(snapshots[0].tags, vec!["immich/upload"]);
    }

    #[test]
    fn build_restic_args_local_repo() {
        let config = BackupConfig {
            repository: Some("/backups".to_string()),
            password: Some("secret".to_string()),
            ..Default::default()
        };

        let (args, secrets) = build_restic_args(&["snapshots", "--json"], &config, &[]);

        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"/backups:/repo".to_string()));
        assert!(args.contains(&"RESTIC_REPOSITORY=/repo".to_string()));
        assert!(args.contains(&RESTIC_IMAGE.to_string()));
        assert!(args.contains(&"snapshots".to_string()));
        assert!(args.contains(&"--json".to_string()));
        // Password should be in secrets, not in command args
        assert!(!args.iter().any(|a| a.contains("RESTIC_PASSWORD")));
        assert!(secrets.iter().any(|(k, v)| k == "RESTIC_PASSWORD" && v == "secret"));
    }

    #[test]
    fn build_restic_args_s3_repo() {
        let config = BackupConfig {
            repository: Some("s3:https://s3.amazonaws.com/mybucket".to_string()),
            password: Some("secret".to_string()),
            s3_access_key: Some("AKID".to_string()),
            s3_secret_key: Some("SKEY".to_string()),
            ..Default::default()
        };

        let (args, secrets) = build_restic_args(&["backup", "/data"], &config, &[]);

        assert!(args.contains(&"RESTIC_REPOSITORY=s3:https://s3.amazonaws.com/mybucket".to_string()));
        // S3 credentials should be in secrets, not args
        assert!(!args.iter().any(|a| a.contains("AWS_ACCESS_KEY_ID")));
        assert!(secrets.iter().any(|(k, v)| k == "AWS_ACCESS_KEY_ID" && v == "AKID"));
        assert!(secrets.iter().any(|(k, v)| k == "AWS_SECRET_ACCESS_KEY" && v == "SKEY"));
        assert!(!args.iter().any(|a| a.contains(":/repo")));
    }

    #[test]
    fn build_restic_args_with_volume_mounts() {
        let config = BackupConfig {
            repository: Some("/backups".to_string()),
            password: Some("secret".to_string()),
            ..Default::default()
        };

        let mounts = vec![("/host/data".to_string(), "/data:ro".to_string())];
        let (args, _secrets) = build_restic_args(&["backup", "/data"], &config, &mounts);

        assert!(args.contains(&"/host/data:/data:ro".to_string()));
    }

    #[test]
    fn backup_config_defaults() {
        let config = BackupConfig::default();
        assert!(config.repository.is_none());
        assert!(config.password.is_none());
        assert!(config.s3_access_key.is_none());
        assert!(config.s3_secret_key.is_none());
    }

    #[test]
    fn parse_backup_result_from_json() {
        let output = r#"{"message_type":"status","percent_done":0.5}
{"message_type":"summary","snapshot_id":"abc123","files_new":10,"data_added":1024}"#;

        let result = parse_backup_result(output).unwrap();
        assert_eq!(result.snapshot_id, "abc123");
        assert_eq!(result.files_new, 10);
        assert_eq!(result.bytes_added, 1024);
    }

    #[test]
    fn require_config_rejects_missing_repo() {
        let config = BackupConfig {
            password: Some("secret".to_string()),
            ..Default::default()
        };
        assert!(require_config(&config).is_err());
    }

    #[test]
    fn require_config_rejects_missing_password() {
        let config = BackupConfig {
            repository: Some("/backups".to_string()),
            ..Default::default()
        };
        assert!(require_config(&config).is_err());
    }

    #[test]
    fn require_config_accepts_valid() {
        let config = BackupConfig {
            repository: Some("/backups".to_string()),
            password: Some("secret".to_string()),
            ..Default::default()
        };
        assert!(require_config(&config).is_ok());
    }
}
