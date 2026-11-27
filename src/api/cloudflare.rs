use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::cloudflare::{self, CfZone};
use crate::config::{self, DomainBinding};
use crate::state::AppState;

use super::response::{ActionResponse, action_err, action_ok};

// ── Request / Response types ────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct CloudflareStatus {
    pub enabled: bool,
    pub tunnel_running: bool,
    pub tunnel_id: Option<String>,
    pub bindings: Vec<CloudflareBinding>,
    /// Current setup step message (only set during setup).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_progress: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CloudflareBinding {
    pub app_id: String,
    pub app_name: String,
    pub fqdn: String,
    pub subdomain: String,
    pub zone_name: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CloudflareConfigRequest {
    pub enabled: bool,
    pub api_token: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct BindDomainRequest {
    pub subdomain: String,
    pub zone_id: String,
    pub zone_name: String,
}

// ── Endpoints ───────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/cloudflare/status",
    responses(
        (status = 200, description = "Cloudflare status", body = CloudflareStatus)
    )
)]
pub async fn cloudflare_status(State(state): State<AppState>) -> Json<CloudflareStatus> {
    let cf_cfg = config::try_load_cloudflare(&state.data_dir);
    let tunnel_running = if cf_cfg.enabled {
        cloudflare::is_cloudflared_running().await
    } else {
        false
    };

    let mut bindings = Vec::new();
    for id in config::list_installed_apps(&state.data_dir) {
        let svc_state = match config::load_app_state(&state.data_dir, &id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(ref domain) = svc_state.domain {
            let name = svc_state
                .display_name
                .clone()
                .or_else(|| {
                    crate::apps::lookup_definition(&id, &state.registry, &state.data_dir)
                        .ok()
                        .map(|d| d.metadata.name.clone())
                })
                .unwrap_or_else(|| id.clone());
            bindings.push(CloudflareBinding {
                app_id: id.clone(),
                app_name: name,
                fqdn: cloudflare::build_fqdn(&domain.subdomain, &domain.zone_name),
                subdomain: domain.subdomain.clone(),
                zone_name: domain.zone_name.clone(),
            });
        }
    }

    let setup_progress = state
        .cloudflare_setup_progress
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(CloudflareStatus {
        enabled: cf_cfg.enabled,
        tunnel_running,
        tunnel_id: cf_cfg.tunnel_id,
        bindings,
        setup_progress,
    })
}

#[utoipa::path(
    put,
    path = "/cloudflare/config",
    request_body = CloudflareConfigRequest,
    responses(
        (status = 200, description = "Configuration updated", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn cloudflare_config_update(
    State(state): State<AppState>,
    Json(body): Json<CloudflareConfigRequest>,
) -> impl IntoResponse {
    if body.enabled {
        let api_token = match &body.api_token {
            Some(t) if !t.trim().is_empty() => t.trim().to_string(),
            _ => {
                return action_err(
                    StatusCode::BAD_REQUEST,
                    "API token is required to enable Cloudflare",
                )
                .into_response();
            }
        };

        let progress = state.cloudflare_setup_progress.clone();
        let result = cloudflare::setup_cloudflare(&state.data_dir, &api_token, |msg| {
            *progress.write().unwrap_or_else(|e| e.into_inner()) = if msg.is_empty() {
                None
            } else {
                Some(msg.to_string())
            };
        })
        .await;
        *state
            .cloudflare_setup_progress
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
        // Even a failed setup may have saved `enabled`, so re-render either way
        // (a no-op for apps whose compose didn't change).
        refresh_public_url_apps(&state, &domain_bound_apps(&state)).await;
        match result {
            Ok(()) => {
                action_ok("Cloudflare enabled and tunnel started".to_string()).into_response()
            }
            Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        }
    } else {
        // Disable: stop cloudflared
        let _ = cloudflare::stop_cloudflared(&state.data_dir).await;
        let cf_config = config::CloudflareConfig {
            enabled: false,
            ..Default::default()
        };
        match config::save_cloudflare_config(&state.data_dir, &cf_config) {
            Ok(()) => {
                refresh_public_url_apps(&state, &domain_bound_apps(&state)).await;
                action_ok("Cloudflare disabled".to_string()).into_response()
            }
            Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/cloudflare/zones",
    responses(
        (status = 200, description = "Available zones", body = Vec<CfZone>),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn cloudflare_zones(State(state): State<AppState>) -> impl IntoResponse {
    let cf_cfg = config::try_load_cloudflare(&state.data_dir);
    let api_token = match cf_cfg.api_token.as_deref() {
        Some(t) => t,
        None => {
            return action_err(StatusCode::BAD_REQUEST, "Cloudflare not configured")
                .into_response();
        }
    };

    let client = cloudflare::CloudflareClient::new(api_token);
    match client.list_zones().await {
        Ok(zones) => Json(zones).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/apps/{id}/domain",
    params(("id" = String, Path, description = "App ID")),
    request_body = BindDomainRequest,
    responses(
        (status = 200, description = "Domain bound", body = DomainBinding),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_domain_bind(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<BindDomainRequest>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    // Verify app is installed and has a port
    let svc_state = match config::load_app_state(&state.data_dir, &id) {
        Ok(s) if s.installed && s.port.is_some() => s,
        Ok(s) if !s.installed => {
            return action_err(StatusCode::BAD_REQUEST, format!("App {id} not installed"))
                .into_response();
        }
        Ok(_) => {
            return action_err(
                StatusCode::BAD_REQUEST,
                format!("App {id} has no port assigned"),
            )
            .into_response();
        }
        Err(e) => return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let _ = svc_state; // used for validation above

    match cloudflare::bind_domain(
        &state.data_dir,
        &id,
        body.subdomain.trim(),
        &body.zone_id,
        &body.zone_name,
    )
    .await
    {
        Ok(binding) => {
            refresh_public_url_apps(&state, &[id]).await;
            Json(binding).into_response()
        }
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/apps/{id}/domain",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "Domain unbound", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_domain_unbind(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    match cloudflare::unbind_domain(&state.data_dir, &id).await {
        Ok(()) => {
            refresh_public_url_apps(&state, std::slice::from_ref(&id)).await;
            action_ok(format!("Domain unbound from {id}")).into_response()
        }
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

fn domain_bound_apps(state: &AppState) -> Vec<String> {
    config::list_installed_apps(&state.data_dir)
        .into_iter()
        .filter(|id| config::load_app_state(&state.data_dir, id).is_ok_and(|s| s.domain.is_some()))
        .collect()
}

/// Re-render and restart the given apps whose compose embeds their public
/// address (`${APP_PUBLIC_URL}` / `${APP_PUBLIC_URLS}`), so a domain change,
/// or an upgrade (called at startup), reaches the app. Stopped apps are only
/// re-rendered; they pick it up when started. Apps whose address can't be
/// resolved right now (no server IP yet at boot) are left untouched.
pub(crate) async fn refresh_public_url_apps(state: &AppState, ids: &[String]) {
    for id in ids {
        let svc_state = match config::load_app_state(&state.data_dir, id) {
            Ok(s) if s.installed => s,
            _ => continue,
        };
        let Ok(def) = crate::apps::lookup_definition(id, &state.registry, &state.data_dir) else {
            continue;
        };
        if !crate::apps::uses_public_url(def) {
            continue;
        }
        let merged = crate::apps::build_merged_env(&state.data_dir, id, def, &svc_state);
        if !merged.contains_key("APP_PUBLIC_URL") {
            tracing::warn!("No public address for {id} yet; keeping its compose");
            continue;
        }
        let svc_dir = match crate::apps::regenerate_compose(&state.data_dir, id, def, &svc_state) {
            Ok(dir) => dir,
            Err(e) => {
                tracing::warn!("Failed to regenerate compose for {id} (public URL refresh): {e}");
                continue;
            }
        };
        let env_path = svc_dir.join(".env");
        if let Err(e) = std::fs::write(
            &env_path,
            crate::compose::generate_env_file(&def.defaults, &merged),
        ) {
            tracing::warn!("Failed to write .env for {id}: {e}");
        }
        crate::compose::restrict_file_permissions(&env_path);
        if let Ok(compose_cmd) = crate::compose::detect_command().await
            && crate::compose::run(&compose_cmd, &svc_dir, &["ps", "-q"])
                .await
                .is_ok_and(|out| !out.trim().is_empty())
            && let Err(e) =
                crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await
        {
            tracing::warn!("Compose up failed for {id}: {e}");
        }
    }
}
