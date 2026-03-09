use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::aws::{AwsSetupRequest, AwsSetupResult};
use crate::backup::{self, BackupResult, Snapshot, SnapshotFile, VerifyResult};
use crate::config::{self, BackupConfig, BackupJob};
use serde::Serialize;
use crate::state::{AppState, BackupJobProgress, RestoreProgress};

use super::response::{action_err, action_ok, ActionResponse};

/// Load backup config or return a 400 error response.
fn require_backup_config(state: &AppState) -> Result<BackupConfig, axum::response::Response> {
    match config::load_backup_config(&state.data_dir) {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(action_err(StatusCode::BAD_REQUEST, "No backup config set").into_response()),
        Err(e) => Err(action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()),
    }
}

// ── Backup config (backward compat) ─────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/backup/config",
    responses(
        (status = 200, description = "Current backup configuration", body = BackupConfig)
    )
)]
pub async fn backup_config_get(State(state): State<AppState>) -> impl IntoResponse {
    let config = config::load_backup_config(&state.data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
    Json(config).into_response()
}

#[utoipa::path(
    put,
    path = "/backup/config",
    request_body = BackupConfig,
    responses(
        (status = 200, description = "Backup config updated", body = ActionResponse),
        (status = 400, description = "Update error", body = ActionResponse)
    )
)]
pub async fn backup_config_update(
    State(state): State<AppState>,
    Json(body): Json<BackupConfig>,
) -> impl IntoResponse {
    match config::save_backup_config(&state.data_dir, &body) {
        Ok(()) => action_ok("Backup config updated").into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── Init ───────────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/backup/init",
    responses(
        (status = 200, description = "Repo initialized", body = ActionResponse),
        (status = 400, description = "Init error", body = ActionResponse)
    )
)]
pub async fn backup_init(State(state): State<AppState>) -> impl IntoResponse {
    let config = match require_backup_config(&state) {
        Ok(c) => c,
        Err(r) => return r,
    };

    if let Err(e) = backup::ensure_restic_image().await {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    match backup::init_repo(&config).await {
        Ok(msg) => action_ok(msg).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── Run backups (backward compat) ──────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/backup/run",
    responses(
        (status = 200, description = "All apps backed up", body = Vec<BackupResult>),
        (status = 400, description = "Backup error", body = ActionResponse)
    )
)]
pub async fn backup_run_all(State(state): State<AppState>) -> impl IntoResponse {
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_all(&state.data_dir, &state.registry, &global_config).await {
        Ok(results) => Json(results).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/backup/run/{id}",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App backed up", body = Vec<BackupResult>),
        (status = 400, description = "Backup error", body = ActionResponse),
        (status = 404, description = "App not found", body = ActionResponse)
    )
)]
pub async fn backup_run_app(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_app(&state.data_dir, &id, &state.registry, &global_config).await {
        Ok(results) => Json(results).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── Snapshots ──────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/backup/snapshots",
    responses(
        (status = 200, description = "List of snapshots", body = Vec<Snapshot>),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn backup_snapshots(State(state): State<AppState>) -> impl IntoResponse {
    let config = match require_backup_config(&state) {
        Ok(c) => c,
        Err(r) => return r,
    };

    match backup::list_snapshots(&config).await {
        Ok(snapshots) => Json(snapshots).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── Restore ────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct RestoreStartResponse {
    pub ok: bool,
    pub message: String,
    pub restore_id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct RestoreRequest {
    #[serde(default)]
    pub target_path: String,
}

/// Validate a restic snapshot ID is a safe hex string (prevents CLI argument injection).
fn validate_snapshot_id(id: &str) -> Result<(), axum::response::Response> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(action_err(
            StatusCode::BAD_REQUEST,
            format!("Invalid snapshot ID '{id}': must be a hex string"),
        )
        .into_response());
    }
    Ok(())
}

/// Validate restore target path isn't a critical system directory.
/// Reuses the same prefix-match blocklist as storage path validation.
fn validate_restore_path(path: &str) -> Result<(), axum::response::Response> {
    // Block root
    if path.trim_end_matches('/').is_empty() || path.trim_end_matches('/') == "/" {
        return Err(action_err(
            StatusCode::BAD_REQUEST,
            format!("Restore to '{path}' is not allowed"),
        )
        .into_response());
    }
    config::validate_storage_path(path).map_err(|e| {
        action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()
    })
}

#[utoipa::path(
    post,
    path = "/backup/restore/{snapshot_id}",
    params(("snapshot_id" = String, Path, description = "Snapshot ID to restore")),
    request_body = RestoreRequest,
    responses(
        (status = 200, description = "Restore started", body = RestoreStartResponse),
        (status = 400, description = "Restore error", body = ActionResponse)
    )
)]
pub async fn backup_restore(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
    Json(body): Json<RestoreRequest>,
) -> impl IntoResponse {
    if let Err(r) = validate_snapshot_id(&snapshot_id) {
        return r;
    }
    let lookup = match find_config_for_snapshot(&state, &snapshot_id).await {
        Ok(l) => l,
        Err(r) => return r,
    };

    // Check if this snapshot is a database dump by parsing tags
    let db_restore_info = 'db: {
        for tag in &lookup.tags {
            if let Some((app_id, vol_name)) = tag.split_once('/') {
                if let Ok(def) = crate::apps::lookup_definition(app_id, &state.registry, &state.data_dir) {
                    if let Some(vol) = def.storage.iter().find(|v| v.name == vol_name) {
                        if let Some(ref db_dump) = vol.db_dump {
                            if let Some(ref restore_cmd) = db_dump.restore_command {
                                break 'db Some((
                                    db_dump.container.clone(),
                                    restore_cmd.clone(),
                                    db_dump.dump_file.clone(),
                                    db_dump.wipe_command.clone(),
                                ));
                            }
                        }
                    }
                }
            }
        }
        None
    };

    let is_db = db_restore_info.is_some();

    if !is_db {
        if let Err(r) = validate_restore_path(&body.target_path) {
            return r;
        }
    }

    let restore_id = config::generate_key_id();
    let target_path = if is_db { None } else { Some(body.target_path.clone()) };

    // Spawn async restore
    let progress = state.restore_progress.clone();
    let snapshot_id_owned = snapshot_id.clone();
    let config_owned = lookup.config.clone();
    let restore_id_clone = restore_id.clone();

    tokio::spawn(async move {
        backup::restore_with_progress(
            &restore_id_clone,
            &snapshot_id_owned,
            &config_owned,
            &progress,
            db_restore_info,
            target_path,
        )
        .await;
    });

    Json(RestoreStartResponse {
        ok: true,
        message: "Restore started".to_string(),
        restore_id,
    }).into_response()
}

