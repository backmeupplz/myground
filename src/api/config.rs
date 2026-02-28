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
        Ok(cfg) => Json(cfg).into_response(),
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
    match config::save_global_config(&state.data_dir, &body) {
        Ok(()) => action_ok("Configuration saved").into_response(),
        Err(e) => action_err(axum::http::StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
