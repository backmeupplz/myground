use axum::extract::State;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailnet_name: Option<String>,
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Health check", body = HealthResponse)
    )
)]
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let ts_cfg = crate::config::try_load_tailscale(&state.data_dir);
    let tailnet_name = if ts_cfg.enabled {
        ts_cfg.tailnet
    } else {
        None
    };
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        server_ip: crate::stats::get_server_ip(),
        tailnet_name,
    })
}
