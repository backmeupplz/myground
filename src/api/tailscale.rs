use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, TailscaleConfig};
use crate::state::AppState;
use crate::tailscale;

use super::response::{action_err, action_ok};

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct TailscaleStatus {
    pub enabled: bool,
    pub exit_node_running: bool,
    /// Whether the exit node has been approved in the Tailscale admin panel.
    pub exit_node_approved: Option<bool>,
    pub tailnet: Option<String>,
    /// Whether HTTPS certificates are available (MagicDNS + HTTPS enabled in Tailscale admin).
    pub https_enabled: Option<bool>,
    /// Whether exit node DNS is routed through Pi-hole.
    pub pihole_dns: bool,
    /// Whether Pi-hole is installed (controls whether pihole_dns toggle is shown).
    pub pihole_installed: bool,
    /// Custom hostname for the exit node (default: "myground").
    pub exit_hostname: Option<String>,
    /// Whether SSH port forwarding (port 22) is enabled on the exit node.
    pub ssh_forward: bool,
    pub apps: Vec<TailscaleAppInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct TailscaleAppInfo {
    pub app_id: String,
    pub hostname: String,
    pub url: Option<String>,
    pub sidecar_running: bool,
    pub tailscale_disabled: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct TailscaleConfigRequest {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auth_key: Option<String>,
    /// Toggle Pi-hole DNS routing for the exit node.
    #[serde(default)]
    pub pihole_dns: Option<bool>,
    /// Custom hostname for the exit node (e.g. "my-exit-node").
    #[serde(default)]
    pub exit_hostname: Option<String>,
    /// Enable SSH port forwarding (port 22) on the exit node.
    #[serde(default)]
    pub ssh_forward: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct AppTailscaleRequest {
    pub disabled: bool,
    /// Custom Tailscale hostname (e.g. "my-photos"). Set to empty string to reset to default.
    #[serde(default)]
    pub hostname: Option<String>,
}

// ── Endpoints ───────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/tailscale/status",
    responses(
        (status = 200, description = "Tailscale status", body = TailscaleStatus)
    )
)]
pub async fn tailscale_status(State(state): State<AppState>) -> Json<TailscaleStatus> {
    let ts_cfg = config::try_load_tailscale(&state.data_dir);

    let exit_node_running = if ts_cfg.enabled {
        tailscale::is_exit_node_running().await
    } else {
        false
    };

    // Try to detect tailnet if running but not yet known
    let tailnet = if exit_node_running && ts_cfg.tailnet.is_none() {
        let detected = tailscale::detect_tailnet().await;
        if let Some(ref tn) = detected {
            let mut updated = ts_cfg.clone();
            updated.tailnet = Some(tn.clone());
            let _ = config::save_tailscale_config(&state.data_dir, &updated);
        }
        detected
    } else {
        ts_cfg.tailnet.clone()
    };

    // Build per-app info
    let installed = config::list_installed_apps(&state.data_dir);
    let apps: Vec<TailscaleAppInfo> = if ts_cfg.enabled {
        let mut svcs = Vec::new();
        for id in &installed {
            let svc_state = config::load_app_state(&state.data_dir, id).unwrap_or_default();
            let sidecar_running = tailscale::is_sidecar_running(id).await;
            let hostname = svc_state
                .tailscale_hostname
                .clone()
                .unwrap_or_else(|| format!("myground-{id}"));
            let url = tailnet
                .as_ref()
                .map(|tn| format!("https://{hostname}.{tn}"));
            svcs.push(TailscaleAppInfo {
                app_id: id.clone(),
                hostname,
                url,
                sidecar_running,
                tailscale_disabled: svc_state.tailscale_disabled,
            });
        }
        svcs
    } else {
        Vec::new()
    };

    let exit_node_approved = if exit_node_running {
        tailscale::is_exit_node_approved().await
    } else {
        None
    };

    let https_enabled = if exit_node_running {
        tailscale::check_https_enabled().await
    } else {
        None
    };

    let pihole_installed = installed.iter().any(|id| id == "pihole");

    Json(TailscaleStatus {
        enabled: ts_cfg.enabled,
        exit_node_running,
        exit_node_approved,
        tailnet,
        https_enabled,
        pihole_dns: ts_cfg.pihole_dns,
        pihole_installed,
        exit_hostname: ts_cfg.exit_hostname,
        ssh_forward: ts_cfg.ssh_forward,
        apps,
    })
}

#[utoipa::path(
    put,
    path = "/tailscale/config",
    request_body = TailscaleConfigRequest,
    responses(
        (status = 200, description = "Config saved", body = super::response::ActionResponse),
        (status = 400, description = "Error", body = super::response::ActionResponse)
    )
)]
pub async fn tailscale_config_update(
    State(state): State<AppState>,
    Json(body): Json<TailscaleConfigRequest>,
) -> impl IntoResponse {
    let existing = config::try_load_tailscale(&state.data_dir);

    let pihole_dns = body.pihole_dns.unwrap_or(existing.pihole_dns);
    let exit_hostname = body.exit_hostname.or(existing.exit_hostname.clone());
    let ssh_forward = body.ssh_forward.unwrap_or(existing.ssh_forward);
    // Save config (without auth_key — it's skip_serializing)
    let ts_cfg = TailscaleConfig {
        enabled: body.enabled,
        auth_key: None,
        tailnet: existing.tailnet,
        pihole_dns,
        exit_hostname,
        ssh_forward,
    };

    if let Err(e) = config::save_tailscale_config(&state.data_dir, &ts_cfg) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    if body.enabled {
        // Start exit node
        let auth_key = body.auth_key.as_deref();
        if let Err(e) = tailscale::ensure_exit_node(&state.data_dir, auth_key, pihole_dns).await {
            return action_err(StatusCode::BAD_REQUEST, format!("Start exit node: {e}"))
                .into_response();
        }

        // Cache key in memory for future app installs
        if let Some(key) = &body.auth_key {
            if !key.trim().is_empty() {
                *state.tailscale_key.write().unwrap_or_else(|e| e.into_inner()) = Some(key.trim().to_string());
            }
        }

        // Inject sidecars into all installed apps
        let installed = config::list_installed_apps(&state.data_dir);
        for id in &installed {
            regenerate_app_compose(&state, id, auth_key, false).await;
        }
    } else {
        // Stop exit node
        let _ = tailscale::stop_exit_node(&state.data_dir).await;

        // Remove sidecars from all installed apps
        let installed = config::list_installed_apps(&state.data_dir);
        for id in &installed {
            remove_app_sidecar(&state, id).await;
        }
    }

    action_ok("Tailscale config saved".to_string()).into_response()
}

