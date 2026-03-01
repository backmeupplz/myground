use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, TailscaleConfig};
use crate::state::AppState;
use crate::tailscale;

use super::response::{action_err, action_ok};

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct TailscaleStatus {
    pub enabled: bool,
    pub running: bool,
    pub tailnet: Option<String>,
    pub services: Vec<TailscaleServiceInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct TailscaleServiceInfo {
    pub service_id: String,
    pub hostname: String,
    pub url: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct TailscaleConfigRequest {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auth_key: Option<String>,
}

// ── Endpoints ───────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/tailscale/status",
    responses(
        (status = 200, description = "Tailscale status", body = TailscaleStatus)
    )
)]
pub async fn tailscale_status(State(state): State<AppState>) -> Json<TailscaleStatus> {
    let ts_cfg = config::try_load_tailscale(&state.data_dir);

    let running = if ts_cfg.enabled {
        tailscale::is_tsdproxy_running().await
    } else {
        false
    };

    // Try to detect tailnet if running but not yet known
    let tailnet = if running && ts_cfg.tailnet.is_none() {
        let detected = tailscale::detect_tailnet().await;
        if let Some(ref tn) = detected {
            // Persist the detected tailnet
            let mut updated = ts_cfg.clone();
            updated.tailnet = Some(tn.clone());
            let _ = config::save_tailscale_config(&state.data_dir, &updated);
        }
        detected
    } else {
        ts_cfg.tailnet.clone()
    };

    // Build per-service info
    let installed = config::list_installed_services(&state.data_dir);
    let services: Vec<TailscaleServiceInfo> = if ts_cfg.enabled {
        installed
            .iter()
            .map(|id| {
                let url = tailnet
                    .as_ref()
                    .map(|tn| format!("https://{id}.{tn}"));
                TailscaleServiceInfo {
                    service_id: id.clone(),
                    hostname: id.clone(),
                    url,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    Json(TailscaleStatus {
        enabled: ts_cfg.enabled,
        running,
        tailnet,
        services,
    })
}

#[utoipa::path(
    put,
    path = "/tailscale/config",
    request_body = TailscaleConfigRequest,
    responses(
        (status = 200, description = "Config saved", body = super::response::ActionResponse),
        (status = 400, description = "Error", body = super::response::ActionResponse)
    )
)]
pub async fn tailscale_config_update(
    State(state): State<AppState>,
    Json(body): Json<TailscaleConfigRequest>,
) -> impl IntoResponse {
    let existing = config::try_load_tailscale(&state.data_dir);

    let ts_cfg = TailscaleConfig {
        enabled: body.enabled,
        auth_key: body.auth_key.or(existing.auth_key),
        tailnet: existing.tailnet,
    };

    if let Err(e) = config::save_tailscale_config(&state.data_dir, &ts_cfg) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    // Start or stop TSDProxy based on enabled state
    if ts_cfg.enabled {
        if let Err(e) = tailscale::ensure_tsdproxy(&state.data_dir).await {
            return action_err(StatusCode::BAD_REQUEST, format!("Start TSDProxy: {e}"))
                .into_response();
        }
    } else {
        let _ = tailscale::stop_tsdproxy(&state.data_dir).await;
    }

    action_ok("Tailscale config saved".to_string()).into_response()
}

#[utoipa::path(
    post,
    path = "/tailscale/refresh",
    responses(
        (status = 200, description = "Services refreshed", body = super::response::ActionResponse),
        (status = 400, description = "Error", body = super::response::ActionResponse)
    )
)]
pub async fn tailscale_refresh(State(state): State<AppState>) -> impl IntoResponse {
    let ts_cfg = config::try_load_tailscale(&state.data_dir);

    let installed = config::list_installed_services(&state.data_dir);
    let mut refreshed = 0u32;

    for id in &installed {
        let svc_dir = config::service_dir(&state.data_dir, id);
        let compose_path = svc_dir.join("docker-compose.yml");
        if !compose_path.exists() {
            continue;
        }

        let Ok(yaml) = std::fs::read_to_string(&compose_path) else {
            tracing::warn!("Skipping {id}: failed to read compose");
            continue;
        };

        let new_yaml = if ts_cfg.enabled {
            // Inject labels
            let port = tailscale::extract_container_port(&yaml).unwrap_or(80);
            match tailscale::inject_tsdproxy_labels(&yaml, id, port) {
                Ok(y) => y,
                Err(e) => {
                    tracing::warn!("Label inject failed for {id}: {e}");
                    continue;
                }
            }
        } else {
            // Remove labels
            match tailscale::remove_tsdproxy_labels(&yaml) {
                Ok(y) => y,
                Err(e) => {
                    tracing::warn!("Label removal failed for {id}: {e}");
                    continue;
                }
            }
        };

        if std::fs::write(&compose_path, &new_yaml).is_ok() {
            refreshed += 1;
            // Restart service to pick up label changes
            let svc_dir_clone = svc_dir.clone();
            if let Ok(compose_cmd) = crate::compose::detect_command().await {
                if let Err(e) = crate::compose::run(&compose_cmd, &svc_dir_clone, &["up", "-d"]).await {
                    tracing::warn!("Compose up failed for {id}: {e}");
                }
            }
        }
    }

    action_ok(format!("Refreshed {refreshed} service(s)")).into_response()
}
