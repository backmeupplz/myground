use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::cloudflare::{self, CfZone};
use crate::config::{self, DomainBinding};
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

// ── Request / Response types ────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct CloudflareStatus {
    pub enabled: bool,
    pub tunnel_running: bool,
    pub tunnel_id: Option<String>,
    pub bindings: Vec<CloudflareBinding>,
}

#[derive(Serialize, ToSchema)]
pub struct CloudflareBinding {
    pub service_id: String,
    pub service_name: String,
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
    for id in config::list_installed_services(&state.data_dir) {
        let svc_state = match config::load_service_state(&state.data_dir, &id) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(ref domain) = svc_state.domain {
            let name = svc_state
                .display_name
                .clone()
                .or_else(|| {
                    crate::services::lookup_definition(&id, &state.registry, &state.data_dir)
                        .ok()
                        .map(|d| d.metadata.name.clone())
                })
                .unwrap_or_else(|| id.clone());
            bindings.push(CloudflareBinding {
                service_id: id.clone(),
                service_name: name,
                fqdn: cloudflare::build_fqdn(&domain.subdomain, &domain.zone_name),
                subdomain: domain.subdomain.clone(),
                zone_name: domain.zone_name.clone(),
            });
        }
    }

    Json(CloudflareStatus {
        enabled: cf_cfg.enabled,
        tunnel_running,
        tunnel_id: cf_cfg.tunnel_id,
        bindings,
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
                return action_err(StatusCode::BAD_REQUEST, "API token is required to enable Cloudflare")
                    .into_response()
            }
        };

        match cloudflare::setup_cloudflare(&state.data_dir, &api_token).await {
            Ok(()) => action_ok("Cloudflare enabled and tunnel started".to_string()).into_response(),
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
            Ok(()) => action_ok("Cloudflare disabled".to_string()).into_response(),
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
                .into_response()
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
    path = "/services/{id}/domain",
    params(("id" = String, Path, description = "Service ID")),
    request_body = BindDomainRequest,
    responses(
        (status = 200, description = "Domain bound", body = DomainBinding),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn service_domain_bind(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<BindDomainRequest>,
) -> impl IntoResponse {
    // Verify service is installed and has a port
    let svc_state = match config::load_service_state(&state.data_dir, &id) {
        Ok(s) if s.installed && s.port.is_some() => s,
        Ok(s) if !s.installed => {
            return action_err(StatusCode::BAD_REQUEST, format!("Service {id} not installed"))
                .into_response()
        }
        Ok(_) => {
            return action_err(
                StatusCode::BAD_REQUEST,
                format!("Service {id} has no port assigned"),
            )
            .into_response()
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
        Ok(binding) => Json(binding).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/services/{id}/domain",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Domain unbound", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn service_domain_unbind(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match cloudflare::unbind_domain(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("Domain unbound from {id}")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
