use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::backup::{self, BackupResult, Snapshot};
use crate::stats;
use crate::config::{self, BackupConfig, AppBackupConfig, InstalledAppState};
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
        update_available: svc_state.update_available,
        domain_url,
        supports_gpu: !def.metadata.gpu_apps.is_empty(),
        gpu_mode: svc_state.gpu_mode.clone(),
        deploying: false,
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
    pub update_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_url: Option<String>,
    pub supports_gpu: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_mode: Option<String>,
    pub deploying: bool,
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

    // Get tailnet for Tailscale URLs
    let ts_cfg = config::load_tailscale_config(&state.data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
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

    let storage_path = body.as_ref().and_then(|b| b.storage_path.as_deref());
    let variables = body.as_ref().and_then(|b| b.variables.clone());
    let display_name = body.as_ref().and_then(|b| b.display_name.as_deref());

    let ts_key = state.tailscale_key.read().unwrap().clone();
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

    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);

    for path in storage_env.values() {
        std::fs::create_dir_all(path).map_err(|e| {
            action_err(StatusCode::BAD_REQUEST, format!("Failed to create storage directory: {e}")).into_response()
        })?;
    }

    let mut merged_env = crate::compose::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    // Inject BIND_IP based on LAN access setting
    let bind_ip = if svc_state.lan_accessible { "0.0.0.0" } else { "127.0.0.1" };
    merged_env.insert("BIND_IP".to_string(), bind_ip.to_string());

    let svc_dir = config::app_dir(&state.data_dir, &id);
    let mut compose_content = crate::compose::generate_compose(def, &merged_env);

    // Inject Tailscale sidecar if enabled and app hasn't opted out
    if let Ok(Some(ts_cfg)) = config::load_tailscale_config(&state.data_dir) {
        if ts_cfg.enabled && !svc_state.tailscale_disabled {
            let mode = &def.metadata.tailscale_mode;
            if mode != "skip" {
                let port = crate::tailscale::extract_container_port(&compose_content).unwrap_or(80);
                let proxy_target = if mode == "network" {
                    format!("http://myground-{id}:{port}")
                } else {
                    format!("http://127.0.0.1:{port}")
                };
                if let Ok(injected) = crate::tailscale::inject_tailscale_sidecar(
                    &compose_content, &id, port, mode, None,
                    svc_state.tailscale_hostname.as_deref(),
                ) {
                    compose_content = injected;
                    let _ = crate::tailscale::write_serve_config(&svc_dir, port, &proxy_target);
                }
            }
        }
    }

    // Inject GPU if enabled
    if let Some(ref gpu_mode) = svc_state.gpu_mode {
        if !def.metadata.gpu_apps.is_empty() {
            if let Ok(injected) = crate::gpu::inject_gpu(&compose_content, &def.metadata.gpu_apps, gpu_mode) {
                compose_content = injected;
            }
        }
    }

    crate::compose::validate_compose(&compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    let compose_path = svc_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, format!("Failed to write app configuration: {e}")).into_response())?;
    crate::compose::restrict_file_permissions(&compose_path);

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
    Ok(Json(svc_state.backup.unwrap_or_default()))
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

