use axum::Json;

use crate::stats::SystemStats;

#[utoipa::path(
    get,
    path = "/stats",
    responses(
        (status = 200, description = "System resource stats", body = SystemStats)
    )
)]
pub async fn system_stats() -> Json<SystemStats> {
    Json(crate::stats::get_stats())
}
