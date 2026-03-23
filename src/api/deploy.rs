use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use std::collections::HashMap;

use crate::config;
use crate::registry::AppDefinition;
use crate::state::AppState;

use super::response::{action_err, action_ok};

/// Check if an app has `arr_config: true` in its metadata.
fn is_arr_config_app(
    registry: &HashMap<String, AppDefinition>,
    data_dir: &std::path::Path,
    app_id: &str,
) -> bool {
    let def_id = config::load_app_state(data_dir, app_id)
        .ok()
        .and_then(|s| s.definition_id.clone())
        .unwrap_or_else(|| app_id.to_string());
    registry
        .get(&def_id)
        .is_some_and(|def| def.metadata.arr_config)
}

/// Run post-deploy hooks: arr auth, auto-link network setup, and autoconfigure.
fn run_post_deploy_hooks(state: &AppState, app_id: &str) {
    let data_dir = state.data_dir.clone();
    let registry = state.registry.clone();
    let app_id = app_id.to_string();

    tokio::spawn(async move {
        // 1. Configure arr auth if needed
        if is_arr_config_app(&registry, &data_dir, &app_id) {
            if let Err(e) = crate::autoconfigure::configure_arr_auth(&data_dir, &app_id).await {
                tracing::warn!("Arr auth config for {app_id} failed: {e}");
            }
        }

        // 2. If the app has links (direct or reverse), set up the shared network
        let svc_state = match config::load_app_state(&data_dir, &app_id) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Check for reverse links: other installed apps that link TO this app.
        let reverse_linked: Vec<String> = config::list_installed_apps_with_state(&data_dir)
            .into_iter()
            .filter(|(id, state)| {
                id.as_str() != app_id
                    && state.app_links.iter().any(|l| l.target_id == app_id)
            })
            .map(|(id, _)| id)
            .collect();

        if svc_state.app_links.is_empty() && reverse_linked.is_empty() {
            return;
        }

        tracing::info!("Post-deploy: setting up links for {app_id}");

        // Create shared Docker network
        if let Err(e) = crate::linking::ensure_shared_network_exists() {
            tracing::warn!("Failed to create shared network: {e}");
            return;
        }

        // Regenerate compose for this app (injects shared network)
        let def_id = svc_state.definition_id.as_deref().unwrap_or(&app_id);
        if let Some(def) = registry.get(def_id) {
            if let Err(e) = crate::apps::regenerate_compose(&data_dir, &app_id, def, &svc_state) {
                tracing::warn!("Failed to regenerate compose for {app_id}: {e}");
            }
        }

        // Regenerate compose for all link targets (adds them to shared network)
        let mut all_affected: Vec<String> = svc_state.app_links.iter().map(|l| l.target_id.clone()).collect();
        for id in &reverse_linked {
            if !all_affected.contains(id) {
                all_affected.push(id.clone());
            }
        }
        let target_dirs = crate::apps::regenerate_linked_apps(&data_dir, &registry, &all_affected);

        // Restart everything
        if let Ok(compose_cmd) = crate::compose::detect_command().await {
            let svc_dir = config::app_dir(&data_dir, &app_id);
            let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await;
            for (_, target_dir) in &target_dirs {
                let _ = crate::compose::run(&compose_cmd, target_dir, &["up", "-d", "--remove-orphans"]).await;
            }
        }

        // 3. Run autoconfigure (download clients, indexer sync, etc.)
        if let Err(e) = crate::autoconfigure::autoconfigure_all_linked(&data_dir, &app_id).await {
            tracing::warn!("autoconfigure for {app_id}: {e}");
        }
    });
}

pub async fn app_deploy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    let guard = match state.try_ws_slot(&id) {
        Some(g) => g,
        None => {
            return action_err(StatusCode::TOO_MANY_REQUESTS, "Too many deploy connections")
                .into_response()
        }
    };
    ws.on_upgrade(move |socket| handle_deploy_stream(socket, state, id, guard))
        .into_response()
}

async fn handle_deploy_stream(
    mut socket: WebSocket,
    state: AppState,
    app_id: String,
    _guard: crate::state::WsGuard,
) {
    // Acquire semaphore permit to limit concurrent deploys
    let _permit = state.deploy_semaphore.acquire().await;

    state.deploying.write().unwrap_or_else(|e| e.into_inner()).insert(app_id.clone());

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);

    let data_dir = state.data_dir.clone();
    let sid = app_id.clone();
    let deploy_task = tokio::spawn(async move {
        crate::compose::deploy_streaming(&data_dir, &sid, tx).await
    });

    // Forward lines from the channel to the WebSocket
    while let Some(line) = rx.recv().await {
        if socket.send(Message::Text(line.into())).await.is_err() {
            break;
        }
    }

    // Wait for deploy to finish and send result
    let deploy_ok = match deploy_task.await {
        Ok(Ok(())) => {
            let _ = socket.send(Message::Text("__DONE__".into())).await;
            true
        }
        Ok(Err(e)) => {
            let _ = socket
                .send(Message::Text(format!("Error: {e}").into()))
                .await;
            false
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error: {e}").into()))
                .await;
            false
        }
    };

    state.deploying.write().unwrap_or_else(|e| e.into_inner()).remove(&app_id);

    // Run post-deploy hooks in background (arr auth, auto-linking, autoconfigure)
    if deploy_ok {
        run_post_deploy_hooks(&state, &app_id);
    }
}

pub async fn app_deploy_background(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    let app_dir = config::app_dir(&state.data_dir, &id);
    if !app_dir.join("docker-compose.yml").exists() {
        return action_err(StatusCode::BAD_REQUEST, format!("App {id} not installed"))
            .into_response();
    }

    state.deploying.write().unwrap_or_else(|e| e.into_inner()).insert(id.clone());

    let data_dir = state.data_dir.clone();
    let deploying = state.deploying.clone();
    let semaphore = state.deploy_semaphore.clone();
    let app_id = id.clone();
    let state_clone = state.clone();
    tokio::spawn(async move {
        // Acquire semaphore permit to limit concurrent deploys
        let _permit = semaphore.acquire().await;
        let result = crate::compose::deploy(&data_dir, &app_id).await;
        deploying.write().unwrap_or_else(|e| e.into_inner()).remove(&app_id);
        match result {
            Ok(()) => run_post_deploy_hooks(&state_clone, &app_id),
            Err(e) => tracing::warn!("Background deploy of {app_id} failed: {e}"),
        }
    });

    action_ok(format!("Deploy started for {id}")).into_response()
}
