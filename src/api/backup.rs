use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::backup::{self, BackupResult, Snapshot};
use crate::config::{self, BackupConfig};
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

/// Load backup config or return a 400 error response.
fn require_backup_config(state: &AppState) -> Result<BackupConfig, axum::response::Response> {
    match config::load_backup_config(&state.data_dir) {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(action_err(StatusCode::BAD_REQUEST, "No backup config set").into_response()),
        Err(e) => Err(action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()),
    }
}

// ── Backup config ──────────────────────────────────────────────────────────

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

// ── Run backups ────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/backup/run",
    responses(
        (status = 200, description = "All services backed up", body = Vec<BackupResult>),
        (status = 400, description = "Backup error", body = ActionResponse)
    )
)]
pub async fn backup_run_all(State(state): State<AppState>) -> impl IntoResponse {
    let backup_config = match require_backup_config(&state) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_all(&state.data_dir, &state.registry, &global_config, &backup_config).await {
        Ok(results) => Json(results).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/backup/run/{id}",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service backed up", body = Vec<BackupResult>),
        (status = 400, description = "Backup error", body = ActionResponse),
        (status = 404, description = "Service not found", body = ActionResponse)
    )
)]
pub async fn backup_run_service(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !state.registry.contains_key(&id) {
        return action_err(StatusCode::NOT_FOUND, format!("Unknown service: {id}")).into_response();
    }

    // Use global backup config if available, otherwise fall back to empty default.
    // The service may have per-service config that doesn't require global config.
    let backup_config = config::load_backup_config(&state.data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_service(&state.data_dir, &id, &state.registry, &global_config, &backup_config).await {
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

#[derive(Deserialize, ToSchema)]
pub struct RestoreRequest {
    pub target_path: String,
}

#[utoipa::path(
    post,
    path = "/backup/restore/{snapshot_id}",
    params(("snapshot_id" = String, Path, description = "Snapshot ID to restore")),
    request_body = RestoreRequest,
    responses(
        (status = 200, description = "Snapshot restored", body = ActionResponse),
        (status = 400, description = "Restore error", body = ActionResponse)
    )
)]
pub async fn backup_restore(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
    Json(body): Json<RestoreRequest>,
) -> impl IntoResponse {
    let config = match require_backup_config(&state) {
        Ok(c) => c,
        Err(r) => return r,
    };

    match backup::restore_snapshot(&body.target_path, &snapshot_id, &config).await {
        Ok(_) => action_ok(format!("Snapshot {snapshot_id} restored")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
