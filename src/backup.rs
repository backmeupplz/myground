use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, BackupConfig, GlobalConfig};
use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

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

fn backup_err(msg: impl Into<String>) -> ServiceError {
    ServiceError::Backup(msg.into())
}

fn require_config(config: &BackupConfig) -> Result<(), ServiceError> {
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
async fn run_docker_raw(args: &[&str]) -> Result<(String, String, bool), ServiceError> {
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
async fn run_docker(args: &[&str], context: &str) -> Result<String, ServiceError> {
    let (stdout, stderr, success) = run_docker_raw(args).await?;
    if !success {
        return Err(backup_err(format!("{context}: {stderr}")));
    }
    Ok(stdout)
}

// ── Restic command building ─────────────────────────────────────────────────

/// Build the docker run command args for a restic invocation.
pub fn build_restic_args(
    restic_args: &[&str],
    config: &BackupConfig,
    volume_mounts: &[(String, String)],
) -> Vec<String> {
    let repo = config.repository.as_deref().unwrap_or("");
    let password = config.password.as_deref().unwrap_or("");

    let mut cmd = vec!["run".to_string(), "--rm".to_string()];

    // Repository: S3 vs local
    if repo.starts_with("s3:") {
        cmd.extend(["-e".to_string(), format!("RESTIC_REPOSITORY={repo}")]);
        if let Some(ref key) = config.s3_access_key {
            cmd.extend(["-e".to_string(), format!("AWS_ACCESS_KEY_ID={key}")]);
        }
        if let Some(ref secret) = config.s3_secret_key {
            cmd.extend(["-e".to_string(), format!("AWS_SECRET_ACCESS_KEY={secret}")]);
        }
    } else {
        cmd.extend([
            "-v".to_string(),
            format!("{repo}:/repo"),
            "-e".to_string(),
            "RESTIC_REPOSITORY=/repo".to_string(),
        ]);
    }

    cmd.extend(["-e".to_string(), format!("RESTIC_PASSWORD={password}")]);

    for (host, container) in volume_mounts {
        cmd.extend(["-v".to_string(), format!("{host}:{container}")]);
    }

    cmd.push(RESTIC_IMAGE.to_string());
    cmd.extend(restic_args.iter().map(|s| s.to_string()));

    cmd
}

/// Run a restic command via `docker run`.
pub async fn run_restic(
    restic_args: &[&str],
    config: &BackupConfig,
    volume_mounts: &[(String, String)],
) -> Result<String, ServiceError> {
    let args = build_restic_args(restic_args, config, volume_mounts);
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_docker(&str_args, "Restic command failed").await
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Pull the restic Docker image.
pub async fn ensure_restic_image() -> Result<(), ServiceError> {
    run_docker(&["pull", RESTIC_IMAGE], "Docker pull failed").await?;
    Ok(())
}

/// Initialize a restic repository (idempotent).
pub async fn init_repo(config: &BackupConfig) -> Result<String, ServiceError> {
    require_config(config)?;

    // For local repos, ensure the directory exists
    if let Some(ref repo) = config.repository {
        if !repo.starts_with("s3:") {
            std::fs::create_dir_all(repo)
                .map_err(|e| backup_err(format!("Failed to create repo dir: {e}")))?;
        }
    }

    let args = build_restic_args(&["init"], config, &[]);
    let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let (stdout, stderr, success) = run_docker_raw(&str_args).await?;

    if !success {
        if stderr.contains("already initialized") || stdout.contains("already initialized") {
            return Ok("Repository already initialized".to_string());
        }
        return Err(backup_err(format!("Restic init failed: {stderr}")));
    }

    Ok(stdout)
}

/// Backup a host path with a tag.
pub async fn backup_path(
    host_path: &str,
    tag: &str,
    config: &BackupConfig,
) -> Result<BackupResult, ServiceError> {
    require_config(config)?;

    let mounts = vec![(host_path.to_string(), "/data:ro".to_string())];
    let output = run_restic(&["backup", "/data", "--tag", tag, "--json"], config, &mounts).await?;

    parse_backup_result(&output)
}

/// Parse the JSON output from `restic backup --json` to extract the summary.
fn parse_backup_result(output: &str) -> Result<BackupResult, ServiceError> {
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
) -> Result<String, ServiceError> {
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

/// Backup a single service. Uses per-service backup config if set, else falls back to global.
pub async fn backup_service(
    base: &Path,
    service_id: &str,
    registry: &HashMap<String, ServiceDefinition>,
    global_config: &GlobalConfig,
    backup_config: &BackupConfig,
) -> Result<Vec<BackupResult>, ServiceError> {
    let svc_state = config::load_service_state(base, service_id)?;
    if !svc_state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let def = crate::services::lookup_definition(service_id, registry, base)?;

    // Determine which backup configs to use
    let svc_backup = svc_state.backup.as_ref();
    let use_local = svc_backup.and_then(|b| b.local.as_ref());
    let use_remote = svc_backup.and_then(|b| b.remote.as_ref());
    let svc_enabled = svc_backup.map(|b| b.enabled).unwrap_or(true);

    // If service backup is explicitly disabled, skip
    if !svc_enabled {
        return Ok(Vec::new());
    }

    let storage_paths =
        config::resolve_storage_paths(base, service_id, def, global_config, &svc_state);

    let dump_dir = base.join("services").join(service_id).join("dumps");
    let dump_dir_str = dump_dir.to_string_lossy().to_string();

    // Collect configs to run against (owned, so we can inject the backup password)
    let mut configs_to_use: Vec<BackupConfig> = Vec::new();
    if let Some(local) = use_local {
        configs_to_use.push(local.clone());
    }
    if let Some(remote) = use_remote {
        configs_to_use.push(remote.clone());
    }
    if configs_to_use.is_empty() {
        // Fall back to global config
        require_config(backup_config)?;
        configs_to_use.push(backup_config.clone());
    }

    // Inject the service-level backup password into any config that lacks one
    if let Some(ref pwd) = svc_state.backup_password {
        for cfg in &mut configs_to_use {
            if cfg.password.is_none() {
                cfg.password = Some(pwd.clone());
            }
        }
    }

    let mut results = Vec::new();
    for cfg in &configs_to_use {
        for vol in &def.storage {
            let Some(host_path) = storage_paths.get(&format!("STORAGE_{}", vol.name)) else {
                continue;
            };

            let tag = format!("{service_id}/{}", vol.name);

            if let Some(ref db_dump) = vol.db_dump {
                dump_database(&db_dump.container, &db_dump.command, &db_dump.dump_file, &dump_dir_str)
                    .await?;
                results.push(backup_path(&dump_dir_str, &tag, cfg).await?);
                let _ = std::fs::remove_file(dump_dir.join(&db_dump.dump_file));
            } else {
                results.push(backup_path(host_path, &tag, cfg).await?);
            }
        }
    }

    Ok(results)
}

/// Backup all installed services.
pub async fn backup_all(
    base: &Path,
    registry: &HashMap<String, ServiceDefinition>,
    global_config: &GlobalConfig,
    backup_config: &BackupConfig,
) -> Result<Vec<BackupResult>, ServiceError> {
    require_config(backup_config)?;

    let mut all_results = Vec::new();
    for service_id in &config::list_installed_services(base) {
        all_results.extend(
            backup_service(base, service_id, registry, global_config, backup_config).await?,
        );
    }

    Ok(all_results)
}

/// List snapshots in the repository.
pub async fn list_snapshots(config: &BackupConfig) -> Result<Vec<Snapshot>, ServiceError> {
    require_config(config)?;

    let output = run_restic(&["snapshots", "--json"], config, &[]).await?;
    serde_json::from_str(&output)
        .map_err(|e| backup_err(format!("Failed to parse snapshots: {e}")))
}

/// Restore a snapshot to a target path.
pub async fn restore_snapshot(
    target_path: &str,
    snapshot_id: &str,
    config: &BackupConfig,
) -> Result<String, ServiceError> {
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

        let args = build_restic_args(&["snapshots", "--json"], &config, &[]);

        assert!(args.contains(&"run".to_string()));
        assert!(args.contains(&"--rm".to_string()));
        assert!(args.contains(&"/backups:/repo".to_string()));
        assert!(args.contains(&"RESTIC_REPOSITORY=/repo".to_string()));
        assert!(args.contains(&"RESTIC_PASSWORD=secret".to_string()));
        assert!(args.contains(&RESTIC_IMAGE.to_string()));
        assert!(args.contains(&"snapshots".to_string()));
        assert!(args.contains(&"--json".to_string()));
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

        let args = build_restic_args(&["backup", "/data"], &config, &[]);

        assert!(args.contains(&"RESTIC_REPOSITORY=s3:https://s3.amazonaws.com/mybucket".to_string()));
        assert!(args.contains(&"AWS_ACCESS_KEY_ID=AKID".to_string()));
        assert!(args.contains(&"AWS_SECRET_ACCESS_KEY=SKEY".to_string()));
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
        let args = build_restic_args(&["backup", "/data"], &config, &mounts);

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