// ── Snapshot detail ─────────────────────────────────────────────────────

/// Result from find_config_for_snapshot: config + snapshot tags.
struct SnapshotLookup {
    config: config::BackupConfig,
    tags: Vec<String>,
}

/// Find the BackupConfig whose repository contains the given snapshot.
async fn find_config_for_snapshot(
    state: &AppState,
    snapshot_id: &str,
) -> Result<SnapshotLookup, axum::response::Response> {
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let mut seen_repos = std::collections::HashSet::new();

    for (app_id, st) in config::list_installed_apps_with_state(&state.data_dir) {
        for job in &st.backup_jobs {
            let cfg = backup::resolve_job_destination(job, &app_id, &global_config, st.backup_password.as_deref());
            let repo_key = cfg.repository.clone().unwrap_or_default();
            if !seen_repos.insert(repo_key) {
                continue;
            }

            // Check if this repo has the snapshot
            let (args, env_file) = match crate::backup::prepare_restic_cmd(
                &["snapshots", snapshot_id, "--json"],
                &cfg,
                &[],
            ) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let str_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let result = crate::backup::run_docker_raw(&str_args).await;
            if let Err(e) = std::fs::remove_file(&env_file) {
                tracing::warn!("Failed to remove temp env file {}: {e}", env_file.display());
            }

            if let Ok((stdout, _, true)) = result {
                if let Ok(snaps) = serde_json::from_str::<Vec<backup::Snapshot>>(&stdout) {
                    if let Some(snap) = snaps.into_iter().next() {
                        return Ok(SnapshotLookup {
                            config: cfg,
                            tags: snap.tags,
                        });
                    }
                }
            }
        }
    }

    Err(action_err(StatusCode::NOT_FOUND, "Snapshot not found in any repository").into_response())
}

