use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::docker::{self, ContainerStatus};
use crate::registry::ServiceMetadata;
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

// ── Available services ──────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AvailableService {
    pub id: String,
    #[serde(flatten)]
    pub metadata: ServiceMetadata,
}

#[utoipa::path(
    get,
    path = "/services/available",
    responses(
        (status = 200, description = "List of available services", body = Vec<AvailableService>)
    )
)]
pub async fn services_available(State(state): State<AppState>) -> Json<Vec<AvailableService>> {
    let mut services: Vec<AvailableService> = state
        .registry
        .iter()
        .map(|(id, def)| AvailableService {
            id: id.clone(),
            metadata: def.metadata.clone(),
        })
        .collect();
    services.sort_by(|a, b| a.id.cmp(&b.id));
    Json(services)
}

// ── Services with live status ───────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct StorageVolumeStatus {
    pub name: String,
    pub container_path: String,
    pub host_path: String,
    pub disk_available_bytes: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub struct ServiceInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: String,
    pub installed: bool,
    pub containers: Vec<ContainerStatus>,
    pub storage: Vec<StorageVolumeStatus>,
}

#[utoipa::path(
    get,
    path = "/services",
    responses(
        (status = 200, description = "All services with live status", body = Vec<ServiceInfo>)
    )
)]
pub async fn services_list(State(state): State<AppState>) -> Json<Vec<ServiceInfo>> {
    let container_map = docker::get_container_statuses(&state.docker).await;
    let installed = crate::config::list_installed_services(&state.data_dir);

    let mut services: Vec<ServiceInfo> = state
        .registry
        .iter()
        .map(|(id, def)| {
            let is_installed = installed.contains(id);
            let storage = if is_installed {
                let svc_state =
                    crate::config::load_service_state(&state.data_dir, id).unwrap_or_default();
                def.storage
                    .iter()
                    .map(|vol| {
                        let host_path = svc_state
                            .storage_paths
                            .get(&vol.name)
                            .cloned()
                            .unwrap_or_default();
                        let disk_available = if !host_path.is_empty() {
                            crate::disk::disk_usage_for_path(&host_path)
                                .map(|d| d.available_bytes)
                        } else {
                            None
                        };
                        StorageVolumeStatus {
                            name: vol.name.clone(),
                            container_path: vol.container_path.clone(),
                            host_path,
                            disk_available_bytes: disk_available,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };

            ServiceInfo {
                id: id.clone(),
                name: def.metadata.name.clone(),
                description: def.metadata.description.clone(),
                icon: def.metadata.icon.clone(),
                category: def.metadata.category.clone(),
                installed: is_installed,
                containers: container_map.get(id).cloned().unwrap_or_default(),
                storage,
            }
        })
        .collect();
    services.sort_by(|a, b| a.id.cmp(&b.id));
    Json(services)
}

// ── Service lifecycle endpoints ─────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/services/{id}/install",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service installed", body = ActionResponse),
        (status = 400, description = "Install error", body = ActionResponse),
        (status = 404, description = "Service not found", body = ActionResponse)
    )
)]
pub async fn service_install(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Some(def) = state.registry.get(&id) else {
        return action_err(StatusCode::NOT_FOUND, format!("Unknown service: {id}")).into_response();
    };

    let global_config = crate::config::load_global_config(&state.data_dir)
        .unwrap_or_default();
    match crate::services::install_service(&state.data_dir, def, &HashMap::new(), &global_config).await {
        Ok(()) => action_ok(format!("Service {id} installed")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/services/{id}/start",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service started", body = ActionResponse),
        (status = 400, description = "Start error", body = ActionResponse)
    )
)]
pub async fn service_start(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match crate::services::start_service(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("Service {id} started")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/services/{id}/stop",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service stopped", body = ActionResponse),
        (status = 400, description = "Stop error", body = ActionResponse)
    )
)]
pub async fn service_stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match crate::services::stop_service(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("Service {id} stopped")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/services/{id}",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service removed", body = ActionResponse),
        (status = 400, description = "Remove error", body = ActionResponse)
    )
)]
pub async fn service_remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match crate::services::remove_service(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("Service {id} removed")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── Storage update ─────────────────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct StorageUpdateRequest {
    paths: HashMap<String, String>,
}

#[utoipa::path(
    put,
    path = "/services/{id}/storage",
    params(("id" = String, Path, description = "Service ID")),
    request_body = StorageUpdateRequest,
    responses(
        (status = 200, description = "Storage paths updated", body = ActionResponse),
        (status = 400, description = "Update error", body = ActionResponse),
        (status = 404, description = "Service not found", body = ActionResponse)
    )
)]
pub async fn service_storage_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StorageUpdateRequest>,
) -> impl IntoResponse {
    let Some(def) = state.registry.get(&id) else {
        return action_err(StatusCode::NOT_FOUND, format!("Unknown service: {id}")).into_response();
    };

    let mut svc_state = match crate::config::load_service_state(&state.data_dir, &id) {
        Ok(s) if s.installed => s,
        Ok(_) => {
            return action_err(StatusCode::BAD_REQUEST, format!("Service {id} not installed"))
                .into_response();
        }
        Err(e) => return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    for (name, path) in &body.paths {
        svc_state.storage_paths.insert(name.clone(), path.clone());
    }

    let global_config = crate::config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        crate::config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);

    for path in storage_env.values() {
        if let Err(e) = std::fs::create_dir_all(path) {
            return action_err(
                StatusCode::BAD_REQUEST,
                format!("Failed to create dir {path}: {e}"),
            )
            .into_response();
        }
    }

    let mut merged_env = crate::services::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    let svc_dir = crate::config::service_dir(&state.data_dir, &id);
    let compose_content = crate::services::generate_compose(def, &merged_env);
    if let Err(e) = std::fs::write(svc_dir.join("docker-compose.yml"), &compose_content) {
        return action_err(StatusCode::BAD_REQUEST, format!("Write compose: {e}")).into_response();
    }

    if let Err(e) = crate::config::save_service_state(&state.data_dir, &id, &svc_state) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    action_ok(format!("Storage paths for {id} updated")).into_response()
}
