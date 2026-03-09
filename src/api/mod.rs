pub mod auth;
mod backup;
mod browse;
pub mod cloudflare;
mod config;
mod deploy;
mod disks;
mod docker;
mod health;
mod logs;
pub mod response;
pub mod apps;
mod stats;
pub mod tailscale;
pub mod updates;

use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use axum::response::Html;

use crate::aws::{AwsSetupRequest, AwsSetupResult};
use crate::backup::{BackupResult, Snapshot, SnapshotFile, VerifyResult};
use crate::config::{AuthConfig, BackupConfig, BackupJob, CloudflareConfig, DomainBinding, GlobalConfig, AppBackupConfig, TailscaleConfig, UpdateConfig};
use crate::disk::{DiskInfo, SmartHealth};
use crate::docker::ContainerStatus;
use crate::stats::SystemStats;

use self::auth::{ApiKeyInfo, AuthStatus, CreateApiKeyRequest, CreateApiKeyResponse, LoginRequest, LoginResponse, SetupRequest};
use self::browse::{BrowseResult, DirEntry, MkdirRequest};
use self::cloudflare::{BindDomainRequest, CloudflareBinding, CloudflareConfigRequest, CloudflareStatus};
use self::tailscale::{AppTailscaleRequest, TailscaleConfigRequest, TailscaleAppInfo, TailscaleStatus};
use crate::registry::{DbDumpConfig, InstallVariable, AppMetadata, StorageVolume};
use crate::state::AppState;
use crate::web::static_handler;

use self::backup::{RestoreRequest, RestoreStartResponse, BackupJobWithApp, CreateJobRequest, UpdateJobRequest};
use self::health::HealthResponse;
use self::response::ActionResponse;
use self::apps::{AvailableApp, BackupPasswordResponse, GpuRequest, InstallRequest, InstallResponse, LanAccessRequest, RenameRequest, AppInfo, StorageVolumeStatus};
use crate::config::VpnConfig;
use self::updates::{AppUpdateInfo, UpdateConfigRequest, UpdateStatus};

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
        AppMetadata,
        AvailableApp,
        AppInfo,
        ContainerStatus,
        StorageVolumeStatus,
        StorageVolume,
        DiskInfo,
        SmartHealth,
        ActionResponse,
        AuthConfig,
        AuthStatus,
        LoginRequest,
        LoginResponse,
        SetupRequest,
        BackupConfig,
        AppBackupConfig,
        Snapshot,
        BackupResult,
        RestoreRequest,
        DbDumpConfig,
        InstallVariable,
        InstallRequest,
        InstallResponse,
        RenameRequest,
        LanAccessRequest,
        GpuRequest,
        BackupPasswordResponse,
        SystemStats,
        BrowseResult,
        DirEntry,
        MkdirRequest,
        GlobalConfig,
        TailscaleConfig,
        TailscaleStatus,
        TailscaleAppInfo,
        TailscaleConfigRequest,
        AppTailscaleRequest,
        crate::config::ApiKeyEntry,
        ApiKeyInfo,
        CreateApiKeyRequest,
        CreateApiKeyResponse,
        UpdateConfig,
        UpdateStatus,
        AppUpdateInfo,
        UpdateConfigRequest,
        CloudflareConfig,
        CloudflareStatus,
        CloudflareBinding,
        CloudflareConfigRequest,
        BindDomainRequest,
        DomainBinding,
        crate::cloudflare::CfZone,
        VpnConfig,
        AwsSetupRequest,
        AwsSetupResult,
        BackupJob,
        VerifyResult,
        SnapshotFile,
        crate::state::BackupJobProgress,
        crate::state::RestoreProgress,
        RestoreStartResponse,
        BackupJobWithApp,
        CreateJobRequest,
        UpdateJobRequest,
    ))
)]
struct ApiDoc;