#[derive(Deserialize)]
pub struct SnapshotFilesQuery {
    #[serde(default)]
    pub path: Option<String>,
}

#[utoipa::path(
    get,
    path = "/backup/snapshots/{id}/files",
    params(
        ("id" = String, Path, description = "Snapshot ID"),
        ("path" = Option<String>, Query, description = "Filter to subdirectory")
    ),
    responses(
        (status = 200, description = "Files in snapshot", body = Vec<SnapshotFile>),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "Snapshot not found", body = ActionResponse)
    )
)]
pub async fn snapshot_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<SnapshotFilesQuery>,
) -> impl IntoResponse {
    if let Err(r) = validate_snapshot_id(&id) {
        return r;
    }

    let lookup = match find_config_for_snapshot(&state, &id).await {
        Ok(l) => l,
        Err(r) => return r,
    };

    match backup::list_snapshot_files(&id, query.path.as_deref(), &lookup.config).await {
        Ok(files) => Json(files).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/backup/snapshots/{id}",
    params(("id" = String, Path, description = "Snapshot ID")),
    responses(
        (status = 200, description = "Snapshot deleted", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "Snapshot not found", body = ActionResponse)
    )
)]
pub async fn snapshot_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(r) = validate_snapshot_id(&id) {
        return r;
    }

    let lookup = match find_config_for_snapshot(&state, &id).await {
        Ok(l) => l,
        Err(r) => return r,
    };

    match backup::forget_snapshot(&id, &lookup.config).await {
        Ok(_) => action_ok(format!("Snapshot {id} deleted")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── AWS auto-setup ────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/backup/aws-setup",
    request_body = AwsSetupRequest,
    responses(
        (status = 200, description = "AWS S3 bucket and IAM user created", body = AwsSetupResult),
        (status = 400, description = "Setup error", body = ActionResponse)
    )
)]
pub async fn backup_aws_setup(
    State(state): State<AppState>,
    Json(body): Json<AwsSetupRequest>,
) -> impl IntoResponse {
    match crate::aws::setup_s3_backup(body).await {
        Ok(result) => {
            let backup_config = BackupConfig {
                repository: Some(result.repository.clone()),
                s3_access_key: Some(result.s3_access_key.clone()),
                s3_secret_key: Some(result.s3_secret_key.clone()),
                ..Default::default()
            };
            if let Err(e) = config::save_backup_config(&state.data_dir, &backup_config) {
                return action_err(
                    StatusCode::BAD_REQUEST,
                    format!("AWS setup succeeded but failed to save config: {e}"),
                )
                .into_response();
            }
            Json(result).into_response()
        }
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── Backup Jobs CRUD ────────────────────────────────────────────────────────

/// Response for job listing: includes app_id alongside the job.
#[derive(serde::Serialize, ToSchema)]
pub struct BackupJobWithApp {
    pub app_id: String,
    #[serde(flatten)]
    pub job: BackupJob,
}

#[utoipa::path(
    get,
    path = "/backup/jobs",
    responses(
        (status = 200, description = "All backup jobs across all apps", body = Vec<BackupJobWithApp>)
    )
)]
pub async fn backup_jobs_list(State(state): State<AppState>) -> impl IntoResponse {
    let mut all_jobs: Vec<BackupJobWithApp> = Vec::new();
    for (id, st) in config::list_installed_apps_with_state(&state.data_dir) {
        for job in st.backup_jobs {
            all_jobs.push(BackupJobWithApp {
                app_id: id.clone(),
                job,
            });
        }
    }
    Json(all_jobs).into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct CreateJobRequest {
    pub app_id: String,
    pub destination_type: String,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub s3_access_key: Option<String>,
    #[serde(default)]
    pub s3_secret_key: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
}

#[utoipa::path(
    post,
    path = "/backup/jobs",
    request_body = CreateJobRequest,
    responses(
        (status = 200, description = "Job created", body = BackupJob),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn backup_jobs_create(
    State(state): State<AppState>,
    Json(body): Json<CreateJobRequest>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&body.app_id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    let mut svc_state = match config::load_app_state(&state.data_dir, &body.app_id) {
        Ok(s) if s.installed => s,
        Ok(_) => return action_err(StatusCode::BAD_REQUEST, format!("App {} not installed", body.app_id)).into_response(),
        Err(e) => return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    // Auto-generate backup password if needed
    if svc_state.backup_password.is_none() {
        svc_state.backup_password = Some(config::generate_backup_password(32));
    }

    let job = BackupJob {
        id: config::generate_key_id(),
        destination_type: body.destination_type,
        repository: body.repository,
        password: body.password,
        s3_access_key: body.s3_access_key,
        s3_secret_key: body.s3_secret_key,
        schedule: body.schedule,
        ..Default::default()
    };

    svc_state.backup_jobs.push(job.clone());
    if let Err(e) = config::save_app_state(&state.data_dir, &body.app_id, &svc_state) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    // Best-effort init repo
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let cfg = backup::resolve_job_destination(&job, &body.app_id, &global_config, svc_state.backup_password.as_deref());
    if let Err(e) = backup::init_repo(&cfg).await {
        tracing::warn!("Best-effort repo init failed for {}: {e}", body.app_id);
    }

    Json(job).into_response()
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateJobRequest {
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub s3_access_key: Option<String>,
    #[serde(default)]
    pub s3_secret_key: Option<String>,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default)]
    pub destination_type: Option<String>,
}

#[utoipa::path(
    put,
    path = "/backup/jobs/{id}",
    params(("id" = String, Path, description = "Job ID")),
    request_body = UpdateJobRequest,
    responses(
        (status = 200, description = "Job updated", body = ActionResponse),
        (status = 404, description = "Job not found", body = ActionResponse)
    )
)]
pub async fn backup_jobs_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateJobRequest>,
) -> impl IntoResponse {
    for (app_id, mut st) in config::list_installed_apps_with_state(&state.data_dir) {
        if let Some(job) = st.backup_jobs.iter_mut().find(|j| j.id == id) {
            if let Some(ref r) = body.repository { job.repository = Some(r.clone()); }
            if let Some(ref p) = body.password { job.password = Some(p.clone()); }
            if let Some(ref k) = body.s3_access_key { job.s3_access_key = Some(k.clone()); }
            if let Some(ref k) = body.s3_secret_key { job.s3_secret_key = Some(k.clone()); }
            if body.schedule.is_some() { job.schedule = body.schedule.clone(); }
            if let Some(ref dt) = body.destination_type { job.destination_type = dt.clone(); }
            if let Err(e) = config::save_app_state(&state.data_dir, &app_id, &st) {
                tracing::warn!("Failed to save app state for {app_id}: {e}");
            }
            return action_ok("Job updated").into_response();
        }
    }
    action_err(StatusCode::NOT_FOUND, "Job not found").into_response()
}