// ── Pi-hole DNS toggle (WebSocket with streaming status) ────────────────

pub async fn pihole_dns_toggle(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let guard = match state.try_ws_slot("__pihole_dns__") {
        Some(g) => g,
        None => {
            return action_err(StatusCode::TOO_MANY_REQUESTS, "Already in progress")
                .into_response()
        }
    };
    ws.on_upgrade(move |socket| handle_pihole_dns_stream(socket, state, guard))
        .into_response()
}

async fn handle_pihole_dns_stream(
    mut socket: axum::extract::ws::WebSocket,
    state: AppState,
    _guard: crate::state::WsGuard,
) {
    use axum::extract::ws::Message;

    // Helper: send text and detect dead connections
    macro_rules! ws_send {
        ($socket:expr, $msg:expr) => {
            if $socket.send(Message::Text($msg.into())).await.is_err() {
                tracing::warn!("pihole-dns: WebSocket send failed, client disconnected");
                return;
            }
        };
    }

    // Wait for the first message — it contains {"enable": true/false}
    let enable = match socket.recv().await {
        Some(Ok(Message::Text(text))) => {
            #[derive(serde::Deserialize)]
            struct Cmd {
                enable: bool,
            }
            match serde_json::from_str::<Cmd>(&text) {
                Ok(cmd) => cmd.enable,
                Err(_) => {
                    let _ = socket.send(Message::Text("Error: invalid command".into())).await;
                    return;
                }
            }
        }
        _ => return,
    };

    let action = if enable { "Enabling" } else { "Disabling" };
    tracing::info!("pihole-dns: {action} Pi-hole DNS via WebSocket");

    // Step 1: Save config
    ws_send!(socket, "Saving configuration...");
    let existing = config::try_load_tailscale(&state.data_dir);
    let ts_cfg = TailscaleConfig {
        enabled: existing.enabled,
        auth_key: None,
        tailnet: existing.tailnet,
        pihole_dns: enable,
        exit_hostname: existing.exit_hostname,
        ssh_forward: existing.ssh_forward,
    };
    if let Err(e) = config::save_tailscale_config(&state.data_dir, &ts_cfg) {
        ws_send!(socket, format!("Error: {e}"));
        return;
    }
    ws_send!(socket, "Configuration saved");

    // Step 2: Get Pi-hole IP (if enabling)
    let pihole_ip = if enable {
        ws_send!(socket, "Looking up Pi-hole container IP...");
        let ip = tailscale::get_pihole_ip_public().await;
        match &ip {
            Some(addr) => {
                ws_send!(socket, format!("Pi-hole IP: {addr}"));
            }
            None => {
                ws_send!(socket, "Warning: Pi-hole container not found, DNS will use defaults");
            }
        }
        ip
    } else {
        None
    };

    // Step 3: Generate and write compose file
    ws_send!(socket, format!("{action} Pi-hole DNS in exit node compose..."));
    let exit_dir = state.data_dir.join("tailscale-exit");
    let hostname = ts_cfg.exit_hostname.as_deref().unwrap_or("myground");
    let compose = tailscale::generate_exit_node_compose_public(pihole_ip.as_deref(), hostname, ts_cfg.ssh_forward);
    let compose_path = exit_dir.join("docker-compose.yml");
    if let Err(e) = std::fs::write(&compose_path, &compose) {
        ws_send!(socket, format!("Error writing compose: {e}"));
        return;
    }
    crate::compose::restrict_file_permissions(&compose_path);

    // Step 4: Restart exit node
    // Run compose in a background task and send keepalive pings to prevent
    // proxy/browser from dropping the idle WebSocket during the long compose run.
    ws_send!(socket, "Restarting exit node (this may take a moment)...");
    let compose_cmd = match crate::compose::detect_command().await {
        Ok(cmd) => cmd,
        Err(e) => {
            ws_send!(socket, format!("Error: {e}"));
            return;
        }
    };
    tracing::info!("pihole-dns: running compose up -d");
    let cmd_clone = compose_cmd.clone();
    let dir_clone = exit_dir.clone();
    let compose_handle = tokio::spawn(async move {
        crate::compose::run(&cmd_clone, &dir_clone, &["up", "-d"]).await
    });

    // Send pings every 3s while compose is running
    let compose_result = {
        tokio::pin!(compose_handle);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        interval.tick().await; // discard immediate first tick
        loop {
            tokio::select! {
                result = &mut compose_handle => {
                    break match result {
                        Ok(inner) => inner,
                        Err(e) => Err(crate::error::AppError::Compose(format!("Task panicked: {e}"))),
                    };
                }
                _ = interval.tick() => {
                    if socket.send(Message::Ping(vec![].into())).await.is_err() {
                        tracing::warn!("pihole-dns: ping failed, client disconnected during compose");
                        return;
                    }
                }
            }
        }
    };
    tracing::info!("pihole-dns: compose up -d finished: {}", compose_result.is_ok());

    if let Err(e) = compose_result {
        ws_send!(socket, format!("Error restarting exit node: {e}"));
        return;
    }
    ws_send!(socket, "Exit node restarted");

    // Step 5: Verify exit node is running
    ws_send!(socket, "Verifying exit node is running...");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    if tailscale::is_exit_node_running().await {
        ws_send!(socket, "Exit node is running");
    } else {
        ws_send!(socket, "Exit node not running yet, retrying...");
        let _ = crate::compose::run(&compose_cmd, &exit_dir, &["up", "-d"]).await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        if tailscale::is_exit_node_running().await {
            ws_send!(socket, "Exit node is running");
        } else {
            ws_send!(socket, "Error: exit node failed to start");
            return;
        }
    }

    let status_msg = format!("Pi-hole DNS {}", if enable { "enabled" } else { "disabled" });
    tracing::info!("pihole-dns: {status_msg}");
    ws_send!(socket, status_msg);
    ws_send!(socket, "__DONE__");
}