async fn api_fallback() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Auth middleware: allows /auth/*, /health through; everything else requires a session.
/// Extract client IP from request for rate limiting.
fn client_ip(req: &axum::http::Request<axum::body::Body>) -> String {
    // Check X-Forwarded-For first (for reverse-proxied setups)
    if let Some(xff) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first_ip) = xff.split(',').next() {
            let ip = first_ip.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    // Fallback to peer address from connection info
    if let Some(addr) = req.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>() {
        return addr.0.ip().to_string();
    }
    "unknown".to_string()
}

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

    // Auth status, health, and API docs are always public
    if path == "/api/auth/status"
        || path == "/api/health"
        || path.starts_with("/api/docs")
    {
        return next.run(req).await;
    }

    // If no auth is configured, ONLY allow setup — block everything else
    if crate::config::try_load_auth(&state.data_dir).is_none() {
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
        .map(|token| state.sessions.read().unwrap_or_else(|e| e.into_inner()).contains(token))
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
        // Rate-limit bearer auth failures per client IP
        let rate_key = format!("bearer:{}", client_ip(&req));
        if state
            .login_attempts
            .read()
            .unwrap()
            .is_blocked(&rate_key)
        {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({"ok": false, "message": "Too many failed attempts. Try again later."})),
            )
                .into_response();
        }

        let valid = if let Some(auth_cfg) = crate::config::try_load_auth(&state.data_dir) {
            auth_cfg
                .api_keys
                .iter()
                .any(|entry| crate::auth::verify_password(key, &entry.key_hash))
        } else {
            false
        };

        if valid {
            state.login_attempts.write().unwrap_or_else(|e| e.into_inner()).clear(&rate_key);
            return next.run(req).await;
        }

        state
            .login_attempts
            .write()
            .unwrap()
            .record_failure(&rate_key);
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
    let is_docs = req.uri().path().starts_with("/api/docs");
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    // Relax CSP for API docs page to load Swagger UI from CDN
    let csp = if is_docs {
        "default-src 'self' https://unpkg.com; style-src 'self' 'unsafe-inline' https://unpkg.com; script-src 'self' 'unsafe-inline' https://unpkg.com; img-src 'self' data:; connect-src 'self' ws: wss:"
    } else {
        "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:"
    };
    headers.insert("content-security-policy", csp.parse().unwrap());
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
        .routes(routes!(apps::apps_available))
        .routes(routes!(apps::apps_list))
        .routes(routes!(apps::app_install))
        .routes(routes!(apps::app_start))
        .routes(routes!(apps::app_stop))
        .routes(routes!(apps::app_remove))
        .routes(routes!(apps::app_storage_update))
        .routes(routes!(apps::app_backup_config_get, apps::app_backup_config_update))
        .routes(routes!(apps::app_backup_snapshots))
        .routes(routes!(apps::app_backup_run))
        .routes(routes!(apps::app_dismiss_credentials))
        .routes(routes!(apps::app_dismiss_backup_password))
        .routes(routes!(apps::app_backup_password))
        .routes(routes!(apps::app_rename))
        .routes(routes!(apps::app_lan_toggle))
        .routes(routes!(apps::app_gpu_toggle))
        .routes(routes!(apps::app_vpn_get, apps::app_vpn_update))
        .routes(routes!(apps::app_extra_folders_update))
        .routes(routes!(apps::vpn_config_get, apps::vpn_config_update))
        .routes(routes!(apps::app_icon))
        .routes(routes!(stats::system_stats))
        .routes(routes!(config::global_config_get, config::global_config_update))
        .routes(routes!(browse::browse))
        .routes(routes!(browse::mkdir))
        .routes(routes!(disks::disks_list))
        .routes(routes!(disks::disks_smart))
        .routes(routes!(backup::backup_config_get))
        .routes(routes!(backup::backup_config_update))
        .routes(routes!(backup::backup_init))
        .routes(routes!(backup::backup_run_all))
        .routes(routes!(backup::backup_run_app))
        .routes(routes!(backup::backup_snapshots))
        .routes(routes!(backup::snapshot_files))
        .routes(routes!(backup::snapshot_delete))
        .routes(routes!(backup::backup_restore))
        .routes(routes!(backup::restore_progress))
        .routes(routes!(backup::restore_list))
        .routes(routes!(backup::backup_aws_setup))
        .routes(routes!(backup::backup_jobs_list, backup::backup_jobs_create))
        .routes(routes!(backup::backup_jobs_update, backup::backup_jobs_delete))
        .routes(routes!(backup::backup_jobs_run))
        .routes(routes!(backup::backup_jobs_progress))
        .routes(routes!(backup::backup_verify))
        .routes(routes!(tailscale::tailscale_status))
        .routes(routes!(tailscale::tailscale_config_update))
        .routes(routes!(tailscale::tailscale_refresh))
        .routes(routes!(tailscale::app_tailscale_toggle))
        .routes(routes!(updates::update_status))
        .routes(routes!(updates::update_check))
        .routes(routes!(updates::update_all))
        .routes(routes!(updates::update_config_get, updates::update_config_update))
        .routes(routes!(cloudflare::cloudflare_status))
        .routes(routes!(cloudflare::cloudflare_config_update))
        .routes(routes!(cloudflare::cloudflare_zones))
        .routes(routes!(cloudflare::app_domain_bind, cloudflare::app_domain_unbind))
        .split_for_parts();

    let api_with_fallback: Router<AppState> = api_router.fallback(api_fallback);

    let ws_routes = Router::new()
        .route("/api/apps/{id}/logs", axum::routing::get(logs::app_logs))
        .route("/api/apps/{id}/deploy", axum::routing::get(deploy::app_deploy).post(deploy::app_deploy_background))
        .route("/api/apps/{id}/update", axum::routing::get(updates::app_update_ws))
        .route("/api/updates/self-update", axum::routing::get(updates::self_update_ws))
        .route("/api/vpn/test", axum::routing::get(apps::vpn_test_ws))
        .route("/api/tailscale/pihole-dns", axum::routing::get(tailscale::pihole_dns_toggle));

    let openapi_spec = api;
    let docs_routes = Router::new()
        .route("/api/docs/openapi.json", axum::routing::get({
            let spec = serde_json::to_string(&openapi_spec).unwrap();
            move || {
                let spec = spec.clone();
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        spec,
                    )
                }
            }
        }))
        .route("/api/docs", axum::routing::get(|| async {
            Html(include_str!("swagger_ui.html"))
        }))
        .route("/api/docs/", axum::routing::get(|| async {
            Html(include_str!("swagger_ui.html"))
        }));

    Router::new()
        .nest("/api", api_with_fallback)
        .merge(ws_routes)
        .merge(docs_routes)
        .fallback(static_handler)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(state)
}

pub async fn serve(state: AppState, address: &str, port: u16) {
    crate::scheduler::spawn(state.clone());

    // Migrate from old TSDProxy if it exists
    crate::tailscale::migrate_from_tsdproxy(&state.data_dir).await;

    // Auto-start exit node if Tailscale is enabled
    if let Ok(Some(ts_cfg)) = crate::config::load_tailscale_config(&state.data_dir) {
        if ts_cfg.enabled {
            if let Err(e) = crate::tailscale::ensure_exit_node(&state.data_dir, None, ts_cfg.pihole_dns).await {
                tracing::warn!("Failed to auto-start exit node: {e}");
            }
        }
    }

    // Regenerate ts-serve.json for all installed apps (fixes stale proxy targets)
    {
        let s = state.clone();
        tokio::spawn(async move {
            crate::tailscale::regenerate_all_serve_configs(&s).await;
        });
    }

    // Auto-start cloudflared if Cloudflare is enabled
    if let Ok(Some(cf_cfg)) = crate::config::load_cloudflare_config(&state.data_dir) {
        if cf_cfg.enabled {
            if let Some(ref token) = cf_cfg.tunnel_token {
                if let Err(e) = crate::cloudflare::ensure_cloudflared(&state.data_dir, token).await {
                    tracing::warn!("Failed to auto-start cloudflared: {e}");
                }
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
