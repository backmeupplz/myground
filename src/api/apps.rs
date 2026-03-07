use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::backup::{self, BackupResult, Snapshot};
use crate::stats;
use crate::config::{self, BackupConfig, BackupJob, AppBackupConfig, InstalledAppState, VpnConfig};
use crate::docker::{self, ContainerStatus};
use crate::registry::{InstallVariable, AppDefinition, AppMetadata, StorageVolume};
use crate::state::AppState;

use super::response::{action_err, action_ok, ActionResponse};

#[derive(Deserialize, ToSchema)]
pub struct RenameRequest {
    pub display_name: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LanAccessRequest {
    pub enabled: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct GpuRequest {
    /// GPU mode: "nvidia", "intel", or "none" to disable.
    pub mode: String,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

type ApiError = axum::response::Response;

/// Validate app ID is safe for filesystem use, or return an API error.
fn validate_id(id: &str) -> Result<(), ApiError> {
    config::validate_app_id(id)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())
}

/// Load the installed app state, or return an API error.
fn require_installed_state(
    data_dir: &std::path::Path,
    id: &str,
) -> Result<InstalledAppState, ApiError> {
    match config::load_app_state(data_dir, id) {
        Ok(s) if s.installed => Ok(s),
        Ok(_) => Err(action_err(StatusCode::BAD_REQUEST, format!("App {id} not installed")).into_response()),
        Err(e) => Err(action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()),
    }
}

/// Look up an app definition, returning an API error if not found.
fn require_definition<'a>(
    id: &str,
    registry: &'a HashMap<String, AppDefinition>,
    data_dir: &std::path::Path,
) -> Result<&'a AppDefinition, ApiError> {
    crate::apps::lookup_definition(id, registry, data_dir)
        .map_err(|_| action_err(StatusCode::NOT_FOUND, format!("Unknown app: {id}")).into_response())
}

/// Save app state, converting errors to API responses.
fn save_state(data_dir: &std::path::Path, id: &str, state: &InstalledAppState) -> Result<(), ApiError> {
    config::save_app_state(data_dir, id, state)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())
}

/// Resolve `${SERVER_IP}` and `${PORT}` placeholders in post-install notes.
fn resolve_post_install_notes(
    template: Option<&str>,
    installed: bool,
    port: Option<u16>,
) -> Option<String> {
    let notes = template?;
    let mut resolved = notes.to_string();
    if installed {
        if let Some(ip) = stats::get_server_ip() {
            resolved = resolved.replace("${SERVER_IP}", &ip);
        }
        if let Some(p) = port {
            resolved = resolved.replace("${PORT}", &p.to_string());
        }
    }
    Some(resolved)
}

/// Build a AppInfo from a definition, state, and container map.
fn build_app_info(
    id: &str,
    def: &AppDefinition,
    svc_state: &InstalledAppState,
    containers: &HashMap<String, Vec<ContainerStatus>>,
    tailscale_tailnet: Option<&str>,
) -> AppInfo {
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

    let post_install_notes = resolve_post_install_notes(
        def.metadata.post_install_notes.as_deref(),
        svc_state.installed,
        svc_state.port,
    );

    let default_ts_hostname = format!("myground-{id}");
    let ts_hostname = svc_state.tailscale_hostname.as_deref().unwrap_or(&default_ts_hostname);
    let tailscale_url = if svc_state.installed && !svc_state.tailscale_disabled {
        tailscale_tailnet.map(|tn| format!("https://{ts_hostname}.{tn}"))
    } else {
        None
    };

    let domain_url = svc_state.domain.as_ref().map(|d| {
        let fqdn = crate::cloudflare::build_fqdn(&d.subdomain, &d.zone_name);
        format!("https://{fqdn}")
    });

    let uses_host_network = def.compose_template.contains("network_mode: host");
    let supports_tailscale = def.metadata.tailscale_mode != "skip";

    AppInfo {
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
        has_backup_password: svc_state.backup_password.is_some(),
        post_install_notes,
        web_path: def.metadata.web_path.clone(),
        tailscale_url,
        tailscale_disabled: svc_state.tailscale_disabled,
        tailscale_hostname: svc_state.tailscale_hostname.clone(),
        lan_accessible: svc_state.lan_accessible,
        uses_host_network,
        supports_tailscale,
        update_available: svc_state.update_available,
        current_digest: svc_state.image_digest.clone(),
        latest_digest: svc_state.latest_image_digest.clone(),
        domain_url,
        supports_gpu: !def.metadata.gpu_apps.is_empty(),
        gpu_mode: svc_state.gpu_mode.clone(),
        deploying: false,
        vpn_enabled: crate::vpn::is_vpn_enabled(svc_state),
        vpn_provider: svc_state
            .vpn
            .as_ref()
            .filter(|v| v.enabled)
            .and_then(|v| v.provider.clone()),
    }
}