#[utoipa::path(
    post,
    path = "/tailscale/refresh",
    responses(
        (status = 200, description = "Apps refreshed", body = super::response::ActionResponse),
        (status = 400, description = "Error", body = super::response::ActionResponse)
    )
)]
pub async fn tailscale_refresh(State(state): State<AppState>) -> impl IntoResponse {
    let ts_cfg = config::try_load_tailscale(&state.data_dir);
    let installed = config::list_installed_apps(&state.data_dir);
    let mut refreshed = 0u32;

    for id in &installed {
        if ts_cfg.enabled {
            regenerate_app_compose(&state, id, None, false).await;
        } else {
            remove_app_sidecar(&state, id).await;
        }
        refreshed += 1;
    }

    action_ok(format!("Refreshed {refreshed} app(s)")).into_response()
}

#[utoipa::path(
    put,
    path = "/apps/{id}/tailscale",
    params(("id" = String, Path, description = "App ID")),
    request_body = AppTailscaleRequest,
    responses(
        (status = 200, description = "Tailscale toggled", body = super::response::ActionResponse),
        (status = 400, description = "Error", body = super::response::ActionResponse)
    )
)]
pub async fn app_tailscale_toggle(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AppTailscaleRequest>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    let mut svc_state = match config::load_app_state(&state.data_dir, &id) {
        Ok(s) if s.installed => s,
        Ok(_) => {
            return action_err(StatusCode::BAD_REQUEST, format!("App {id} not installed"))
                .into_response()
        }
        Err(e) => {
            return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()
        }
    };

    svc_state.tailscale_disabled = body.disabled;

    // Update hostname if provided
    let hostname_changed = if let Some(ref hostname) = body.hostname {
        let old = svc_state.tailscale_hostname.clone();
        if hostname.is_empty() {
            svc_state.tailscale_hostname = None;
        } else {
            svc_state.tailscale_hostname = Some(hostname.clone());
        }
        old != svc_state.tailscale_hostname
    } else {
        false
    };

    if let Err(e) = config::save_app_state(&state.data_dir, &id, &svc_state) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    // Regenerate compose file
    if body.disabled {
        remove_app_sidecar(&state, &id).await;
    } else {
        regenerate_app_compose(&state, &id, None, hostname_changed).await;
    }

    let msg = if body.disabled {
        format!("Tailscale disabled for {id}")
    } else {
        format!("Tailscale enabled for {id}")
    };
    action_ok(msg).into_response()
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Regenerate an app's compose file with sidecar injection, then restart.
/// When `force_recreate_sidecar` is true, the sidecar container is force-recreated
/// and Tailscale is told to adopt the new hostname.
async fn regenerate_app_compose(state: &AppState, id: &str, auth_key: Option<&str>, force_recreate_sidecar: bool) {
    // Fall back to the exit node's cached auth key when none is provided
    let fallback_key = if auth_key.is_none() {
        tailscale::read_exit_node_auth_key(&state.data_dir)
    } else {
        None
    };
    let effective_key = auth_key.or(fallback_key.as_deref());

    let svc_state = config::load_app_state(&state.data_dir, id).unwrap_or_default();
    if svc_state.tailscale_disabled {
        return;
    }

    let def_id = svc_state.definition_id.as_deref().unwrap_or(id);
    let Some(def) = state.registry.get(def_id) else {
        return;
    };

    let mode = &def.metadata.tailscale_mode;
    let vpn_active = crate::vpn::is_vpn_enabled(&svc_state);
    let effective_mode = crate::apps::effective_tailscale_mode(mode, vpn_active);
    if effective_mode == "skip" {
        return;
    }

    let svc_dir = config::app_dir(&state.data_dir, id);
    let compose_path = svc_dir.join("docker-compose.yml");
    let Ok(yaml) = std::fs::read_to_string(&compose_path) else {
        return;
    };

    // First remove any existing sidecar
    let clean = match tailscale::remove_tailscale_sidecar(&yaml) {
        Ok(y) => y,
        Err(_) => yaml,
    };

    // Also remove old TSDProxy labels if present
    let clean = match tailscale::remove_tsdproxy_labels(&clean) {
        Ok(y) => y,
        Err(_) => clean,
    };

    let toml_port = def.health.as_ref().and_then(|h| h.container_port).unwrap_or(80);
    let main_svc = tailscale::extract_main_service_name(&clean);
    let port = tailscale::extract_main_service_container_port(&clean).unwrap_or(toml_port);
    let host_net = clean.contains("network_mode: host");
    let proxy_target = crate::apps::tailscale_proxy_target(id, port, effective_mode, vpn_active, main_svc.as_deref(), host_net);

    // Regenerate .env if the template uses dynamic vars that depend on tailnet/hostname
    if def.compose_template.contains("${NEXTCLOUD_TRUSTED_DOMAINS}") {
        let merged = crate::apps::build_merged_env(&state.data_dir, id, def, &svc_state);
        let env_content = crate::compose::generate_env_file(&def.defaults, &merged);
        let env_path = svc_dir.join(".env");
        let _ = std::fs::write(&env_path, &env_content);
        crate::compose::restrict_file_permissions(&env_path);
    }

    match tailscale::inject_tailscale_sidecar(&clean, id, port, effective_mode, effective_key, svc_state.tailscale_hostname.as_deref()) {
        Ok(injected) => {
            let _ = std::fs::write(&compose_path, &injected);
            let _ = tailscale::write_serve_config(&svc_dir, &proxy_target);
            // Ensure ts-sidecar.env exists (compose always references it)
            let env_path = svc_dir.join("ts-sidecar.env");
            if let Some(key) = effective_key {
                let _ = std::fs::write(&env_path, format!("TS_AUTHKEY={key}\n"));
                crate::compose::restrict_file_permissions(&env_path);
            } else if !env_path.exists() {
                let _ = std::fs::write(&env_path, "");
                crate::compose::restrict_file_permissions(&env_path);
            }
        }
        Err(e) => {
            tracing::warn!("Sidecar inject failed for {id}: {e}");
            return;
        }
    }

    // Restart the app
    if let Ok(compose_cmd) = crate::compose::detect_command().await {
        if force_recreate_sidecar {
            // Force-recreate the sidecar so Docker picks up the new hostname
            // (TS_HOSTNAME env var ensures containerboot applies the correct hostname)
            let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--force-recreate", "--no-deps", "ts-sidecar"]).await;
            // Wait for tailscaled to be ready and re-apply the serve config
            // so ${TS_CERT_DOMAIN} resolves to the new hostname's cert domain.
            // Await directly (not fire-and-forget) so HTTPS is ready before we restart the app.
            let default_hostname = format!("myground-{id}");
            let expected_hostname = svc_state.tailscale_hostname.as_deref()
                .unwrap_or(&default_hostname);
            tailscale::apply_serve_config(id, &svc_dir, Some(expected_hostname)).await;
            // Now restart the rest — app won't be bounced until serve config is applied
            let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await;
        } else {
            if let Err(e) = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await {
                tracing::warn!("Compose up failed for {id}: {e}");
            }
        }
    }

    // Run on_tailscale_change commands if defined
    tailscale::run_on_tailscale_change(id, def, &svc_state, &state.data_dir).await;
}

/// Remove sidecar from an app's compose file and restart.
async fn remove_app_sidecar(state: &AppState, id: &str) {
    // Log out from Tailscale first so the machine is removed from the tailnet
    tailscale::logout_sidecar(id).await;

    let svc_dir = config::app_dir(&state.data_dir, id);
    let compose_path = svc_dir.join("docker-compose.yml");
    let Ok(yaml) = std::fs::read_to_string(&compose_path) else {
        return;
    };

    let new_yaml = match tailscale::remove_tailscale_sidecar(&yaml) {
        Ok(y) => y,
        Err(e) => {
            tracing::warn!("Sidecar removal failed for {id}: {e}");
            return;
        }
    };

    // Also clean old TSDProxy labels
    let new_yaml = match tailscale::remove_tsdproxy_labels(&new_yaml) {
        Ok(y) => y,
        Err(_) => new_yaml,
    };

    if std::fs::write(&compose_path, &new_yaml).is_ok() {
        // Remove ts-serve.json
        let _ = std::fs::remove_file(svc_dir.join("ts-serve.json"));

        if let Ok(compose_cmd) = crate::compose::detect_command().await {
            if let Err(e) = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await {
                tracing::warn!("Compose up failed for {id}: {e}");
            }
        }
    }
}
