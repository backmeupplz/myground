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

#[tokio::test]
async fn health_returns_ok() {
    let response = app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn health_returns_json_content_type() {
    let response = app()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("application/json"));
}

#[tokio::test]
async fn unknown_api_route_returns_404() {
    let response = app()
        .oneshot(
            Request::get("/api/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn openapi_spec_is_valid_json() {
    let response = app()
        .oneshot(
            Request::get("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

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

#[tokio::test]
async fn frontend_serves_index_html() {
    let response = app()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("MyGround"));
    assert!(html.contains("<div id=\"app\">"));
}

#[tokio::test]
async fn spa_fallback_serves_index_for_unknown_routes() {
    let response = app()
        .oneshot(
            Request::get("/some/random/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("<div id=\"app\">"));
}

#[tokio::test]
async fn frontend_assets_are_served() {
    // First get index.html to find the JS filename
    let response = app()
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    // Extract JS path from <script src="/assets/...">
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

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("javascript"),
        "expected javascript content-type, got: {content_type}"
    );
}

// ── Docker status endpoint ──────────────────────────────────────────────────

#[tokio::test]
async fn docker_status_returns_json() {
    let response = app()
        .oneshot(
            Request::get("/api/docker/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // With None docker, should report disconnected
    assert_eq!(json["connected"], false);
}

// ── Services available endpoint ─────────────────────────────────────────────

#[tokio::test]
async fn services_available_returns_three() {
    let response = app()
        .oneshot(
            Request::get("/api/services/available")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();

    assert_eq!(arr.len(), 3);

    // Check sorted order
    let ids: Vec<&str> = arr.iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["filebrowser", "immich", "whoami"]);
}

// ── Services list endpoint ──────────────────────────────────────────────────

#[tokio::test]
async fn services_list_returns_all_with_status() {
    let response = app()
        .oneshot(
            Request::get("/api/services")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();

    assert_eq!(arr.len(), 3);
    for svc in arr {
        assert_eq!(svc["installed"], false);
        assert!(svc["containers"].as_array().unwrap().is_empty());
        assert!(svc["storage"].as_array().unwrap().is_empty());
    }
}

// ── Service install returns 404 for unknown ─────────────────────────────────

// ── Disks endpoints ─────────────────────────────────────────────────────────

#[tokio::test]
async fn disks_list_returns_json_array() {
    let response = app()
        .oneshot(
            Request::get("/api/disks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().is_some());
    assert!(!json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn disks_smart_returns_json_array() {
    let response = app()
        .oneshot(
            Request::get("/api/disks/smart")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().is_some());
}

// ── Service lifecycle endpoints ──────────────────────────────────────────────

#[tokio::test]
async fn install_unknown_service_returns_404() {
    let response = app()
        .oneshot(
            Request::post("/api/services/nonexistent/install")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = json_body(response).await;
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("nonexistent"));
}

#[tokio::test]
async fn start_not_installed_returns_400() {
    let response = app()
        .oneshot(
            Request::post("/api/services/whoami/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn stop_not_installed_returns_400() {
    let response = app()
        .oneshot(
            Request::post("/api/services/whoami/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn remove_not_installed_returns_400() {
    let response = app()
        .oneshot(
            Request::delete("/api/services/whoami")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert_eq!(json["ok"], false);
}

#[tokio::test]
async fn start_unknown_service_returns_400() {
    let response = app()
        .oneshot(
            Request::post("/api/services/nonexistent/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Service not found in registry still returns 400 (not installed)
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn storage_update_unknown_service_returns_404() {
    let response = app()
        .oneshot(
            Request::put("/api/services/nonexistent/storage")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"paths":{"data":"/tmp"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn storage_update_not_installed_returns_400() {
    let response = app()
        .oneshot(
            Request::put("/api/services/whoami/storage")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"paths":{"data":"/tmp"}}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = json_body(response).await;
    assert_eq!(json["ok"], false);
    assert!(json["message"].as_str().unwrap().contains("not installed"));
}

// ── Available services metadata ─────────────────────────────────────────────

#[tokio::test]
async fn services_available_includes_metadata() {
    let response = app()
        .oneshot(
            Request::get("/api/services/available")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let json = json_body(response).await;
    let arr = json.as_array().unwrap();

    // Each service should have full metadata
    for svc in arr {
        assert!(svc["id"].is_string());
        assert!(svc["name"].is_string());
        assert!(svc["description"].is_string());
        assert!(svc["icon"].is_string());
        assert!(svc["category"].is_string());
        assert!(svc["website"].is_string());
    }
}

// ── OpenAPI spec completeness ───────────────────────────────────────────────

#[tokio::test]
async fn openapi_spec_lists_all_endpoints() {
    let response = app()
        .oneshot(
            Request::get("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let json = json_body(response).await;
    let paths = json["paths"].as_object().unwrap();

    // Verify all expected endpoints are documented
    assert!(paths.contains_key("/health"));
    assert!(paths.contains_key("/docker/status"));
    assert!(paths.contains_key("/services"));
    assert!(paths.contains_key("/services/available"));
    assert!(paths.contains_key("/services/{id}/install"));
    assert!(paths.contains_key("/services/{id}/start"));
    assert!(paths.contains_key("/services/{id}/stop"));
    assert!(paths.contains_key("/services/{id}"));
    assert!(paths.contains_key("/services/{id}/storage"));
    assert!(paths.contains_key("/disks"));
    assert!(paths.contains_key("/disks/smart"));
}
