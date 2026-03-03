use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use tower::ServiceExt;

/// Create a router with NO auth configured (for testing setup flow + public endpoints).
fn app() -> axum::Router {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    myground::build_router(state)
}

/// Create a router WITH auth pre-configured and return a session cookie for authenticated requests.
fn app_authed() -> (axum::Router, String) {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.keep();
    let state = myground::AppState::with_docker(None, data_dir.clone());

    // Pre-configure auth in the data dir
    let hash = myground::auth::hash_password("secret123").unwrap();
    let auth = myground::config::AuthConfig {
        username: "admin".to_string(),
        password_hash: hash,
        cli_token_hash: None,
        api_keys: vec![],
    };
    myground::config::save_auth_config(&data_dir, &auth).unwrap();

    // Create a session token
    let token = myground::auth::generate_session_token();
    state.sessions.write().unwrap().insert(token.clone());
    let cookie = format!("myground_session={token}");

    (myground::build_router(state), cookie)
}

async fn json_body(response: Response) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// GET a path (unauthenticated) and return (status, json).
async fn get(app: axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// GET a path with session cookie and return (status, json).
async fn get_auth(app: axum::Router, path: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::get(path)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// POST a path (unauthenticated) and return (status, json).
#[allow(dead_code)]
async fn post(app: axum::Router, path: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(Request::post(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// POST a path with session cookie and return (status, json).
async fn post_auth(app: axum::Router, path: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::post(path)
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
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

/// POST a path with JSON body and session cookie.
async fn post_json_auth(
    app: axum::Router,
    path: &str,
    body: &str,
    cookie: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::post(path)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// PUT a path with JSON body and return (status, json).
#[allow(dead_code)]
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

/// PUT a path with JSON body and session cookie.
async fn put_json_auth(
    app: axum::Router,
    path: &str,
    body: &str,
    cookie: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::put(path)
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

/// DELETE a path with session cookie.
async fn delete_auth(app: axum::Router, path: &str, cookie: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::delete(path)
                .header("cookie", cookie)
                .body(Body::empty())
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
    let (app, cookie) = app_authed();
    let response = app
        .oneshot(
            Request::get("/api/nonexistent")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Unauthenticated requests are blocked ─────────────────────────────────

#[tokio::test]
async fn unauthenticated_api_returns_401() {
    let (app, _cookie) = app_authed();
    let (status, _) = get(app, "/api/apps").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn no_auth_setup_blocks_api() {
    let (status, _) = get(app(), "/api/apps").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ── OpenAPI ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn openapi_spec_is_valid_json() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/docs/openapi.json", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["openapi"], "3.1.0");
    assert_eq!(json["info"]["title"], "MyGround API");
    assert!(json["paths"]["/health"].is_object());
}

#[tokio::test]
async fn swagger_ui_is_accessible() {
    let (app, cookie) = app_authed();
    let response = app
        .oneshot(
            Request::get("/api/docs/")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
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
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/docker/status", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["connected"], false);
}

// ── Apps available ──────────────────────────────────────────────────────

#[tokio::test]
async fn apps_available_returns_all_registered() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/apps/available", &cookie).await;
    assert_eq!(status, StatusCode::OK);

    let registry = myground::registry::load_registry();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), registry.len());

    let ids: Vec<&str> = arr.iter().map(|s| s["id"].as_str().unwrap()).collect();
    for id in registry.keys() {
        assert!(ids.contains(&id.as_str()), "Registry app {id} missing from available list");
    }
}

#[tokio::test]
async fn apps_available_includes_metadata() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/apps/available", &cookie).await;
    for svc in json.as_array().unwrap() {
        assert!(svc["id"].is_string());
        assert!(svc["name"].is_string());
        assert!(svc["description"].is_string());
        assert!(svc["icon"].is_string());
        assert!(svc["category"].is_string());
        assert!(svc["website"].is_string());
    }
}

// ── post_install_notes in available apps ────────────────────────────────

#[tokio::test]
async fn available_pihole_has_post_install_notes() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/apps/available", &cookie).await;
    let pihole = json
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "pihole")
        .expect("pihole should be in available apps");
    let notes = pihole["post_install_notes"].as_str().unwrap();
    assert!(notes.contains("${SERVER_IP}"));
    assert!(notes.contains("${PORT}"));
}

#[tokio::test]
async fn available_whoami_has_no_post_install_notes() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/apps/available", &cookie).await;
    let whoami = json
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "whoami")
        .expect("whoami should be in available apps");
    assert!(whoami.get("post_install_notes").is_none() || whoami["post_install_notes"].is_null());
}

// ── Apps list ───────────────────────────────────────────────────────────

#[tokio::test]
async fn apps_list_returns_all_with_status() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/apps", &cookie).await;
    assert_eq!(status, StatusCode::OK);

    let registry = myground::registry::load_registry();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), registry.len());
    for svc in arr {
        assert_eq!(svc["installed"], false);
        assert!(svc["containers"].as_array().unwrap().is_empty());
        assert!(svc["storage"].as_array().unwrap().is_empty());
    }
}

// ── Disks ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn disks_list_returns_json_array() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/disks", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn disks_smart_returns_json_array() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/disks/smart", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.as_array().is_some());
}