#[utoipa::path(
    delete,
    path = "/backup/jobs/{id}",
    params(("id" = String, Path, description = "Job ID")),
    responses(
        (status = 200, description = "Job deleted", body = ActionResponse),
        (status = 404, description = "Job not found", body = ActionResponse)
    )
)]
pub async fn backup_jobs_delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    for (app_id, mut st) in config::list_installed_apps_with_state(&state.data_dir) {
        let before = st.backup_jobs.len();
        st.backup_jobs.retain(|j| j.id != id);
        if st.backup_jobs.len() < before {
            if let Err(e) = config::save_app_state(&state.data_dir, &app_id, &st) {
                tracing::warn!("Failed to save app state for {app_id}: {e}");
            }
            return action_ok("Job deleted").into_response();
        }
    }
    action_err(StatusCode::NOT_FOUND, "Job not found").into_response()
}

#[utoipa::path(
    post,
    path = "/backup/jobs/{id}/run",
    params(("id" = String, Path, description = "Job ID")),
    responses(
        (status = 200, description = "Job started", body = ActionResponse),
        (status = 404, description = "Job not found", body = ActionResponse)
    )
)]
pub async fn backup_jobs_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Find the job and its app
    let mut found_app: Option<String> = None;
    for (app_id, st) in config::list_installed_apps_with_state(&state.data_dir) {
        if st.backup_jobs.iter().any(|j| j.id == id) {
            found_app = Some(app_id);
            break;
        }
    }

    let app_id = match found_app {
        Some(id) => id,
        None => return action_err(StatusCode::NOT_FOUND, "Job not found").into_response(),
    };

    // Skip if already running
    {
        let map = state.backup_progress.read().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = map.get(&id) {
            if p.status == "running" {
                return action_ok("Job already running").into_response();
            }
        }
    }

    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let progress = state.backup_progress.clone();
    let cancel = state.backup_cancel.clone();
    let registry = state.registry.clone();
    let data_dir = state.data_dir.clone();
    let job_id = id.clone();

    // Run async — return immediately
    tokio::spawn(async move {
        let _ = backup::backup_job_run(
            &data_dir,
            &app_id,
            &job_id,
            &registry,
            &global_config,
            &progress,
            &cancel,
        )
        .await;
    });

    action_ok("Job started").into_response()
}

