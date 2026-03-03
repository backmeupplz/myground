use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::config;
use crate::state::AppState;

use super::response::{action_err, action_ok};

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
    state.deploying.write().unwrap().insert(app_id.clone());

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
    match deploy_task.await {
        Ok(Ok(())) => {
            let _ = socket.send(Message::Text("__DONE__".into())).await;
        }
        Ok(Err(e)) => {
            let _ = socket
                .send(Message::Text(format!("Error: {e}").into()))
                .await;
        }
        Err(e) => {
            let _ = socket
                .send(Message::Text(format!("Error: {e}").into()))
                .await;
        }
    }

    state.deploying.write().unwrap().remove(&app_id);
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

    state.deploying.write().unwrap().insert(id.clone());

    let data_dir = state.data_dir.clone();
    let deploying = state.deploying.clone();
    let app_id = id.clone();
    tokio::spawn(async move {
        let result = crate::compose::deploy(&data_dir, &app_id).await;
        deploying.write().unwrap().remove(&app_id);
        if let Err(e) = result {
            tracing::warn!("Background deploy of {app_id} failed: {e}");
        }
    });

    action_ok(format!("Deploy started for {id}")).into_response()
}