// ── Available apps ──────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AvailableApp {
    pub id: String,
    #[serde(flatten)]
    pub metadata: AppMetadata,
    pub has_storage: bool,
    pub install_variables: Vec<InstallVariable>,
    pub storage_volumes: Vec<StorageVolume>,
}

#[utoipa::path(
    get,
    path = "/apps/available",
    responses(
        (status = 200, description = "List of available apps", body = Vec<AvailableApp>)
    )
)]
pub async fn apps_available(State(state): State<AppState>) -> Json<Vec<AvailableApp>> {
    let mut apps: Vec<AvailableApp> = state
        .registry
        .iter()
        .map(|(id, def)| AvailableApp {
            id: id.clone(),
            metadata: def.metadata.clone(),
            has_storage: !def.storage.is_empty(),
            install_variables: def.install_variables.clone(),
            storage_volumes: def.storage.clone(),
        })
        .collect();
    apps.sort_by(|a, b| a.id.cmp(&b.id));
    Json(apps)
}

// ── Apps with live status ───────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct StorageVolumeStatus {
    pub name: String,
    pub description: String,
    pub container_path: String,
    pub host_path: String,
    pub disk_available_bytes: Option<u64>,
    pub is_db_dump: bool,
}

#[derive(Serialize, ToSchema)]
pub struct AppInfo {
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
    pub has_backup_password: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_install_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailscale_url: Option<String>,
    pub tailscale_disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailscale_hostname: Option<String>,
    pub lan_accessible: bool,
    pub uses_host_network: bool,
    pub supports_tailscale: bool,
    pub update_available: bool,
    /// Full repo digest of the currently running image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_digest: Option<String>,
    /// Full repo digest of the latest available image when an update exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_url: Option<String>,
    pub supports_gpu: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_mode: Option<String>,
    pub deploying: bool,
    pub vpn_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_provider: Option<String>,
}

fn build_storage_status(
    def: &crate::registry::AppDefinition,
    svc_state: &crate::config::InstalledAppState,
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
                description: vol.description.clone(),
                container_path: vol.container_path.clone(),
                host_path,
                disk_available_bytes: disk_available,
                is_db_dump: vol.db_dump.as_ref().and_then(|d| d.restore_command.as_ref()).is_some(),
            }
        })
        .collect()
}

