use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;

use crate::state::AppState;

pub async fn service_deploy(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_deploy_stream(socket, state, id))
}

async fn handle_deploy_stream(mut socket: WebSocket, state: AppState, service_id: String) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);

    let data_dir = state.data_dir.clone();
    let sid = service_id.clone();
    let deploy_task = tokio::spawn(async move {
        crate::services::deploy_service_streaming(&data_dir, &sid, tx).await
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
}
