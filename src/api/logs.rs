use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures_util::StreamExt;

use crate::docker;
use crate::state::AppState;

const LOG_TAIL_LINES: &str = "100";

pub async fn service_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_log_stream(socket, state, id))
}

/// Pick the best container to tail: prefer a running one, skip init containers.
fn pick_container(containers: &[docker::ContainerStatus]) -> Option<&str> {
    // Prefer running, non-init containers
    if let Some(c) = containers
        .iter()
        .find(|c| c.state == "running" && !c.name.contains("-init"))
    {
        return Some(&c.name);
    }
    // Any running container
    if let Some(c) = containers.iter().find(|c| c.state == "running") {
        return Some(&c.name);
    }
    // First non-init container
    if let Some(c) = containers.iter().find(|c| !c.name.contains("-init")) {
        return Some(&c.name);
    }
    containers.first().map(|c| c.name.as_str())
}

async fn handle_log_stream(mut socket: WebSocket, state: AppState, service_id: String) {
    use bollard::query_parameters::LogsOptionsBuilder;

    let Some(ref docker) = state.docker else {
        let _ = socket
            .send(Message::Text("Docker not connected".into()))
            .await;
        return;
    };

    loop {
        let installed = crate::config::list_installed_services(&state.data_dir);
        let statuses = docker::get_container_statuses(&state.docker, &installed).await;
        let Some(containers) = statuses.get(&service_id) else {
            let _ = socket
                .send(Message::Text("No containers found".into()))
                .await;
            break;
        };

        let Some(container_name) = pick_container(containers) else {
            let _ = socket
                .send(Message::Text("No containers found".into()))
                .await;
            break;
        };
        let container_name = container_name.to_string();

        let opts = LogsOptionsBuilder::default()
            .follow(true)
            .stdout(true)
            .stderr(true)
            .tail(LOG_TAIL_LINES)
            .build();

        let mut stream = docker.logs(&container_name, Some(opts));

        let stream_ended = 'inner: loop {
            tokio::select! {
                item = stream.next() => {
                    match item {
                        Some(Ok(output)) => {
                            let text = output.to_string();
                            if socket.send(Message::Text(text.into())).await.is_err() {
                                return; // client disconnected
                            }
                        }
                        Some(Err(e)) => {
                            let _ = socket.send(Message::Text(format!("Error: {e}").into())).await;
                            break 'inner true;
                        }
                        None => {
                            // Stream ended (container stopped/restarted)
                            break 'inner true;
                        }
                    }
                }
                Some(msg) = socket.recv() => {
                    if msg.is_err() {
                        return; // client disconnected
                    }
                }
            }
        };

        if !stream_ended {
            break;
        }

        // Wait before re-attaching to give the container time to restart
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
