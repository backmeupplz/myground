use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, UpdateConfig};
use crate::state::AppState;
use crate::updates;

use super::response::{action_err, action_ok, ActionResponse};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AppUpdateInfo {
    pub id: String,
    pub name: String,
    pub update_available: bool,
    pub last_check: Option<String>,
    /// Full repo digest of the currently running image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_digest: Option<String>,
    /// Full repo digest of the latest available image when an update exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_digest: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct UpdateStatus {
    pub myground_version: String,
    pub latest_myground_version: Option<String>,
    pub myground_update_available: bool,
    pub apps: Vec<AppUpdateInfo>,
    pub last_check: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateConfigRequest {
    pub auto_update_apps: bool,
    pub auto_update_myground: bool,
}

// ── GET /updates/status ────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/updates/status",
    responses(
        (status = 200, description = "Update status", body = UpdateStatus)
    )
)]
pub async fn update_status(State(state): State<AppState>) -> Json<UpdateStatus> {
    let global = config::load_global_config(&state.data_dir).unwrap_or_default();
    let updates_cfg = global.updates.unwrap_or_default();

    let installed = config::list_installed_apps(&state.data_dir);
    let apps: Vec<AppUpdateInfo> = installed
        .iter()
        .filter_map(|id| {
            let svc_state = config::load_app_state(&state.data_dir, id).ok()?;
            if !svc_state.installed {
                return None;
            }
            let name = svc_state
                .display_name
                .as_deref()
                .or_else(|| {
                    state
                        .registry
                        .get(id)
                        .map(|def| def.metadata.name.as_str())
                })
                .unwrap_or(id)
                .to_string();
            Some(AppUpdateInfo {
                id: id.clone(),
                name,
                update_available: svc_state.update_available,
                last_check: svc_state.last_update_check,
                current_digest: svc_state.image_digest.clone(),
                latest_digest: svc_state.latest_image_digest.clone(),
            })
        })
        .collect();

    let myground_update_available = updates_cfg
        .latest_myground_version
        .as_ref()
        .map(|v| v != env!("CARGO_PKG_VERSION"))
        .unwrap_or(false);

    Json(UpdateStatus {
        myground_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_myground_version: updates_cfg.latest_myground_version,
        myground_update_available,
        apps,
        last_check: updates_cfg.last_check,
    })
}

// ── POST /updates/check ────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/updates/check",
    responses(
        (status = 200, description = "Check triggered", body = ActionResponse)
    )
)]
pub async fn update_check(State(state): State<AppState>) -> Json<ActionResponse> {
    let data_dir = state.data_dir.clone();
    let registry = state.registry.clone();

    tokio::spawn(async move {
        let (svc_count, mg_update) = updates::check_all_updates(&data_dir, &registry).await;
        tracing::info!(
            "Update check complete: {svc_count} app update(s), myground update: {mg_update}"
        );
    });

    action_ok("Update check started".to_string())
}

// ── POST /updates/update-all ───────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/updates/update-all",
    responses(
        (status = 200, description = "Updates started", body = ActionResponse)
    )
)]
pub async fn update_all(State(state): State<AppState>) -> Json<ActionResponse> {
    let data_dir = state.data_dir.clone();

    tokio::spawn(async move {
        let installed = config::list_installed_apps(&data_dir);
        for id in &installed {
            let svc_state = match config::load_app_state(&data_dir, id) {
                Ok(s) if s.update_available => s,
                _ => continue,
            };
            drop(svc_state);
            tracing::info!("Auto-updating app {id}");
            if let Err(e) = updates::update_app(&data_dir, id).await {
                tracing::error!("Failed to update app {id}: {e}");
            }
        }
    });

    action_ok("Updating all apps".to_string())
}

// ── POST /updates/self-update ──────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/updates/self-update",
    responses(
        (status = 200, description = "Self-update started", body = ActionResponse),
        (status = 400, description = "No update available", body = ActionResponse)
    )
)]
pub async fn self_update(State(state): State<AppState>) -> impl IntoResponse {
    let global = config::load_global_config(&state.data_dir).unwrap_or_default();
    let url = global
        .updates
        .as_ref()
        .and_then(|u| u.latest_myground_url.clone());

    match url {
        Some(download_url) => {
            tokio::spawn(async move {
                if let Err(e) = updates::self_update(&download_url).await {
                    tracing::error!("Self-update failed: {e}");
                }
            });
            action_ok("Self-update started — MyGround will restart".to_string()).into_response()
        }
        None => action_err(
            StatusCode::BAD_REQUEST,
            "No update URL available. Run a check first.".to_string(),
        )
        .into_response(),
    }
}

// ── GET /updates/config ────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/updates/config",
    responses(
        (status = 200, description = "Update config", body = UpdateConfig)
    )
)]
pub async fn update_config_get(State(state): State<AppState>) -> Json<UpdateConfig> {
    let global = config::load_global_config(&state.data_dir).unwrap_or_default();
    Json(global.updates.unwrap_or_default())
}

// ── PUT /updates/config ────────────────────────────────────────────────────

#[utoipa::path(
    put,
    path = "/updates/config",
    request_body = UpdateConfigRequest,
    responses(
        (status = 200, description = "Config saved", body = ActionResponse),
        (status = 400, description = "Save error", body = ActionResponse)
    )
)]
pub async fn update_config_update(
    State(state): State<AppState>,
    Json(body): Json<UpdateConfigRequest>,
) -> impl IntoResponse {
    let mut global = match config::load_global_config(&state.data_dir) {
        Ok(g) => g,
        Err(e) => return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    let updates = global.updates.get_or_insert_with(UpdateConfig::default);
    updates.auto_update_apps = body.auto_update_apps;
    updates.auto_update_myground = body.auto_update_myground;

    match config::save_global_config(&state.data_dir, &global) {
        Ok(()) => action_ok("Update config saved".to_string()).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── WebSocket: /apps/{id}/update ────────────────────────────────────────────

pub async fn app_update_ws(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    let guard = match state.try_ws_slot(&id) {
        Some(g) => g,
        None => {
            return action_err(StatusCode::TOO_MANY_REQUESTS, "Too many update connections")
                .into_response()
        }
    };
    ws.on_upgrade(move |socket| handle_update_stream(socket, state, id, guard))
        .into_response()
}

async fn handle_update_stream(
    mut socket: WebSocket,
    state: AppState,
    app_id: String,
    _guard: crate::state::WsGuard,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);

    let data_dir = state.data_dir.clone();
    let sid = app_id.clone();
    let update_task = tokio::spawn(async move {
        updates::update_app_streaming(&data_dir, &sid, tx).await
    });

    // Forward lines from the channel to the WebSocket
    while let Some(line) = rx.recv().await {
        if socket.send(Message::Text(line.into())).await.is_err() {
            break;
        }
    }

    // Wait for update to finish and send result
    match update_task.await {
        Ok(Ok(())) => {
            let _ = socket.send(Message::Text("__DONE__".into())).await;
        }
        Ok(Err(e)) => {
            let _ = socket
                .send(Message::Text(format!("Error: {e}").into()))
                .await;
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error: {e}").into()))
                .await;
        }
    }
}
