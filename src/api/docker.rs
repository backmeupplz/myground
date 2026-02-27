use axum::extract::State;
use axum::Json;

use crate::docker;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/docker/status",
    responses(
        (status = 200, description = "Docker daemon status", body = docker::DockerStatus)
    )
)]
pub async fn docker_status(State(state): State<AppState>) -> Json<docker::DockerStatus> {
    Json(docker::get_status(&state.docker).await)
}
