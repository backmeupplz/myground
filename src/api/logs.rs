use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures_util::StreamExt;

use crate::docker;
use crate::state::AppState;

pub async fn service_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_log_stream(socket, state, id))
}

async fn handle_log_stream(mut socket: WebSocket, state: AppState, service_id: String) {
    use bollard::query_parameters::LogsOptionsBuilder;

    let Some(ref docker) = state.docker else {
        let _ = socket
            .send(Message::Text("Docker not connected".into()))
            .await;
        return;
    };

    let statuses = docker::get_container_statuses(&state.docker).await;
    let Some(containers) = statuses.get(&service_id) else {
        let _ = socket
            .send(Message::Text("No containers found".into()))
            .await;
        return;
    };

    let container_name = &containers[0].name;

    let opts = LogsOptionsBuilder::default()
        .follow(true)
        .stdout(true)
        .stderr(true)
        .tail("100")
        .build();

    let mut stream = docker.logs(container_name, Some(opts));

    loop {
        tokio::select! {
            Some(result) = stream.next() => {
                match result {
                    Ok(output) => {
                        let text = output.to_string();
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = socket.send(Message::Text(format!("Error: {e}").into())).await;
                        break;
                    }
                }
            }
            Some(msg) = socket.recv() => {
                if msg.is_err() {
                    break;
                }
            }
            else => break,
        }
    }
}