// ── App lifecycle ───────────────────────────────────────────────────────

#[tokio::test]
async fn install_unknown_app_returns_404() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/apps/nonexistent/install", &cookie).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("nonexistent"));
}

#[tokio::test]
async fn start_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/apps/whoami/start", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn stop_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/apps/whoami/stop", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn remove_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = delete_auth(app, "/api/apps/whoami", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn start_unknown_app_returns_400() {
    let (app, cookie) = app_authed();
    let (status, _) = post_auth(app, "/api/apps/nonexistent/start", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn storage_update_unknown_app_returns_404() {
    let (app, cookie) = app_authed();
    let (status, _) = put_json_auth(app, "/api/apps/nonexistent/storage", r#"{"paths":{"data":"/tmp"}}"#, &cookie).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn storage_update_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = put_json_auth(app, "/api/apps/whoami/storage", r#"{"paths":{"data":"/tmp"}}"#, &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("not installed"));
}

// ── Backup endpoints ────────────────────────────────────────────────────────

#[tokio::test]
async fn backup_config_get_returns_default() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/backup/config", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["repository"].is_null());
    assert!(json["password"].is_null());
}

#[tokio::test]
async fn backup_config_update_persists() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.keep();
    let state = myground::AppState::with_docker(None, data_dir.clone());

    // Set up auth
    let hash = myground::auth::hash_password("secret123").unwrap();
    myground::config::save_auth_config(&data_dir, &myground::config::AuthConfig {
        username: "admin".to_string(),
        password_hash: hash,
        cli_token_hash: None,
        api_keys: vec![],
    }).unwrap();
    let token = myground::auth::generate_session_token();
    state.sessions.write().unwrap().insert(token.clone());
    let cookie = format!("myground_session={token}");

    let router = myground::build_router(state);

    let (status, json) = put_json_auth(
        router.clone(),
        "/api/backup/config",
        r#"{"repository":"/backups","password":"secret"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    let (status, json) = get_auth(router, "/api/backup/config", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["repository"], "/backups");
    assert_eq!(json["password"], "secret");
}

#[tokio::test]
async fn backup_snapshots_returns_error_when_no_config() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/backup/snapshots", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("No backup config"));
}

#[tokio::test]
async fn backup_init_returns_error_when_no_config() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/backup/init", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── Apps list with port field ──────────────────────────────────────────

#[tokio::test]
async fn apps_list_includes_port_field() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/apps", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    // Not installed apps should have port: null
    for svc in json.as_array().unwrap() {
        assert!(svc.get("port").is_some(), "Missing port field on app");
        assert!(svc["port"].is_null());
    }
}

// ── Per-app backup config ──────────────────────────────────────────────

#[tokio::test]
async fn app_backup_config_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/apps/whoami/backup", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn app_backup_config_update_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = put_json_auth(
        app,
        "/api/apps/whoami/backup",
        r#"{"enabled":true}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── Per-app backup endpoints ────────────────────────────────────────

#[tokio::test]
async fn app_backup_snapshots_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/apps/whoami/backup/snapshots", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn app_backup_run_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/apps/whoami/backup/run", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── OpenAPI spec completeness ───────────────────────────────────────────────

#[tokio::test]
async fn openapi_spec_lists_all_endpoints() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/docs/openapi.json", &cookie).await;
    let paths = json["paths"].as_object().unwrap();

    let expected = [
        "/health",
        "/auth/status",
        "/auth/setup",
        "/auth/login",
        "/auth/logout",
        "/docker/status",
        "/apps",
        "/apps/available",
        "/apps/{id}/install",
        "/apps/{id}/start",
        "/apps/{id}/stop",
        "/apps/{id}",
        "/apps/{id}/storage",
        "/apps/{id}/backup",
        "/apps/{id}/backup/snapshots",
        "/apps/{id}/backup/run",
        "/disks",
        "/disks/smart",
        "/backup/config",
        "/backup/init",
        "/backup/run",
        "/backup/run/{id}",
        "/backup/snapshots",
        "/backup/restore/{snapshot_id}",
        "/tailscale/status",
        "/tailscale/config",
        "/tailscale/refresh",
        "/auth/api-keys",
        "/auth/api-keys/{id}",
        "/apps/{id}/gpu",
        "/apps/{id}/lan",
        "/apps/{id}/rename",
        "/apps/{id}/tailscale",
        "/apps/{id}/dismiss-credentials",
        "/apps/{id}/dismiss-backup-password",
        "/apps/{id}/backup-password",
        "/updates/status",
        "/updates/check",
        "/updates/update-all",
        "/updates/self-update",
        "/updates/config",
        "/cloudflare/status",
        "/cloudflare/config",
        "/cloudflare/zones",
        "/apps/{id}/domain",
        "/backup/aws-setup",
    ];

    for path in expected {
        assert!(paths.contains_key(path), "Missing endpoint: {path}");
    }
}

