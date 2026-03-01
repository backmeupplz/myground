use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::backup::{self, BackupResult, Snapshot};
use crate::config::{self, BackupConfig, ServiceBackupConfig, ServiceState};
use crate::docker::{self, ContainerStatus};
use crate::registry::{InstallVariable, ServiceDefinition, ServiceMetadata};
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

#[derive(Deserialize, ToSchema)]
pub struct RenameRequest {
    pub display_name: String,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

type ApiError = axum::response::Response;

/// Load the installed service state, or return an API error.
fn require_installed_state(
    data_dir: &std::path::Path,
    id: &str,
) -> Result<ServiceState, ApiError> {
    match config::load_service_state(data_dir, id) {
        Ok(s) if s.installed => Ok(s),
        Ok(_) => Err(action_err(StatusCode::BAD_REQUEST, format!("Service {id} not installed")).into_response()),
        Err(e) => Err(action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()),
    }
}

/// Look up a service definition, returning an API error if not found.
fn require_definition<'a>(
    id: &str,
    registry: &'a HashMap<String, ServiceDefinition>,
    data_dir: &std::path::Path,
) -> Result<&'a ServiceDefinition, ApiError> {
    crate::services::lookup_definition(id, registry, data_dir)
        .map_err(|_| action_err(StatusCode::NOT_FOUND, format!("Unknown service: {id}")).into_response())
}

/// Save service state, converting errors to API responses.
fn save_state(data_dir: &std::path::Path, id: &str, state: &ServiceState) -> Result<(), ApiError> {
    config::save_service_state(data_dir, id, state)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())
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

    let name = svc_state
        .display_name
        .as_deref()
        .unwrap_or(&def.metadata.name)
        .to_string();

    ServiceInfo {
        id: id.to_string(),
        name,
        description: def.metadata.description.clone(),
        icon: def.metadata.icon.clone(),
        category: def.metadata.category.clone(),
        installed: svc_state.installed,
        has_storage: !def.storage.is_empty(),
        backup_supported: def.metadata.backup_supported,
        containers: containers.get(id).cloned().unwrap_or_default(),
        storage,
        port: svc_state.port,
        install_variables: def.install_variables.clone(),
        env_overrides: svc_state.env_overrides.clone(),
        backup_password: svc_state.backup_password.clone(),
    }
}

// ── Available services ──────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AvailableService {
    pub id: String,
    #[serde(flatten)]
    pub metadata: ServiceMetadata,
    pub has_storage: bool,
    pub install_variables: Vec<InstallVariable>,
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
            has_storage: !def.storage.is_empty(),
            install_variables: def.install_variables.clone(),
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
    pub backup_supported: bool,
    pub containers: Vec<ContainerStatus>,
    pub storage: Vec<StorageVolumeStatus>,
    pub port: Option<u16>,
    pub install_variables: Vec<InstallVariable>,
    pub env_overrides: HashMap<String, String>,
    pub backup_password: Option<String>,
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
    #[serde(default)]
    pub variables: Option<HashMap<String, String>>,
    #[serde(default)]
    pub display_name: Option<String>,
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
    let variables = body.as_ref().and_then(|b| b.variables.clone());
    let display_name = body.as_ref().and_then(|b| b.display_name.as_deref());

    match crate::services::install_service_setup(
        &state.data_dir,
        &state.registry,
        &id,
        storage_path,
        variables.as_ref(),
        display_name,
    ) {
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
) -> Result<impl IntoResponse, ApiError> {
    let def = require_definition(&id, &state.registry, &state.data_dir)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    for (name, path) in &body.paths {
        svc_state.storage_paths.insert(name.clone(), path.clone());
    }

    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);

    for path in storage_env.values() {
        std::fs::create_dir_all(path).map_err(|e| {
            action_err(StatusCode::BAD_REQUEST, format!("Failed to create dir {path}: {e}")).into_response()
        })?;
    }

    let mut merged_env = crate::compose::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    let svc_dir = config::service_dir(&state.data_dir, &id);
    let compose_content = crate::compose::generate_compose(def, &merged_env);
    std::fs::write(svc_dir.join("docker-compose.yml"), &compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, format!("Write compose: {e}")).into_response())?;

    save_state(&state.data_dir, &id, &svc_state)?;
    Ok(action_ok(format!("Storage paths for {id} updated")))
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
) -> Result<impl IntoResponse, ApiError> {
    let svc_state = require_installed_state(&state.data_dir, &id)?;
    Ok(Json(svc_state.backup.unwrap_or_default()))
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
) -> Result<impl IntoResponse, ApiError> {
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    // Auto-generate backup password when backups are being enabled for the first time
    let was_enabled = svc_state.backup.as_ref().map(|b| b.enabled).unwrap_or(false);
    if body.enabled && !was_enabled && svc_state.backup_password.is_none() {
        svc_state.backup_password = Some(config::generate_backup_password(32));
    }

    svc_state.backup = Some(body.clone());
    save_state(&state.data_dir, &id, &svc_state)?;

    // Best-effort: auto-init restic repos with the generated password
    if svc_state.backup_password.is_some() {
        if let Some(ref local) = body.local {
            let mut init_cfg = local.clone();
            if init_cfg.password.is_none() {
                init_cfg.password = svc_state.backup_password.clone();
            }
            let _ = crate::backup::init_repo(&init_cfg).await;
        }
        if let Some(ref remote) = body.remote {
            let mut init_cfg = remote.clone();
            if init_cfg.password.is_none() {
                init_cfg.password = svc_state.backup_password.clone();
            }
            let _ = crate::backup::init_repo(&init_cfg).await;
        }
    }

    Ok(action_ok(format!("Backup config for {id} updated")))
}

