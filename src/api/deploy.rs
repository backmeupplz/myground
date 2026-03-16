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

/// Spawn a background task to configure arr auth if the app needs it.
fn run_arr_auth_if_needed(state: &AppState, app_id: &str) {
    if is_arr_config_app(&state.registry, &state.data_dir, app_id) {
        let data_dir = state.data_dir.clone();
        let app_id = app_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = crate::autoconfigure::configure_arr_auth(&data_dir, &app_id).await {
                tracing::warn!("Arr auth config for {app_id} failed: {e}");
            }
        });
    }
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

    // Run arr auth config in background after successful deploy
    if deploy_ok {
        run_arr_auth_if_needed(&state, &app_id);
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
    let registry = state.registry.clone();
    let deploying = state.deploying.clone();
    let semaphore = state.deploy_semaphore.clone();
    let app_id = id.clone();
    tokio::spawn(async move {
        // Acquire semaphore permit to limit concurrent deploys
        let _permit = semaphore.acquire().await;
        let result = crate::compose::deploy(&data_dir, &app_id).await;
        deploying.write().unwrap_or_else(|e| e.into_inner()).remove(&app_id);
        match result {
            Ok(()) => {
                // Run arr auth config after successful deploy
                if is_arr_config_app(&registry, &data_dir, &app_id) {
                    let data_dir = data_dir.clone();
                    let app_id = app_id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = crate::autoconfigure::configure_arr_auth(&data_dir, &app_id).await {
                            tracing::warn!("Arr auth config for {app_id} failed: {e}");
                        }
                    });
                }
            }
            Err(e) => {
                tracing::warn!("Background deploy of {app_id} failed: {e}");
            }
        }
    });

    action_ok(format!("Deploy started for {id}")).into_response()
}