#[utoipa::path(
    get,
    path = "/apps",
    responses(
        (status = 200, description = "All apps with live status", body = Vec<AppInfo>)
    )
)]
pub async fn apps_list(State(state): State<AppState>) -> Json<Vec<AppInfo>> {
    let installed = config::list_installed_apps(&state.data_dir);
    let container_map = docker::get_container_statuses(&state.docker, &installed).await;

    // Get tailnet for Tailscale URLs, detecting on first call if needed
    let mut ts_cfg = config::load_tailscale_config(&state.data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
    if ts_cfg.enabled && ts_cfg.tailnet.is_none() {
        if let Some(tn) = crate::tailscale::detect_tailnet().await {
            ts_cfg.tailnet = Some(tn);
            let _ = config::save_tailscale_config(&state.data_dir, &ts_cfg);
        }
    }
    let tailnet = if ts_cfg.enabled {
        ts_cfg.tailnet.as_deref()
    } else {
        None
    };

    let mut apps: Vec<AppInfo> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Registry apps
    for (id, def) in state.registry.iter() {
        let svc_state = if installed.contains(id) {
            config::load_app_state(&state.data_dir, id).unwrap_or_default()
        } else {
            InstalledAppState::default()
        };
        apps.push(build_app_info(id, def, &svc_state, &container_map, tailnet));
        seen_ids.insert(id.clone());
    }

    // Installed instances not in registry (e.g. filebrowser-2)
    for id in &installed {
        if seen_ids.contains(id) {
            continue;
        }
        let svc_state = config::load_app_state(&state.data_dir, id).unwrap_or_default();
        let parent_id = svc_state.definition_id.as_deref().unwrap_or(id);
        if let Some(def) = state.registry.get(parent_id) {
            apps.push(build_app_info(id, def, &svc_state, &container_map, tailnet));
        }
    }

    // Mark apps that are currently deploying
    let deploying_set = state.deploying.read().unwrap();
    for app in &mut apps {
        if deploying_set.contains(&app.id) {
            app.deploying = true;
        }
    }
    drop(deploying_set);

    apps.sort_by(|a, b| a.id.cmp(&b.id));
    Json(apps)
}

// ── App lifecycle endpoints ─────────────────────────────────────────────────

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
    path = "/apps/{id}/install",
    params(("id" = String, Path, description = "App ID")),
    request_body(content = Option<InstallRequest>, content_type = "application/json"),
    responses(
        (status = 200, description = "App installed", body = InstallResponse),
        (status = 400, description = "Install error", body = ActionResponse),
        (status = 404, description = "App not found", body = ActionResponse)
    )
)]
pub async fn app_install(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<InstallRequest>>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }

    // Serialise installs to prevent port-allocation races
    let _lock = state.install_lock.lock().await;

    let storage_path = body.as_ref().and_then(|b| b.storage_path.as_deref());
    let variables = body.as_ref().and_then(|b| b.variables.clone());
    let display_name = body.as_ref().and_then(|b| b.display_name.as_deref());

    let ts_key = state.tailscale_key.read().unwrap().clone()
        .or_else(|| crate::tailscale::read_exit_node_auth_key(&state.data_dir));
    match crate::apps::install_app_setup(
        &state.data_dir,
        &state.registry,
        &id,
        storage_path,
        variables.as_ref(),
        display_name,
        ts_key.as_deref(),
    ) {
        Ok(result) => Json(InstallResponse {
            ok: true,
            message: format!("App {} installed", result.instance_id),
            port: result.port,
        })
        .into_response(),
        Err(crate::error::AppError::NotFound(msg)) => {
            action_err(StatusCode::NOT_FOUND, msg).into_response()
        }
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/apps/{id}/start",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App started", body = ActionResponse),
        (status = 400, description = "Start error", body = ActionResponse)
    )
)]
pub async fn app_start(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    match crate::apps::start_app(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("App {id} started")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/apps/{id}/stop",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App stopped", body = ActionResponse),
        (status = 400, description = "Stop error", body = ActionResponse)
    )
)]
pub async fn app_stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    match crate::apps::stop_app(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("App {id} stopped")).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/apps/{id}",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App removed", body = ActionResponse),
        (status = 400, description = "Remove error", body = ActionResponse)
    )
)]
pub async fn app_remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = config::validate_app_id(&id) {
        return action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    match crate::apps::remove_app(&state.data_dir, &id).await {
        Ok(()) => action_ok(format!("App {id} removed")).into_response(),
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
    path = "/apps/{id}/storage",
    params(("id" = String, Path, description = "App ID")),
    request_body = StorageUpdateRequest,
    responses(
        (status = 200, description = "Storage paths updated", body = ActionResponse),
        (status = 400, description = "Update error", body = ActionResponse),
        (status = 404, description = "App not found", body = ActionResponse)
    )
)]
pub async fn app_storage_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StorageUpdateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let def = require_definition(&id, &state.registry, &state.data_dir)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    for (name, path) in &body.paths {
        config::validate_storage_path(path)
            .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;
        svc_state.storage_paths.insert(name.clone(), path.clone());
    }

    // Create storage directories
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);
    for path in storage_env.values() {
        std::fs::create_dir_all(path).map_err(|e| {
            action_err(StatusCode::BAD_REQUEST, format!("Failed to create storage directory: {e}")).into_response()
        })?;
    }

    // Regenerate compose with all sidecars
    crate::apps::regenerate_compose(&state.data_dir, &id, def, &svc_state)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    save_state(&state.data_dir, &id, &svc_state)?;
    Ok(action_ok(format!("Storage paths for {id} updated")))
}