// ── Dismiss credentials ─────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/services/{id}/dismiss-credentials",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Credentials dismissed", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn service_dismiss_credentials(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let def = require_definition(&id, &state.registry, &state.data_dir)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    // Remove password and text credential variables from env_overrides
    for v in &def.install_variables {
        if v.input_type == "password" || v.input_type == "text" {
            svc_state.env_overrides.remove(&v.key);
        }
    }

    save_state(&state.data_dir, &id, &svc_state)?;
    Ok(action_ok("Credentials dismissed".to_string()))
}

// ── Dismiss backup password ─────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/services/{id}/dismiss-backup-password",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Backup password dismissed", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn service_dismiss_backup_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    // Only allow dismissing when backups are fully disabled
    let backup_active = svc_state
        .backup
        .as_ref()
        .map(|b| b.enabled || b.remote.is_some())
        .unwrap_or(false);

    if backup_active {
        return Err(action_err(
            StatusCode::BAD_REQUEST,
            "Cannot dismiss backup password while backups are active".to_string(),
        )
        .into_response());
    }

    svc_state.backup_password = None;
    save_state(&state.data_dir, &id, &svc_state)?;
    Ok(action_ok("Backup password dismissed".to_string()))
}

// ── Per-service backup actions ───────────────────────────────────────────

/// Inject the service's backup_password into configs that lack a password.
fn inject_backup_password(configs: &mut [BackupConfig], password: Option<&str>) {
    if let Some(pwd) = password {
        for cfg in configs.iter_mut() {
            if cfg.password.is_none() {
                cfg.password = Some(pwd.to_string());
            }
        }
    }
}

#[utoipa::path(
    get,
    path = "/services/{id}/backup/snapshots",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service backup snapshots", body = Vec<Snapshot>),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn service_backup_snapshots(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let svc_state = require_installed_state(&state.data_dir, &id)?;
    let svc_backup = svc_state.backup.as_ref();

    // Collect per-service configs (local + remote), fall back to global
    let mut configs: Vec<BackupConfig> = Vec::new();
    if let Some(local) = svc_backup.and_then(|b| b.local.as_ref()) {
        configs.push(local.clone());
    }
    if let Some(remote) = svc_backup.and_then(|b| b.remote.as_ref()) {
        configs.push(remote.clone());
    }
    if configs.is_empty() {
        match config::load_backup_config(&state.data_dir) {
            Ok(Some(c)) => configs.push(c),
            _ => {
                return Err(action_err(
                    StatusCode::BAD_REQUEST,
                    "No backup config set for this service or globally",
                )
                .into_response());
            }
        }
    }

    inject_backup_password(&mut configs, svc_state.backup_password.as_deref());

    let mut all_snapshots: Vec<Snapshot> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cfg in &configs {
        match backup::list_snapshots(cfg).await {
            Ok(snaps) => {
                for s in snaps {
                    // Filter by service tag and deduplicate
                    let matches_service = s.tags.iter().any(|t| t.starts_with(&format!("{id}/")));
                    if matches_service && seen_ids.insert(s.id.clone()) {
                        all_snapshots.push(s);
                    }
                }
            }
            Err(_) => continue,
        }
    }

    all_snapshots.sort_by(|a, b| b.time.cmp(&a.time));
    Ok(Json(all_snapshots).into_response())
}

#[utoipa::path(
    post,
    path = "/services/{id}/backup/run",
    params(("id" = String, Path, description = "Service ID")),
    responses(
        (status = 200, description = "Service backed up", body = Vec<BackupResult>),
        (status = 400, description = "Backup error", body = ActionResponse)
    )
)]
pub async fn service_backup_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_installed_state(&state.data_dir, &id)?;
    require_definition(&id, &state.registry, &state.data_dir)?;

    let backup_config = config::load_backup_config(&state.data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_service(&state.data_dir, &id, &state.registry, &global_config, &backup_config).await {
        Ok(results) => Ok(Json(results).into_response()),
        Err(e) => Err(action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()),
    }
}

// ── Rename service ──────────────────────────────────────────────────────

#[utoipa::path(
    put,
    path = "/services/{id}/rename",
    params(("id" = String, Path, description = "Service ID")),
    request_body = RenameRequest,
    responses(
        (status = 200, description = "Service renamed", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn service_rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    svc_state.display_name = if body.display_name.trim().is_empty() {
        None
    } else {
        Some(body.display_name.trim().to_string())
    };

    save_state(&state.data_dir, &id, &svc_state)?;
    Ok(action_ok(format!("Service {id} renamed")))
}
