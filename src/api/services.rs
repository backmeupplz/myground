use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::{self, ServiceBackupConfig, ServiceState};
use crate::docker::{self, ContainerStatus};
use crate::registry::{ServiceDefinition, ServiceMetadata};
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Load the installed service state, or return an API error.
fn require_installed_state(
    data_dir: &std::path::Path,
    id: &str,
) -> Result<ServiceState, (StatusCode, String)> {
    match config::load_service_state(data_dir, id) {
        Ok(s) if s.installed => Ok(s),
        Ok(_) => Err((StatusCode::BAD_REQUEST, format!("Service {id} not installed"))),
        Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    }
}

/// Look up a service definition, returning an API error if not found.
fn require_definition<'a>(
    id: &str,
    registry: &'a HashMap<String, ServiceDefinition>,
    data_dir: &std::path::Path,
) -> Result<&'a ServiceDefinition, (StatusCode, String)> {
    crate::services::lookup_definition(id, registry, data_dir)
        .map_err(|_| (StatusCode::NOT_FOUND, format!("Unknown service: {id}")))
}

/// Build a ServiceInfo from a definition, state, and container map.
fn build_service_info(
    id: &str,
    def: &ServiceDefinition,
    svc_state: &ServiceState,
    containers: &HashMap<String, Vec<ContainerStatus>>,
) -> ServiceInfo {
    let storage = if svc_state.installed {
        build_storage_status(def, svc_state)
    } else {
        Vec::new()
    };

    ServiceInfo {
        id: id.to_string(),
        name: def.metadata.name.clone(),
        description: def.metadata.description.clone(),
        icon: def.metadata.icon.clone(),
        category: def.metadata.category.clone(),
        installed: svc_state.installed,
        has_storage: !def.storage.is_empty(),
        containers: containers.get(id).cloned().unwrap_or_default(),
        storage,
        port: svc_state.port,
    }
}

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
    pub has_storage: bool,
    pub containers: Vec<ContainerStatus>,
    pub storage: Vec<StorageVolumeStatus>,
    pub port: Option<u16>,
}

fn build_storage_status(
    def: &crate::registry::ServiceDefinition,
    svc_state: &crate::config::ServiceState,
) -> Vec<StorageVolumeStatus> {
    def.storage
        .iter()
        .map(|vol| {
            let host_path = svc_state
                .storage_paths
                .get(&vol.name)
                .cloned()
                .unwrap_or_default();
            let disk_available = if !host_path.is_empty() {
                crate::disk::disk_usage_for_path(&host_path).map(|d| d.available_bytes)
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
}

#[utoipa::path(
    get,
    path = "/services",
    responses(
        (status = 200, description = "All services with live status", body = Vec<ServiceInfo>)
    )
)]
pub async fn services_list(State(state): State<AppState>) -> Json<Vec<ServiceInfo>> {
    let installed = config::list_installed_services(&state.data_dir);
    let container_map = docker::get_container_statuses(&state.docker, &installed).await;

    let mut services: Vec<ServiceInfo> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Registry services
    for (id, def) in state.registry.iter() {
        let svc_state = if installed.contains(id) {
            config::load_service_state(&state.data_dir, id).unwrap_or_default()
        } else {
            ServiceState::default()
        };
        services.push(build_service_info(id, def, &svc_state, &container_map));
        seen_ids.insert(id.clone());
    }

    // Installed instances not in registry (e.g. filebrowser-2)
    for id in &installed {
        if seen_ids.contains(id) {
            continue;
        }
        let svc_state = config::load_service_state(&state.data_dir, id).unwrap_or_default();
        let parent_id = svc_state.definition_id.as_deref().unwrap_or(id);
        if let Some(def) = state.registry.get(parent_id) {
            services.push(build_service_info(id, def, &svc_state, &container_map));
        }
    }

    services.sort_by(|a, b| a.id.cmp(&b.id));
    Json(services)
}

// ── Service lifecycle endpoints ─────────────────────────────────────────────

#[derive(Deserialize, ToSchema)]
pub struct InstallRequest {
    #[serde(default)]
    pub storage_path: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct InstallResponse {
    pub ok: bool,
    pub message: String,
    pub port: u16,
}

#[utoipa::path(
    post,
    path = "/services/{id}/install",
    params(("id" = String, Path, description = "Service ID")),
    request_body(content = Option<InstallRequest>, content_type = "application/json"),
    responses(
        (status = 200, description = "Service installed", body = InstallResponse),
        (status = 400, description = "Install error", body = ActionResponse),
        (status = 404, description = "Service not found", body = ActionResponse)
    )
)]
pub async fn service_install(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<InstallRequest>>,
) -> impl IntoResponse {
    let storage_path = body.as_ref().and_then(|b| b.storage_path.as_deref());

    match crate::services::install_service(
        &state.data_dir,
        &state.registry,
        &id,
        storage_path,
    )
    .await
    {
        Ok(result) => Json(InstallResponse {
            ok: true,
            message: format!("Service {} installed", result.instance_id),
            port: result.port,
        })
        .into_response(),
        Err(crate::error::ServiceError::NotFound(msg)) => {
            action_err(StatusCode::NOT_FOUND, msg).into_response()
        }
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
    let def = match require_definition(&id, &state.registry, &state.data_dir) {
        Ok(d) => d,
        Err((code, msg)) => return action_err(code, msg).into_response(),
    };

    let mut svc_state = match require_installed_state(&state.data_dir, &id) {
        Ok(s) => s,
        Err((code, msg)) => return action_err(code, msg).into_response(),
    };

    for (name, path) in &body.paths {
        svc_state.storage_paths.insert(name.clone(), path.clone());
    }

    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);

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

    let svc_dir = config::service_dir(&state.data_dir, &id);
    let compose_content = crate::services::generate_compose(def, &merged_env);
    if let Err(e) = std::fs::write(svc_dir.join("docker-compose.yml"), &compose_content) {
        return action_err(StatusCode::BAD_REQUEST, format!("Write compose: {e}")).into_response();
    }

    if let Err(e) = config::save_service_state(&state.data_dir, &id, &svc_state) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    action_ok(format!("Storage paths for {id} updated")).into_response()
}

// ── Per-service backup config ──────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/services/{id}/backup",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service backup config", body = ServiceBackupConfig),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "Not found", body = ActionResponse)
    )
)]
pub async fn service_backup_config_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let svc_state = match require_installed_state(&state.data_dir, &id) {
        Ok(s) => s,
        Err((code, msg)) => return action_err(code, msg).into_response(),
    };
    Json(svc_state.backup.unwrap_or_default()).into_response()
}

#[utoipa::path(
    put,
    path = "/services/{id}/backup",
    params(("id" = String, Path, description = "Service ID")),
    request_body = ServiceBackupConfig,
    responses(
        (status = 200, description = "Backup config updated", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "Not found", body = ActionResponse)
    )
)]
pub async fn service_backup_config_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ServiceBackupConfig>,
) -> impl IntoResponse {
    let mut svc_state = match require_installed_state(&state.data_dir, &id) {
        Ok(s) => s,
        Err((code, msg)) => return action_err(code, msg).into_response(),
    };

    svc_state.backup = Some(body);

    match config::save_service_state(&state.data_dir, &id, &svc_state) {
        Ok(()) => action_ok(format!("Backup config for {id} updated")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}