// ── Per-app backup config ──────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/apps/{id}/backup",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App backup config", body = AppBackupConfig),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "Not found", body = ActionResponse)
    )
)]
pub async fn app_backup_config_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let svc_state = require_installed_state(&state.data_dir, &id)?;
    // Convert backup_jobs → AppBackupConfig for backward compat
    let cfg = jobs_to_app_backup_config(&svc_state.backup_jobs);
    Ok(Json(cfg))
}

/// Convert backup_jobs to legacy AppBackupConfig for backward-compat API.
fn jobs_to_app_backup_config(jobs: &[BackupJob]) -> AppBackupConfig {
    let mut local = Vec::new();
    let mut remote = Vec::new();
    let mut schedule = None;
    for j in jobs {
        let cfg = BackupConfig {
            repository: j.repository.clone(),
            password: j.password.clone(),
            s3_access_key: j.s3_access_key.clone(),
            s3_secret_key: j.s3_secret_key.clone(),
        };
        if j.destination_type == "local" {
            local.push(cfg);
        } else {
            remote.push(cfg);
        }
        if schedule.is_none() {
            schedule = j.schedule.clone();
        }
    }
    AppBackupConfig {
        enabled: !jobs.is_empty(),
        local,
        remote,
        schedule,
    }
}

#[utoipa::path(
    put,
    path = "/apps/{id}/backup",
    params(("id" = String, Path, description = "App ID")),
    request_body = AppBackupConfig,
    responses(
        (status = 200, description = "Backup config updated", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "Not found", body = ActionResponse)
    )
)]
pub async fn app_backup_config_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AppBackupConfig>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    // Auto-generate backup password when backups are being enabled for the first time
    let was_enabled = !svc_state.backup_jobs.is_empty();
    if body.enabled && !was_enabled && svc_state.backup_password.is_none() {
        svc_state.backup_password = Some(config::generate_backup_password(32));
    }

    // Convert AppBackupConfig → backup_jobs
    let mut new_jobs = Vec::new();
    for cfg in &body.local {
        new_jobs.push(BackupJob {
            id: config::generate_key_id(),
            destination_type: "local".to_string(),
            repository: cfg.repository.clone(),
            password: cfg.password.clone(),
            schedule: body.schedule.clone(),
            ..Default::default()
        });
    }
    for cfg in &body.remote {
        new_jobs.push(BackupJob {
            id: config::generate_key_id(),
            destination_type: "remote".to_string(),
            repository: cfg.repository.clone(),
            password: cfg.password.clone(),
            s3_access_key: cfg.s3_access_key.clone(),
            s3_secret_key: cfg.s3_secret_key.clone(),
            schedule: body.schedule.clone(),
            ..Default::default()
        });
    }
    svc_state.backup_jobs = new_jobs;
    save_state(&state.data_dir, &id, &svc_state)?;

    // Best-effort: auto-init restic repos with the generated password
    if svc_state.backup_password.is_some() {
        for cfg in body.local.iter().chain(body.remote.iter()) {
            let mut init_cfg = cfg.clone();
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
    path = "/apps/{id}/dismiss-credentials",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "Credentials dismissed", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_dismiss_credentials(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
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
    path = "/apps/{id}/dismiss-backup-password",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "Backup password dismissed", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_dismiss_backup_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    // Only allow dismissing when backups are fully disabled
    let backup_active = !svc_state.backup_jobs.is_empty();

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

// ── Backup password retrieval ─────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct BackupPasswordResponse {
    pub password: Option<String>,
}

#[utoipa::path(
    get,
    path = "/apps/{id}/backup-password",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "Backup password", body = BackupPasswordResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_backup_password(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let svc_state = require_installed_state(&state.data_dir, &id)?;
    Ok(Json(BackupPasswordResponse {
        password: svc_state.backup_password,
    }))
}

// ── Per-app backup actions ───────────────────────────────────────────────


#[utoipa::path(
    get,
    path = "/apps/{id}/backup/snapshots",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App backup snapshots", body = Vec<Snapshot>),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_backup_snapshots(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let svc_state = require_installed_state(&state.data_dir, &id)?;
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    // Build configs from backup_jobs, tracking destination type for source badge
    let mut configs: Vec<(BackupConfig, String)> = Vec::new();
    for job in &svc_state.backup_jobs {
        let cfg = backup::resolve_job_destination(
            job,
            &id,
            &global_config,
            svc_state.backup_password.as_deref(),
        );
        let source = job.destination_type.clone();
        configs.push((cfg, source));
    }
    if configs.is_empty() {
        match config::load_backup_config(&state.data_dir) {
            Ok(Some(mut c)) => {
                if c.password.is_none() {
                    c.password = svc_state.backup_password.clone();
                }
                configs.push((c, "remote".to_string()));
            }
            _ => {
                return Err(action_err(
                    StatusCode::BAD_REQUEST,
                    "No backup config set for this app or globally",
                )
                .into_response());
            }
        }
    }

    let mut all_snapshots: Vec<Snapshot> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (cfg, source) in &configs {
        match backup::list_snapshots(cfg).await {
            Ok(snaps) => {
                for mut s in snaps {
                    // Filter by app tag and deduplicate
                    let matches_app = s.tags.iter().any(|t| t.starts_with(&format!("{id}/")));
                    if matches_app && seen_ids.insert(s.id.clone()) {
                        s.source = Some(source.clone());
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
    path = "/apps/{id}/backup/run",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "App backed up", body = Vec<BackupResult>),
        (status = 400, description = "Backup error", body = ActionResponse)
    )
)]
pub async fn app_backup_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    require_installed_state(&state.data_dir, &id)?;
    require_definition(&id, &state.registry, &state.data_dir)?;

    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_app(&state.data_dir, &id, &state.registry, &global_config).await {
        Ok(results) => {
            // Update last_backup_at
            if let Ok(mut st) = config::load_app_state(&state.data_dir, &id) {
                st.last_backup_at = Some(chrono::Utc::now().to_rfc3339());
                let _ = config::save_app_state(&state.data_dir, &id, &st);
            }
            Ok(Json(results).into_response())
        }
        Err(e) => Err(action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response()),
    }
}

// ── Rename app ──────────────────────────────────────────────────────────

#[utoipa::path(
    put,
    path = "/apps/{id}/rename",
    params(("id" = String, Path, description = "App ID")),
    request_body = RenameRequest,
    responses(
        (status = 200, description = "App renamed", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RenameRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    svc_state.display_name = if body.display_name.trim().is_empty() {
        None
    } else {
        Some(body.display_name.trim().to_string())
    };

    save_state(&state.data_dir, &id, &svc_state)?;
    Ok(action_ok(format!("App {id} renamed")))
}

// ── LAN access toggle ──────────────────────────────────────────────────

#[utoipa::path(
    put,
    path = "/apps/{id}/lan",
    params(("id" = String, Path, description = "App ID")),
    request_body = LanAccessRequest,
    responses(
        (status = 200, description = "LAN access toggled", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "App not found", body = ActionResponse)
    )
)]
pub async fn app_lan_toggle(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<LanAccessRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let def = require_definition(&id, &state.registry, &state.data_dir)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    svc_state.lan_accessible = body.enabled;
    save_state(&state.data_dir, &id, &svc_state)?;

    // Regenerate compose and restart
    let svc_dir = crate::apps::regenerate_compose(&state.data_dir, &id, def, &svc_state)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    if let Ok(compose_cmd) = crate::compose::detect_command().await {
        let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await;
    }

    let msg = if body.enabled {
        format!("LAN access enabled for {id} (binding to 0.0.0.0)")
    } else {
        format!("LAN access disabled for {id} (binding to 127.0.0.1)")
    };
    Ok(action_ok(msg))
}

// ── GPU acceleration toggle ─────────────────────────────────────────────

#[utoipa::path(
    put,
    path = "/apps/{id}/gpu",
    params(("id" = String, Path, description = "App ID")),
    request_body = GpuRequest,
    responses(
        (status = 200, description = "GPU mode updated", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "App not found", body = ActionResponse)
    )
)]
pub async fn app_gpu_toggle(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<GpuRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;

    let valid_modes = ["nvidia", "intel", "none"];
    if !valid_modes.contains(&body.mode.as_str()) {
        return Err(action_err(
            StatusCode::BAD_REQUEST,
            format!("Invalid GPU mode '{}'. Must be nvidia, intel, or none.", body.mode),
        )
        .into_response());
    }

    let def = require_definition(&id, &state.registry, &state.data_dir)?;

    if def.metadata.gpu_apps.is_empty() {
        return Err(action_err(
            StatusCode::BAD_REQUEST,
            format!("App {id} does not support GPU acceleration"),
        )
        .into_response());
    }

    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    svc_state.gpu_mode = if body.mode == "none" {
        None
    } else {
        Some(body.mode.clone())
    };
    save_state(&state.data_dir, &id, &svc_state)?;

    // Regenerate compose and restart
    let svc_dir = crate::apps::regenerate_compose(&state.data_dir, &id, def, &svc_state)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    if let Ok(compose_cmd) = crate::compose::detect_command().await {
        let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await;
    }

    let msg = match body.mode.as_str() {
        "none" => format!("GPU acceleration disabled for {id}"),
        mode => format!("GPU acceleration set to {mode} for {id}"),
    };
    Ok(action_ok(msg))
}