#[utoipa::path(
    post,
    path = "/backup/jobs/{id}/cancel",
    params(("id" = String, Path, description = "Job ID")),
    responses(
        (status = 200, description = "Job cancelled", body = ActionResponse),
        (status = 404, description = "No running job", body = ActionResponse)
    )
)]
pub async fn backup_jobs_cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Check if the job is actually running
    let is_running = {
        let map = state.backup_progress.read().unwrap_or_else(|e| e.into_inner());
        map.get(&id).map_or(false, |p| p.status == "running")
    };
    if !is_running {
        return action_err(StatusCode::NOT_FOUND, "No running backup for this job").into_response();
    }

    // Mark for cancellation
    {
        let mut set = state.backup_cancel.write().unwrap_or_else(|e| e.into_inner());
        set.insert(id.clone());
    }

    // Stop the restic container (named myground-backup-{job_id})
    let container_name = format!("myground-backup-{id}");
    let _ = tokio::process::Command::new("docker")
        .args(["stop", "-t", "5", &container_name])
        .output()
        .await;

    action_ok("Backup cancellation requested").into_response()
}

#[utoipa::path(
    get,
    path = "/backup/jobs/{id}/progress",
    params(("id" = String, Path, description = "Job ID")),
    responses(
        (status = 200, description = "Job progress", body = BackupJobProgress),
        (status = 404, description = "No active progress", body = ActionResponse)
    )
)]
pub async fn backup_jobs_progress(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let map = state.backup_progress.read().unwrap_or_else(|e| e.into_inner());
    match map.get(&id) {
        Some(p) => Json(p.clone()).into_response(),
        None => action_err(StatusCode::NOT_FOUND, "No active progress for this job").into_response(),
    }
}

// ── Restore progress ────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/backup/restore/{restore_id}/progress",
    params(("restore_id" = String, Path, description = "Restore ID")),
    responses(
        (status = 200, description = "Restore progress", body = RestoreProgress),
        (status = 404, description = "No active restore", body = ActionResponse)
    )
)]
pub async fn restore_progress(
    State(state): State<AppState>,
    Path(restore_id): Path<String>,
) -> impl IntoResponse {
    let map = state.restore_progress.read().unwrap_or_else(|e| e.into_inner());
    match map.get(&restore_id) {
        Some(p) => Json(p.clone()).into_response(),
        None => action_err(StatusCode::NOT_FOUND, "No active restore with this ID").into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/backup/restores",
    responses(
        (status = 200, description = "Active/recent restore operations", body = Vec<RestoreProgress>)
    )
)]
pub async fn restore_list(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let map = state.restore_progress.read().unwrap_or_else(|e| e.into_inner());
    let restores: Vec<RestoreProgress> = map.values().cloned().collect();
    Json(restores).into_response()
}

// ── Verify ──────────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/backup/verify",
    request_body = BackupConfig,
    responses(
        (status = 200, description = "Verification result", body = VerifyResult)
    )
)]
pub async fn backup_verify(
    Json(body): Json<BackupConfig>,
) -> impl IntoResponse {
    Json(backup::verify_repo(&body).await).into_response()
}
