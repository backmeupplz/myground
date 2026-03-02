use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use crate::config::{self, GlobalConfig};
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

#[utoipa::path(
    get,
    path = "/config",
    responses(
        (status = 200, description = "Global configuration", body = GlobalConfig)
    )
)]
pub async fn global_config_get(State(state): State<AppState>) -> impl IntoResponse {
    match config::load_global_config(&state.data_dir) {
        Ok(mut cfg) => {
            // Redact sensitive fields from API response
            cfg.auth = None;
            if let Some(ref mut ts) = cfg.tailscale {
                ts.auth_key = ts.auth_key.as_ref().map(|_| "***".to_string());
            }
            if let Some(ref mut backup) = cfg.backup {
                backup.password = backup.password.as_ref().map(|_| "***".to_string());
                backup.s3_secret_key = backup.s3_secret_key.as_ref().map(|_| "***".to_string());
            }
            Json(cfg).into_response()
        }
        Err(e) => action_err(axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/config",
    request_body = GlobalConfig,
    responses(
        (status = 200, description = "Configuration updated", body = ActionResponse),
        (status = 400, description = "Update error", body = ActionResponse)
    )
)]
pub async fn global_config_update(
    State(state): State<AppState>,
    Json(body): Json<GlobalConfig>,
) -> impl IntoResponse {
    // Load existing config and only allow updating safe fields.
    // Auth and Tailscale must be modified through their own endpoints.
    let existing = match config::load_global_config(&state.data_dir) {
        Ok(c) => c,
        Err(e) => {
            return action_err(axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    };

    let safe_config = GlobalConfig {
        version: body.version,
        default_storage_path: body.default_storage_path,
        backup: body.backup,
        auth: existing.auth,         // preserve — cannot be changed via this endpoint
        tailscale: existing.tailscale, // preserve — cannot be changed via this endpoint
        cloudflare: existing.cloudflare, // preserve — cannot be changed via this endpoint
    };

    match config::save_global_config(&state.data_dir, &safe_config) {
        Ok(()) => action_ok("Configuration saved").into_response(),
        Err(e) => action_err(axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