// ── VPN sidecar config ──────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/apps/{id}/vpn",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "VPN config", body = VpnConfig),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn app_vpn_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let svc_state = require_installed_state(&state.data_dir, &id)?;
    Ok(Json(svc_state.vpn.unwrap_or_default()))
}

#[utoipa::path(
    put,
    path = "/apps/{id}/vpn",
    params(("id" = String, Path, description = "App ID")),
    request_body = VpnConfig,
    responses(
        (status = 200, description = "VPN config updated", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse),
        (status = 404, description = "App not found", body = ActionResponse)
    )
)]
pub async fn app_vpn_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VpnConfig>,
) -> Result<impl IntoResponse, ApiError> {
    validate_id(&id)?;
    let def = require_definition(&id, &state.registry, &state.data_dir)?;
    let mut svc_state = require_installed_state(&state.data_dir, &id)?;

    // Reject VPN on apps with network_mode: host
    if def.compose_template.contains("network_mode: host") && body.enabled {
        return Err(action_err(
            StatusCode::BAD_REQUEST,
            "VPN sidecar is incompatible with apps using host networking".to_string(),
        )
        .into_response());
    }

    // When enabling with no provider, merge from global VPN config
    let effective = if body.enabled && body.provider.as_ref().map(|p| p.is_empty()).unwrap_or(true) {
        let global_vpn = config::try_load_vpn(&state.data_dir);
        if global_vpn.provider.is_some() {
            VpnConfig {
                enabled: true,
                provider: global_vpn.provider,
                vpn_type: global_vpn.vpn_type,
                server_countries: global_vpn.server_countries,
                port_forwarding: global_vpn.port_forwarding,
                env_vars: global_vpn.env_vars,
            }
        } else {
            body.clone()
        }
    } else {
        body.clone()
    };

    svc_state.vpn = Some(effective.clone());
    save_state(&state.data_dir, &id, &svc_state)?;

    // Clean up env file when disabling
    if !effective.enabled {
        let svc_dir = config::app_dir(&state.data_dir, &id);
        let _ = std::fs::remove_file(svc_dir.join("vpn-sidecar.env"));
    }

    // Regenerate compose and restart
    let svc_dir = crate::apps::regenerate_compose(&state.data_dir, &id, def, &svc_state)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    if let Ok(compose_cmd) = crate::compose::detect_command().await {
        let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d", "--remove-orphans"]).await;
    }

    let msg = if effective.enabled {
        let provider = effective.provider.as_deref().unwrap_or("unknown");
        format!("VPN enabled for {id} (provider: {provider})")
    } else {
        format!("VPN disabled for {id}")
    };
    Ok(action_ok(msg))
}

