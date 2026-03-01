use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;

fn app() -> axum::Router {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    myground::build_router(state)
}

async fn json_body(response: Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// GET a path and return (status, json).
async fn get(app: axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// POST a path and return (status, json).
async fn post(app: axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::post(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// POST a path with JSON body and return (status, json).
#[allow(dead_code)]
async fn post_json(
    app: axum::Router,
    path: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// PUT a path with JSON body and return (status, json).
async fn put_json(
    app: axum::Router,
    path: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::put(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

// ── Health ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let (status, json) = get(app(), "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn health_returns_json_content_type() {
    let response = app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let content_type = response.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("application/json"));
}

#[tokio::test]
async fn unknown_api_route_returns_404() {
    let response = app()
        .oneshot(Request::get("/api/nonexistent").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── OpenAPI ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn openapi_spec_is_valid_json() {
    let (status, json) = get(app(), "/api-docs/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["openapi"], "3.1.0");
    assert_eq!(json["info"]["title"], "MyGround API");
    assert!(json["paths"]["/health"].is_object());
}

#[tokio::test]
async fn swagger_ui_is_accessible() {
    let response = app()
        .oneshot(Request::get("/api-docs/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ── Frontend ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn frontend_serves_index_html() {
    let response = app()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("MyGround"));
    assert!(html.contains("<div id=\"app\">"));
}

#[tokio::test]
async fn spa_fallback_serves_index_for_unknown_routes() {
    let response = app()
        .oneshot(Request::get("/some/random/route").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("<div id=\"app\">"));
}

#[tokio::test]
async fn frontend_assets_are_served() {
    let response = app()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    let js_path = html
        .split("src=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("should find JS asset path in HTML");

    let response = app()
        .oneshot(Request::get(js_path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("javascript"), "expected javascript, got: {content_type}");
}

// ── Docker status ───────────────────────────────────────────────────────────

#[tokio::test]
async fn docker_status_returns_json() {
    let (status, json) = get(app(), "/api/docker/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["connected"], false);
}

// ── Services available ──────────────────────────────────────────────────────

#[tokio::test]
async fn services_available_returns_three() {
    let (status, json) = get(app(), "/api/services/available").await;
    assert_eq!(status, StatusCode::OK);

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 5);

    let ids: Vec<&str> = arr.iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["beszel", "filebrowser", "immich", "navidrome", "whoami"]);
}

#[tokio::test]
async fn services_available_includes_metadata() {
    let (_, json) = get(app(), "/api/services/available").await;
    for svc in json.as_array().unwrap() {
        assert!(svc["id"].is_string());
        assert!(svc["name"].is_string());
        assert!(svc["description"].is_string());
        assert!(svc["icon"].is_string());
        assert!(svc["category"].is_string());
        assert!(svc["website"].is_string());
    }
}

// ── Services list ───────────────────────────────────────────────────────────

#[tokio::test]
async fn services_list_returns_all_with_status() {
    let (status, json) = get(app(), "/api/services").await;
    assert_eq!(status, StatusCode::OK);

    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 5);
    for svc in arr {
        assert_eq!(svc["installed"], false);
        assert!(svc["containers"].as_array().unwrap().is_empty());
        assert!(svc["storage"].as_array().unwrap().is_empty());
    }
}

// ── Disks ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn disks_list_returns_json_array() {
    let (status, json) = get(app(), "/api/disks").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn disks_smart_returns_json_array() {
    let (status, json) = get(app(), "/api/disks/smart").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.as_array().is_some());
}

// ── Service lifecycle ───────────────────────────────────────────────────────

#[tokio::test]
async fn install_unknown_service_returns_404() {
    let (status, json) = post(app(), "/api/services/nonexistent/install").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("nonexistent"));
}

#[tokio::test]
async fn start_not_installed_returns_400() {
    let (status, json) = post(app(), "/api/services/whoami/start").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn stop_not_installed_returns_400() {
    let (status, json) = post(app(), "/api/services/whoami/stop").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn remove_not_installed_returns_400() {
    let response = app()
        .oneshot(Request::delete("/api/services/whoami").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn start_unknown_service_returns_400() {
    let (status, _) = post(app(), "/api/services/nonexistent/start").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn storage_update_unknown_service_returns_404() {
    let (status, _) = put_json(app(), "/api/services/nonexistent/storage", r#"{"paths":{"data":"/tmp"}}"#).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn storage_update_not_installed_returns_400() {
    let (status, json) = put_json(app(), "/api/services/whoami/storage", r#"{"paths":{"data":"/tmp"}}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("not installed"));
}

// ── Backup endpoints ────────────────────────────────────────────────────────

#[tokio::test]
async fn backup_config_get_returns_default() {
    let (status, json) = get(app(), "/api/backup/config").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["repository"].is_null());
    assert!(json["password"].is_null());
}

#[tokio::test]
async fn backup_config_update_persists() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let (status, json) = put_json(
        router.clone(),
        "/api/backup/config",
        r#"{"repository":"/backups","password":"secret","keep_daily":7}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    let (status, json) = get(router, "/api/backup/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["repository"], "/backups");
    assert_eq!(json["password"], "secret");
    assert_eq!(json["keep_daily"], 7);
}

#[tokio::test]
async fn backup_snapshots_returns_error_when_no_config() {
    let (status, json) = get(app(), "/api/backup/snapshots").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("No backup config"));
}

#[tokio::test]
async fn backup_init_returns_error_when_no_config() {
    let (status, json) = post(app(), "/api/backup/init").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── Services list with port field ──────────────────────────────────────────

#[tokio::test]
async fn services_list_includes_port_field() {
    let (status, json) = get(app(), "/api/services").await;
    assert_eq!(status, StatusCode::OK);
    // Not installed services should have port: null
    for svc in json.as_array().unwrap() {
        assert!(svc.get("port").is_some(), "Missing port field on service");
        assert!(svc["port"].is_null());
    }
}

// ── Per-service backup config ──────────────────────────────────────────────

#[tokio::test]
async fn service_backup_config_not_installed_returns_400() {
    let (status, json) = get(app(), "/api/services/whoami/backup").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn service_backup_config_update_not_installed_returns_400() {
    let (status, json) = put_json(
        app(),
        "/api/services/whoami/backup",
        r#"{"enabled":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── OpenAPI spec completeness ───────────────────────────────────────────────

#[tokio::test]
async fn openapi_spec_lists_all_endpoints() {
    let (_, json) = get(app(), "/api-docs/openapi.json").await;
    let paths = json["paths"].as_object().unwrap();

    let expected = [
        "/health",
        "/docker/status",
        "/services",
        "/services/available",
        "/services/{id}/install",
        "/services/{id}/start",
        "/services/{id}/stop",
        "/services/{id}",
        "/services/{id}/storage",
        "/services/{id}/backup",
        "/disks",
        "/disks/smart",
        "/backup/config",
        "/backup/init",
        "/backup/run",
        "/backup/run/{id}",
        "/backup/snapshots",
        "/backup/restore/{snapshot_id}",
        "/backup/prune",
    ];

    for path in expected {
        assert!(paths.contains_key(path), "Missing endpoint: {path}");
    }
}

// ── OpenAPI includes new schemas ───────────────────────────────────────────

// ── Browse endpoint ────────────────────────────────────────────────────────

#[tokio::test]
async fn browse_root_returns_entries() {
    let (status, json) = get(app(), "/api/browse?path=/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["path"], "/");
    assert!(json["entries"].as_array().is_some());
}

#[tokio::test]
async fn browse_default_path_returns_root() {
    let (status, json) = get(app(), "/api/browse").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["path"], "/");
}

// ── System stats ────────────────────────────────────────────────────────

#[tokio::test]
async fn stats_returns_cpu_and_memory() {
    let (status, json) = get(app(), "/api/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["cpu_count"].as_u64().unwrap() > 0);
    assert!(json["ram_total_bytes"].as_u64().unwrap() > 0);
}

// ── Global config ───────────────────────────────────────────────────────

#[tokio::test]
async fn global_config_get_returns_version() {
    let (status, json) = get(app(), "/api/config").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn global_config_update_persists() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let (status, json) = put_json(
        router.clone(),
        "/api/config",
        r#"{"version":"0.1.0","default_storage_path":"/mnt/data"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    let (status, json) = get(router, "/api/config").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["default_storage_path"], "/mnt/data");
}

// ── Service rename ──────────────────────────────────────────────────────

#[tokio::test]
async fn rename_not_installed_returns_400() {
    let (status, json) = put_json(
        app(),
        "/api/services/whoami/rename",
        r#"{"display_name":"My Whoami"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── Dismiss endpoints ───────────────────────────────────────────────────

#[tokio::test]
async fn dismiss_credentials_not_installed_returns_400() {
    let (status, json) = post(app(), "/api/services/whoami/dismiss-credentials").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn dismiss_backup_password_not_installed_returns_400() {
    let (status, json) = post(app(), "/api/services/whoami/dismiss-backup-password").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── OpenAPI includes new schemas ───────────────────────────────────────

#[tokio::test]
async fn openapi_spec_includes_new_schemas() {
    let (_, json) = get(app(), "/api-docs/openapi.json").await;
    let schemas = json["components"]["schemas"].as_object().unwrap();
    assert!(schemas.contains_key("InstallRequest"), "Missing InstallRequest schema");
    assert!(schemas.contains_key("InstallResponse"), "Missing InstallResponse schema");
    assert!(schemas.contains_key("ServiceBackupConfig"), "Missing ServiceBackupConfig schema");
}