// ── OpenAPI includes new schemas ───────────────────────────────────────

// ── Browse endpoint ────────────────────────────────────────────────────────

#[tokio::test]
async fn browse_root_returns_entries() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/browse?path=/", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["path"], "/");
    assert!(json["entries"].as_array().is_some());
}

#[tokio::test]
async fn browse_default_path_returns_root() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/browse", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["path"], "/");
}

// ── System stats ────────────────────────────────────────────────────────

#[tokio::test]
async fn stats_returns_cpu_and_memory() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/stats", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["cpu_count"].as_u64().unwrap() > 0);
    assert!(json["ram_total_bytes"].as_u64().unwrap() > 0);
}

// ── Global config ───────────────────────────────────────────────────────

#[tokio::test]
async fn global_config_get_returns_version() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/config", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["version"].is_string());
}

#[tokio::test]
async fn global_config_update_persists() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.keep();
    let state = myground::AppState::with_docker(None, data_dir.clone());

    let hash = myground::auth::hash_password("secret123").unwrap();
    myground::config::save_auth_config(&data_dir, &myground::config::AuthConfig {
        username: "admin".to_string(),
        password_hash: hash,
        cli_token_hash: None,
        api_keys: vec![],
    }).unwrap();
    let token = myground::auth::generate_session_token();
    state.sessions.write().unwrap().insert(token.clone());
    let cookie = format!("myground_session={token}");

    let router = myground::build_router(state);

    let (status, json) = put_json_auth(
        router.clone(),
        "/api/config",
        r#"{"version":"0.1.0","default_storage_path":"/mnt/data"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    let (status, json) = get_auth(router, "/api/config", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["default_storage_path"], "/mnt/data");
}

// ── App rename ──────────────────────────────────────────────────────

#[tokio::test]
async fn rename_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = put_json_auth(
        app,
        "/api/apps/whoami/rename",
        r#"{"display_name":"My Whoami"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── Dismiss endpoints ───────────────────────────────────────────────────

#[tokio::test]
async fn dismiss_credentials_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/apps/whoami/dismiss-credentials", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn dismiss_backup_password_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/apps/whoami/dismiss-backup-password", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
}

// ── OpenAPI includes new schemas ───────────────────────────────────────

