use axum::Json;

use crate::disk::{DiskInfo, SmartHealth};

#[utoipa::path(
    get,
    path = "/disks",
    responses(
        (status = 200, description = "List all mounted disks", body = Vec<DiskInfo>)
    )
)]
pub async fn disks_list() -> Json<Vec<DiskInfo>> {
    Json(crate::disk::list_disks())
}

#[utoipa::path(
    get,
    path = "/disks/smart",
    responses(
        (status = 200, description = "SMART health for all disks", body = Vec<SmartHealth>)
    )
)]
pub async fn disks_smart() -> Json<Vec<SmartHealth>> {
    Json(crate::disk::smart_health_all())
}
