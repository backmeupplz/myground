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
    /// Whether exit node DNS is routed through Pi-hole.
    pub pihole_dns: bool,
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

    Json(TailscaleStatus {
        enabled: ts_cfg.enabled,
        exit_node_running,
        exit_node_approved,
        tailnet,
        pihole_dns: ts_cfg.pihole_dns,
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

    // Save config (without auth_key — it's skip_serializing)
    let ts_cfg = TailscaleConfig {
        enabled: body.enabled,
        auth_key: None,
        tailnet: existing.tailnet,
        pihole_dns,
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
                *state.tailscale_key.write().unwrap() = Some(key.trim().to_string());
            }
        }

        // Inject sidecars into all installed apps
        let installed = config::list_installed_apps(&state.data_dir);
        for id in &installed {
            regenerate_app_compose(&state, id, auth_key).await;
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
            regenerate_app_compose(&state, id, None).await;
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
    if let Some(ref hostname) = body.hostname {
        if hostname.is_empty() {
            svc_state.tailscale_hostname = None;
        } else {
            svc_state.tailscale_hostname = Some(hostname.clone());
        }
    }

    if let Err(e) = config::save_app_state(&state.data_dir, &id, &svc_state) {
        return action_err(StatusCode::BAD_REQUEST, format!("Save error: {e}")).into_response();
    }

    // Regenerate compose file
    if body.disabled {
        remove_app_sidecar(&state, &id).await;
    } else {
        regenerate_app_compose(&state, &id, None).await;
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
async fn regenerate_app_compose(state: &AppState, id: &str, auth_key: Option<&str>) {
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

    let port = tailscale::extract_container_port(&clean).unwrap_or(80);
    let proxy_target = crate::apps::tailscale_proxy_target(id, port, effective_mode, vpn_active);

    match tailscale::inject_tailscale_sidecar(&clean, id, port, effective_mode, auth_key, svc_state.tailscale_hostname.as_deref()) {
        Ok(injected) => {
            let _ = std::fs::write(&compose_path, &injected);
            let _ = tailscale::write_serve_config(&svc_dir, port, &proxy_target);
            // Ensure ts-sidecar.env exists (compose always references it)
            let env_path = svc_dir.join("ts-sidecar.env");
            if let Some(key) = auth_key {
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
        if let Err(e) = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await {
            tracing::warn!("Compose up failed for {id}: {e}");
        }
    }
}

/// Remove sidecar from an app's compose file and restart.
async fn remove_app_sidecar(state: &AppState, id: &str) {
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