#[tokio::test]
async fn openapi_spec_includes_new_schemas() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/docs/openapi.json", &cookie).await;
    let schemas = json["components"]["schemas"].as_object().unwrap();
    assert!(schemas.contains_key("InstallRequest"), "Missing InstallRequest schema");
    assert!(schemas.contains_key("InstallResponse"), "Missing InstallResponse schema");
    assert!(schemas.contains_key("AppBackupConfig"), "Missing AppBackupConfig schema");
    assert!(schemas.contains_key("AuthStatus"), "Missing AuthStatus schema");
    assert!(schemas.contains_key("TailscaleStatus"), "Missing TailscaleStatus schema");
    assert!(schemas.contains_key("TailscaleConfig"), "Missing TailscaleConfig schema");
}

// ── Auth endpoints ──────────────────────────────────────────────────────

#[tokio::test]
async fn auth_status_returns_setup_required() {
    let (status, json) = get(app(), "/api/auth/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["setup_required"], true);
    assert_eq!(json["authenticated"], false);
}

#[tokio::test]
async fn auth_setup_creates_account() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let (status, json) = post_json(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn auth_setup_rejects_short_password() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let (status, json) = post_json(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin","password":"short"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["message"].as_str().unwrap().contains("at least 8"));
}

#[tokio::test]
async fn auth_setup_rejects_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let _ = post_json(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;

    // Second setup attempt — needs auth now since setup was already done
    let (status, _json) = post_json_auth(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin2","password":"othersecret"}"#,
        "myground_session=invalid",
    )
    .await;
    // It will get 401 since we don't have a valid session, OR 400 if the middleware lets setup through
    // Actually, /auth/setup is only allowed when auth isn't configured. After setup, it requires auth.
    // But once auth is configured, /auth/setup is no longer in the "no auth required" list.
    // So it will hit the session check and fail with 401.
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_login_with_valid_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    // Setup first
    let _ = post_json(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;

    // Login
    let (status, json) = post_json(
        router.clone(),
        "/api/auth/login",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn auth_login_with_wrong_password() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let _ = post_json(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;

    let (status, json) = post_json(
        router.clone(),
        "/api/auth/login",
        r#"{"username":"admin","password":"wrongpass"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn auth_login_rate_limited_after_failures() {
    let dir = tempfile::tempdir().unwrap();
    let state = myground::AppState::with_docker(None, dir.keep());
    let router = myground::build_router(state);

    let _ = post_json(
        router.clone(),
        "/api/auth/setup",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;

    // 5 failed attempts
    for _ in 0..5 {
        let _ = post_json(
            router.clone(),
            "/api/auth/login",
            r#"{"username":"admin","password":"wrongpass"}"#,
        )
        .await;
    }

    // 6th attempt should be rate limited
    let (status, json) = post_json(
        router.clone(),
        "/api/auth/login",
        r#"{"username":"admin","password":"secret123"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(json["message"].as_str().unwrap().contains("Too many"));
}

// ── Tailscale endpoints ─────────────────────────────────────────────────

#[tokio::test]
async fn tailscale_status_returns_disabled_by_default() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/tailscale/status", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["enabled"], false);
    assert_eq!(json["exit_node_running"], false);
}

// ── GPU toggle endpoint ─────────────────────────────────────────────────

#[tokio::test]
async fn gpu_toggle_unknown_app_returns_404() {
    let (app, cookie) = app_authed();
    let (status, json) = put_json_auth(
        app,
        "/api/apps/nonexistent/gpu",
        r#"{"mode":"nvidia"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn gpu_toggle_unsupported_app_returns_400() {
    let (app, cookie) = app_authed();
    // whoami has no gpu support configured
    let (status, json) = put_json_auth(
        app,
        "/api/apps/whoami/gpu",
        r#"{"mode":"nvidia"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("does not support GPU"));
}

#[tokio::test]
async fn gpu_toggle_invalid_mode_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = put_json_auth(
        app,
        "/api/apps/jellyfin/gpu",
        r#"{"mode":"amd"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("Invalid GPU mode"));
}

#[tokio::test]
async fn gpu_toggle_not_installed_returns_400() {
    let (app, cookie) = app_authed();
    // jellyfin supports GPU but isn't installed
    let (status, json) = put_json_auth(
        app,
        "/api/apps/jellyfin/gpu",
        r#"{"mode":"nvidia"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("not installed"));
}

#[tokio::test]
async fn apps_list_includes_gpu_fields() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/apps", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    for svc in json.as_array().unwrap() {
        assert!(svc.get("supports_gpu").is_some(), "Missing supports_gpu field");
        // gpu_mode is skip_serializing_if None, so check it's absent when not set
    }
    // jellyfin should have supports_gpu: true
    let jellyfin = json.as_array().unwrap().iter().find(|s| s["id"] == "jellyfin").unwrap();
    assert_eq!(jellyfin["supports_gpu"], true);
    // whoami should have supports_gpu: false
    let whoami = json.as_array().unwrap().iter().find(|s| s["id"] == "whoami").unwrap();
    assert_eq!(whoami["supports_gpu"], false);
}

// ── Updates endpoints ───────────────────────────────────────────────────

#[tokio::test]
async fn update_status_returns_version() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/updates/status", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["myground_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["myground_update_available"], false);
    assert!(json["apps"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn update_check_returns_ok() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/updates/check", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert!(json["message"].as_str().unwrap().contains("check"));
}

#[tokio::test]
async fn update_all_returns_ok() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/updates/update-all", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn self_update_no_url_returns_400() {
    let (app, cookie) = app_authed();
    let (status, json) = post_auth(app, "/api/updates/self-update", &cookie).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("No update URL"));
}

#[tokio::test]
async fn update_config_get_returns_defaults() {
    let (app, cookie) = app_authed();
    let (status, json) = get_auth(app, "/api/updates/config", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["auto_update_apps"], false);
    assert_eq!(json["auto_update_myground"], false);
}

#[tokio::test]
async fn update_config_update_persists() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.keep();
    let state = myground::AppState::with_docker(None, data_dir.clone());

    let hash = myground::auth::hash_password("secret123").unwrap();
    myground::config::save_auth_config(&data_dir, &myground::config::AuthConfig {
        username: "admin".to_string(),
        password_hash: hash,
        cli_token_hash: None,
        api_keys: vec![],
    }).unwrap();
    let token = myground::auth::generate_session_token();
    state.sessions.write().unwrap().insert(token.clone());
    let cookie = format!("myground_session={token}");

    let router = myground::build_router(state);

    let (status, json) = put_json_auth(
        router.clone(),
        "/api/updates/config",
        r#"{"auto_update_apps":true,"auto_update_myground":true}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    let (status, json) = get_auth(router, "/api/updates/config", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["auto_update_apps"], true);
    assert_eq!(json["auto_update_myground"], true);
}

// ── Auth config round-trip ──────────────────────────────────────────────

#[tokio::test]
async fn global_config_with_auth_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    myground::config::ensure_data_dir(base).unwrap();

    let auth = myground::config::AuthConfig {
        username: "admin".to_string(),
        password_hash: "hash123".to_string(),
        cli_token_hash: None,
        api_keys: vec![],
    };
    myground::config::save_auth_config(base, &auth).unwrap();

    let loaded = myground::config::load_auth_config(base).unwrap().unwrap();
    assert_eq!(loaded.username, "admin");
    assert_eq!(loaded.password_hash, "hash123");
}

#[tokio::test]
async fn global_config_with_tailscale_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    myground::config::ensure_data_dir(base).unwrap();

    let ts = myground::config::TailscaleConfig {
        enabled: true,
        auth_key: None, // auth_key is skip_serializing — not stored
        tailnet: Some("tail1234b.ts.net".to_string()),
    };
    myground::config::save_tailscale_config(base, &ts).unwrap();

    let loaded = myground::config::load_tailscale_config(base).unwrap().unwrap();
    assert!(loaded.enabled);
    assert!(loaded.auth_key.is_none()); // auth_key is never written
    assert_eq!(loaded.tailnet.unwrap(), "tail1234b.ts.net");
}

// ── API key endpoints ───────────────────────────────────────────────────

/// GET with Bearer token auth.
async fn get_bearer(app: axum::Router, path: &str, key: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::get(path)
                .header("authorization", format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json_body(response).await)
}

#[tokio::test]
async fn api_key_create_list_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.keep();
    let state = myground::AppState::with_docker(None, data_dir.clone());

    let hash = myground::auth::hash_password("secret123").unwrap();
    myground::config::save_auth_config(&data_dir, &myground::config::AuthConfig {
        username: "admin".to_string(),
        password_hash: hash,
        cli_token_hash: None,
        api_keys: vec![],
    }).unwrap();
    let token = myground::auth::generate_session_token();
    state.sessions.write().unwrap().insert(token.clone());
    let cookie = format!("myground_session={token}");

    let router = myground::build_router(state);

    // Create a key
    let (status, json) = post_json_auth(
        router.clone(),
        "/api/auth/api-keys",
        r#"{"name":"test-key"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);
    assert!(json["key"].as_str().unwrap().starts_with("myground_ak_"));
    let key_id = json["id"].as_str().unwrap().to_string();
    let raw_key = json["key"].as_str().unwrap().to_string();

    // List keys
    let (status, json) = get_auth(router.clone(), "/api/auth/api-keys", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    let keys = json.as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["name"], "test-key");
    // key_hash should NOT be in the response
    assert!(keys[0].get("key_hash").is_none());

    // Use key to authenticate
    let (status, _) = get_bearer(router.clone(), "/api/health", &raw_key).await;
    assert_eq!(status, StatusCode::OK);

    // Use key for a protected endpoint
    let (status, _) = get_bearer(router.clone(), "/api/apps", &raw_key).await;
    assert_eq!(status, StatusCode::OK);

    // Revoke key
    let (status, json) = delete_auth(
        router.clone(),
        &format!("/api/auth/api-keys/{key_id}"),
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    // Key should no longer work
    let (status, _) = get_bearer(router.clone(), "/api/apps", &raw_key).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // List should be empty
    let (status, json) = get_auth(router.clone(), "/api/auth/api-keys", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn api_key_empty_name_rejected() {
    let (app, cookie) = app_authed();
    let (status, json) = post_json_auth(
        app,
        "/api/auth/api-keys",
        r#"{"name":""}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["message"].as_str().unwrap().contains("Name required"));
}

#[tokio::test]
async fn api_key_revoke_nonexistent_returns_404() {
    let (app, cookie) = app_authed();
    let (status, json) = delete_auth(
        app,
        "/api/auth/api-keys/deadbeef",
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn openapi_includes_api_key_endpoints() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/docs/openapi.json", &cookie).await;
    let paths = json["paths"].as_object().unwrap();
    assert!(paths.contains_key("/auth/api-keys"), "Missing /auth/api-keys endpoint");
    assert!(paths.contains_key("/auth/api-keys/{id}"), "Missing /auth/api-keys/{{id}} endpoint");
}

// ── AWS auto-setup endpoint ─────────────────────────────────────────────

#[tokio::test]
async fn aws_setup_requires_auth() {
    let (app, _cookie) = app_authed();
    let (status, _) = post_json(
        app,
        "/api/backup/aws-setup",
        r#"{"access_key":"AKIA1234","secret_key":"secret","region":"us-east-1"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn aws_setup_rejects_empty_credentials() {
    let (app, cookie) = app_authed();
    let (status, json) = post_json_auth(
        app,
        "/api/backup/aws-setup",
        r#"{"access_key":"","secret_key":"secret","region":"us-east-1"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("access key"));
}

#[tokio::test]
async fn aws_setup_rejects_empty_secret() {
    let (app, cookie) = app_authed();
    let (status, json) = post_json_auth(
        app,
        "/api/backup/aws-setup",
        r#"{"access_key":"AKIA1234","secret_key":"","region":"us-east-1"}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("secret key"));
}

#[tokio::test]
async fn aws_setup_rejects_empty_region() {
    let (app, cookie) = app_authed();
    let (status, json) = post_json_auth(
        app,
        "/api/backup/aws-setup",
        r#"{"access_key":"AKIA1234","secret_key":"secret","region":""}"#,
        &cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("region"));
}

#[tokio::test]
async fn openapi_includes_aws_setup_schemas() {
    let (app, cookie) = app_authed();
    let (_, json) = get_auth(app, "/api/docs/openapi.json", &cookie).await;
    let schemas = json["components"]["schemas"].as_object().unwrap();
    assert!(schemas.contains_key("AwsSetupRequest"), "Missing AwsSetupRequest schema");
    assert!(schemas.contains_key("AwsSetupResult"), "Missing AwsSetupResult schema");
}