// ── Global VPN config ────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/vpn/config",
    responses(
        (status = 200, description = "Global VPN config", body = VpnConfig)
    )
)]
pub async fn vpn_config_get(State(state): State<AppState>) -> impl IntoResponse {
    match config::load_vpn_config(&state.data_dir) {
        Ok(Some(mut cfg)) => {
            // Redact env_vars values
            for v in cfg.env_vars.values_mut() {
                *v = "***".to_string();
            }
            Json(cfg).into_response()
        }
        Ok(None) => Json(VpnConfig::default()).into_response(),
        Err(e) => action_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[utoipa::path(
    put,
    path = "/vpn/config",
    request_body = VpnConfig,
    responses(
        (status = 200, description = "Global VPN config saved", body = ActionResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn vpn_config_update(
    State(state): State<AppState>,
    Json(body): Json<VpnConfig>,
) -> impl IntoResponse {
    match config::save_vpn_config(&state.data_dir, &body) {
        Ok(()) => action_ok("Global VPN config saved".to_string()).into_response(),
        Err(e) => action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

// ── VPN test ────────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct VpnTestResponse {
    pub ok: bool,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/vpn/test",
    request_body(content = Option<VpnConfig>, content_type = "application/json"),
    responses(
        (status = 200, description = "VPN test result", body = VpnTestResponse),
        (status = 400, description = "Error", body = ActionResponse)
    )
)]
pub async fn vpn_test(
    State(state): State<AppState>,
    body: Option<Json<VpnConfig>>,
) -> impl IntoResponse {
    // Use provided config or fall back to saved global config
    let config = match body {
        Some(Json(cfg)) if cfg.provider.is_some() => cfg,
        _ => {
            match config::load_vpn_config(&state.data_dir) {
                Ok(Some(cfg)) if cfg.provider.is_some() => cfg,
                _ => {
                    return action_err(
                        StatusCode::BAD_REQUEST,
                        "No VPN configuration provided or saved".to_string(),
                    )
                    .into_response();
                }
            }
        }
    };

    match crate::vpn::test_vpn_connection(&config).await {
        Ok(msg) => Json(VpnTestResponse {
            ok: true,
            message: msg,
        })
        .into_response(),
        Err(e) => Json(VpnTestResponse {
            ok: false,
            message: e.to_string(),
        })
        .into_response(),
    }
}

/// Get the SVG icon for an app.
#[utoipa::path(
    get,
    path = "/apps/{id}/icon.svg",
    params(("id" = String, Path, description = "App ID")),
    responses(
        (status = 200, description = "SVG icon", content_type = "image/svg+xml"),
        (status = 404, description = "Icon not found"),
    )
)]
pub async fn app_icon(Path(id): Path<String>) -> impl IntoResponse {
    // Try exact ID first, then strip trailing "-N" suffix for duplicate instances
    let icon = crate::registry::get_app_icon(&id).or_else(|| {
        let base = id.rsplit_once('-')
            .and_then(|(prefix, suffix)| suffix.chars().all(|c| c.is_ascii_digit()).then_some(prefix))?;
        crate::registry::get_app_icon(base)
    });
    match icon {
        Some(data) => (
            StatusCode::OK,
            [
                ("content-type", "image/svg+xml"),
                ("cache-control", "public, max-age=86400"),
            ],
            data,
        ).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
