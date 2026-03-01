pub mod auth;
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
pub mod tailscale;

use axum::extract::State;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

use crate::backup::{BackupResult, Snapshot};
use crate::config::{AuthConfig, BackupConfig, GlobalConfig, ServiceBackupConfig, TailscaleConfig};
use crate::disk::{DiskInfo, SmartHealth};
use crate::docker::ContainerStatus;
use crate::stats::SystemStats;

use self::auth::{ApiKeyInfo, AuthStatus, CreateApiKeyRequest, CreateApiKeyResponse, LoginRequest, LoginResponse, SetupRequest};
use self::browse::{BrowseResult, DirEntry};
use self::tailscale::{TailscaleConfigRequest, TailscaleServiceInfo, TailscaleStatus};
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
        AuthConfig,
        AuthStatus,
        LoginRequest,
        LoginResponse,
        SetupRequest,
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
        TailscaleConfig,
        TailscaleStatus,
        TailscaleServiceInfo,
        TailscaleConfigRequest,
        crate::config::ApiKeyEntry,
        ApiKeyInfo,
        CreateApiKeyRequest,
        CreateApiKeyResponse,
    ))
)]
struct ApiDoc;

async fn api_fallback() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Auth middleware: allows /auth/*, /health through; everything else requires a session.
async fn auth_middleware(
    State(state): State<AppState>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let path = req.uri().path();

    // Non-API routes (frontend static files) are always allowed
    if !path.starts_with("/api/") {
        return next.run(req).await;
    }

    // Auth status and health are always public
    if path == "/api/auth/status" || path == "/api/health" {
        return next.run(req).await;
    }

    // If no auth is configured, ONLY allow setup — block everything else
    if crate::config::load_auth_config(&state.data_dir)
        .unwrap_or(None)
        .is_none()
    {
        if path == "/api/auth/setup" {
            return next.run(req).await;
        }
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({"ok": false, "message": "Setup required"})),
        )
            .into_response();
    }

    // Auth login/logout are accessible without a session (but require auth to be configured)
    if path == "/api/auth/login" || path == "/api/auth/logout" {
        return next.run(req).await;
    }

    // Check session cookie
    let session_valid = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(crate::auth::extract_session_from_cookies)
        .map(|token| state.sessions.read().unwrap().contains(token))
        .unwrap_or(false);

    if session_valid {
        return next.run(req).await;
    }

    // Check Authorization: Bearer {api-key}
    let bearer_token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(crate::auth::extract_bearer_token);

    if let Some(key) = bearer_token {
        // Rate-limit bearer auth failures
        if state
            .login_attempts
            .read()
            .unwrap()
            .is_blocked("__bearer__")
        {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({"ok": false, "message": "Too many failed attempts. Try again later."})),
            )
                .into_response();
        }

        let valid = if let Ok(Some(auth_cfg)) = crate::config::load_auth_config(&state.data_dir) {
            auth_cfg
                .api_keys
                .iter()
                .any(|entry| crate::auth::verify_password(key, &entry.key_hash))
        } else {
            false
        };

        if valid {
            state.login_attempts.write().unwrap().clear("__bearer__");
            return next.run(req).await;
        }

        state
            .login_attempts
            .write()
            .unwrap()
            .record_failure("__bearer__");
    }

    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({"ok": false, "message": "Not authenticated"})),
    )
        .into_response()
}

/// Add security headers to all responses.
async fn security_headers(
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> axum::response::Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert(
        "content-security-policy",
        "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:"
            .parse()
            .unwrap(),
    );
    resp
}

pub fn build_router(state: AppState) -> Router {
    let (api_router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::health))
        .routes(routes!(auth::auth_status))
        .routes(routes!(auth::auth_setup))
        .routes(routes!(auth::auth_login))
        .routes(routes!(auth::auth_logout))
        .routes(routes!(auth::api_keys_list, auth::api_keys_create))
        .routes(routes!(auth::api_keys_revoke))
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
        .routes(routes!(tailscale::tailscale_status))
        .routes(routes!(tailscale::tailscale_config_update))
        .routes(routes!(tailscale::tailscale_refresh))
        .split_for_parts();

    let api_with_fallback: Router<AppState> = api_router.fallback(api_fallback);

    let ws_routes = Router::new()
        .route("/api/services/{id}/logs", axum::routing::get(logs::service_logs))
        .route("/api/services/{id}/deploy", axum::routing::get(deploy::service_deploy));

    // Restrictive CORS: frontend is same-origin, only allow essential headers
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::COOKIE, header::AUTHORIZATION])
        .allow_credentials(false);

    Router::new()
        .nest("/api", api_with_fallback)
        .merge(ws_routes)
        .merge(SwaggerUi::new("/api/docs").url("/api/docs/openapi.json", api))
        .fallback(static_handler)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(cors)
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(state)
}

pub async fn serve(state: AppState, address: &str, port: u16) {
    crate::scheduler::spawn(state.clone());

    // Auto-start TSDProxy if Tailscale is enabled
    if let Ok(Some(ts_cfg)) = crate::config::load_tailscale_config(&state.data_dir) {
        if ts_cfg.enabled {
            if let Err(e) = crate::tailscale::ensure_tsdproxy(&state.data_dir).await {
                tracing::warn!("Failed to auto-start TSDProxy: {e}");
            }
        }
    }

    let app = build_router(state);

    let bind = format!("{address}:{port}");
    tracing::info!("MyGround starting on http://{bind}");
    tracing::info!("API docs at http://{bind}/api-docs");

    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