/// Inject the app's backup_password into configs that lack a password.
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
    let svc_backup = svc_state.backup.as_ref();

    // Collect per-app configs (local + remote), fall back to global
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
                    "No backup config set for this app or globally",
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
                    // Filter by app tag and deduplicate
                    let matches_app = s.tags.iter().any(|t| t.starts_with(&format!("{id}/")));
                    if matches_app && seen_ids.insert(s.id.clone()) {
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

    let backup_config = config::load_backup_config(&state.data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();

    match backup::backup_app(&state.data_dir, &id, &state.registry, &global_config, &backup_config).await {
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

    // Regenerate compose file with updated BIND_IP
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);

    let mut merged_env = crate::compose::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    let bind_ip = if svc_state.lan_accessible { "0.0.0.0" } else { "127.0.0.1" };
    merged_env.insert("BIND_IP".to_string(), bind_ip.to_string());

    // Inject SERVER_IP if needed
    if def.compose_template.contains("${SERVER_IP}") {
        if let Some(ip) = stats::get_server_ip() {
            merged_env.insert("SERVER_IP".to_string(), ip);
        }
    }

    let svc_dir = config::app_dir(&state.data_dir, &id);
    let mut compose_content = crate::compose::generate_compose(def, &merged_env);

    // Re-inject Tailscale sidecar if enabled
    if let Ok(Some(ts_cfg)) = config::load_tailscale_config(&state.data_dir) {
        if ts_cfg.enabled && !svc_state.tailscale_disabled {
            let mode = &def.metadata.tailscale_mode;
            if mode != "skip" {
                let port = crate::tailscale::extract_container_port(&compose_content).unwrap_or(80);
                let proxy_target = if mode == "network" {
                    format!("http://myground-{id}:{port}")
                } else {
                    format!("http://127.0.0.1:{port}")
                };
                if let Ok(injected) = crate::tailscale::inject_tailscale_sidecar(
                    &compose_content, &id, port, mode, None,
                    svc_state.tailscale_hostname.as_deref(),
                ) {
                    compose_content = injected;
                    let _ = crate::tailscale::write_serve_config(&svc_dir, port, &proxy_target);
                }
            }
        }
    }

    // Inject GPU if enabled
    if let Some(ref gpu_mode) = svc_state.gpu_mode {
        if !def.metadata.gpu_apps.is_empty() {
            if let Ok(injected) = crate::gpu::inject_gpu(&compose_content, &def.metadata.gpu_apps, gpu_mode) {
                compose_content = injected;
            }
        }
    }

    crate::compose::validate_compose(&compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    let compose_path = svc_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, format!("Failed to write app configuration: {e}")).into_response())?;
    crate::compose::restrict_file_permissions(&compose_path);

    // Restart app
    if let Ok(compose_cmd) = crate::compose::detect_command().await {
        let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d"]).await;
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

    // Regenerate compose file
    let global_config = config::load_global_config(&state.data_dir).unwrap_or_default();
    let storage_env =
        config::resolve_storage_paths(&state.data_dir, &id, def, &global_config, &svc_state);

    let mut merged_env = crate::compose::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    let bind_ip = if svc_state.lan_accessible { "0.0.0.0" } else { "127.0.0.1" };
    merged_env.insert("BIND_IP".to_string(), bind_ip.to_string());

    if def.compose_template.contains("${SERVER_IP}") {
        if let Some(ip) = stats::get_server_ip() {
            merged_env.insert("SERVER_IP".to_string(), ip);
        }
    }

    let svc_dir = config::app_dir(&state.data_dir, &id);
    let mut compose_content = crate::compose::generate_compose(def, &merged_env);

    // Re-inject Tailscale sidecar if enabled
    if let Ok(Some(ts_cfg)) = config::load_tailscale_config(&state.data_dir) {
        if ts_cfg.enabled && !svc_state.tailscale_disabled {
            let mode = &def.metadata.tailscale_mode;
            if mode != "skip" {
                let port = crate::tailscale::extract_container_port(&compose_content).unwrap_or(80);
                let proxy_target = if mode == "network" {
                    format!("http://myground-{id}:{port}")
                } else {
                    format!("http://127.0.0.1:{port}")
                };
                if let Ok(injected) = crate::tailscale::inject_tailscale_sidecar(
                    &compose_content, &id, port, mode, None,
                    svc_state.tailscale_hostname.as_deref(),
                ) {
                    compose_content = injected;
                    let _ = crate::tailscale::write_serve_config(&svc_dir, port, &proxy_target);
                }
            }
        }
    }

    // Inject GPU
    if let Some(ref gpu_mode) = svc_state.gpu_mode {
        if let Ok(injected) = crate::gpu::inject_gpu(&compose_content, &def.metadata.gpu_apps, gpu_mode) {
            compose_content = injected;
        }
    }

    crate::compose::validate_compose(&compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, e.to_string()).into_response())?;

    let compose_path = svc_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_content)
        .map_err(|e| action_err(StatusCode::BAD_REQUEST, format!("Failed to write app configuration: {e}")).into_response())?;
    crate::compose::restrict_file_permissions(&compose_path);

    // Restart app
    if let Ok(compose_cmd) = crate::compose::detect_command().await {
        let _ = crate::compose::run(&compose_cmd, &svc_dir, &["up", "-d"]).await;
    }

    let msg = match body.mode.as_str() {
        "none" => format!("GPU acceleration disabled for {id}"),
        mode => format!("GPU acceleration set to {mode} for {id}"),
    };
    Ok(action_ok(msg))
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
    match crate::registry::get_app_icon(&id) {
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
