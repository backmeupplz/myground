mod backup;
mod browse;
mod config;
mod deploy;
mod disks;
mod docker;
mod health;
mod logs;
pub mod response;
pub mod services;
mod stats;

use axum::http::StatusCode;
use axum::Router;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

use crate::backup::{BackupResult, Snapshot};
use crate::config::{BackupConfig, GlobalConfig, ServiceBackupConfig};
use crate::disk::{DiskInfo, SmartHealth};
use crate::docker::ContainerStatus;
use crate::stats::SystemStats;

use self::browse::{BrowseResult, DirEntry};
use crate::registry::{DbDumpConfig, InstallVariable, ServiceMetadata};
use crate::state::AppState;
use crate::web::static_handler;

use self::backup::RestoreRequest;
use self::health::HealthResponse;
use self::response::ActionResponse;
use self::services::{AvailableService, InstallRequest, InstallResponse, RenameRequest, ServiceInfo, StorageVolumeStatus};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "MyGround API",
        description = "Self-hosting platform API",
        version = "0.1.0"
    ),
    servers(
        (url = "/api")
    ),
    components(schemas(
        HealthResponse,
        crate::docker::DockerStatus,
        ServiceMetadata,
        AvailableService,
        ServiceInfo,
        ContainerStatus,
        StorageVolumeStatus,
        DiskInfo,
        SmartHealth,
        ActionResponse,
        BackupConfig,
        ServiceBackupConfig,
        Snapshot,
        BackupResult,
        RestoreRequest,
        DbDumpConfig,
        InstallVariable,
        InstallRequest,
        InstallResponse,
        RenameRequest,
        SystemStats,
        BrowseResult,
        DirEntry,
        GlobalConfig,
    ))
)]
struct ApiDoc;

async fn api_fallback() -> StatusCode {
    StatusCode::NOT_FOUND
}

pub fn build_router(state: AppState) -> Router {
    let (api_router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::health))
        .routes(routes!(docker::docker_status))
        .routes(routes!(services::services_available))
        .routes(routes!(services::services_list))
        .routes(routes!(services::service_install))
        .routes(routes!(services::service_start))
        .routes(routes!(services::service_stop))
        .routes(routes!(services::service_remove))
        .routes(routes!(services::service_storage_update))
        .routes(routes!(services::service_backup_config_get, services::service_backup_config_update))
        .routes(routes!(services::service_backup_snapshots))
        .routes(routes!(services::service_backup_run))
        .routes(routes!(services::service_dismiss_credentials))
        .routes(routes!(services::service_dismiss_backup_password))
        .routes(routes!(services::service_rename))
        .routes(routes!(stats::system_stats))
        .routes(routes!(config::global_config_get, config::global_config_update))
        .routes(routes!(browse::browse))
        .routes(routes!(disks::disks_list))
        .routes(routes!(disks::disks_smart))
        .routes(routes!(backup::backup_config_get))
        .routes(routes!(backup::backup_config_update))
        .routes(routes!(backup::backup_init))
        .routes(routes!(backup::backup_run_all))
        .routes(routes!(backup::backup_run_service))
        .routes(routes!(backup::backup_snapshots))
        .routes(routes!(backup::backup_restore))
        .routes(routes!(backup::backup_prune))
        .split_for_parts();

    let api_with_fallback: Router<AppState> = api_router.fallback(api_fallback);

    let ws_routes = Router::new()
        .route("/api/services/{id}/logs", axum::routing::get(logs::service_logs))
        .route("/api/services/{id}/deploy", axum::routing::get(deploy::service_deploy));

    Router::new()
        .nest("/api", api_with_fallback)
        .merge(ws_routes)
        .merge(SwaggerUi::new("/api-docs").url("/api-docs/openapi.json", api))
        .fallback(static_handler)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(state: AppState, address: &str, port: u16) {
    let app = build_router(state);

    let bind = format!("{address}:{port}");
    tracing::info!("MyGround starting on http://{bind}");
    tracing::info!("API docs at http://{bind}/api-docs");

    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
