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

/// Resolve `${SERVER_IP}`, `${PORT}`, and `${TAILSCALE_URL}` placeholders in post-install notes.
/// Lines containing unresolved `${...}` variables are stripped from the output.
fn resolve_post_install_notes(
    template: Option<&str>,
    installed: bool,
    port: Option<u16>,
    tailscale_url: Option<&str>,
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
        if let Some(url) = tailscale_url {
            resolved = resolved.replace("${TAILSCALE_URL}", url);
        }
    }
    // Strip segments that still contain unresolved ${...} variables.
    // Notes use literal "\n" (two chars) as line separator (from TOML).
    let filtered: Vec<&str> = resolved
        .split("\\n")
        .filter(|segment| !segment.contains("${"))
        .collect();
    Some(filtered.join("\\n"))
}

/// Computed app status result.
struct ComputedStatus {
    status: String,
    status_detail: String,
    ready: bool,
}

/// Compute authoritative app status from container states.
///
/// Priority order:
/// 1. Not installed → "not_installed"
/// 2. Deploying → "deploying"
/// 3. No containers → "stopped"
/// 4. Any Restarting or dead → "crashing"
/// 5. No running + all created → "starting" (pulling images)
/// 6. No running + failed exits → "crashing"
/// 7. No running (exited 0 only) → "stopped"
/// 8. Running + health: starting → "health_checking"
/// 9. Running + has_health_check but no annotations → "starting"
/// 10. Healthy + tailscale sidecar not serving → "running" but not ready
/// 11. Fully ready → "running" + ready
fn compute_app_status(
    installed: bool,
    deploying: bool,
    containers: &[ContainerStatus],
    has_health_check: bool,
    has_tailscale: bool,
    sidecar_serving: Option<bool>,
) -> ComputedStatus {
    // 1. Not installed
    if !installed {
        return ComputedStatus {
            status: "not_installed".into(),
            status_detail: "Not installed".into(),
            ready: false,
        };
    }

    // 2. Deploying
    if deploying {
        return ComputedStatus {
            status: "deploying".into(),
            status_detail: "Deploying containers...".into(),
            ready: false,
        };
    }

    // 3. No containers
    if containers.is_empty() {
        return ComputedStatus {
            status: "stopped".into(),
            status_detail: "All containers stopped".into(),
            ready: false,
        };
    }

    // 4. Any Restarting or dead
    for c in containers {
        if c.status.contains("Restarting") {
            return ComputedStatus {
                status: "crashing".into(),
                status_detail: format!("Container {} restarting", c.name),
                ready: false,
            };
        }
        if c.state == "dead" {
            return ComputedStatus {
                status: "crashing".into(),
                status_detail: format!("Container {} crashed", c.name),
                ready: false,
            };
        }
    }

    let running: Vec<&ContainerStatus> = containers.iter().filter(|c| c.state == "running").collect();

    if running.is_empty() {
        // 5. No running + all created → starting (pulling images)
        if containers.iter().all(|c| c.state == "created") {
            return ComputedStatus {
                status: "starting".into(),
                status_detail: "Pulling images...".into(),
                ready: false,
            };
        }

        // 6. No running + any failed exit
        let failed = containers.iter().find(|c| {
            c.state == "exited" && !c.status.contains("(0)")
        });
        if let Some(c) = failed {
            return ComputedStatus {
                status: "crashing".into(),
                status_detail: format!("Container {} exited with error", c.name),
                ready: false,
            };
        }

        // 7. No running, exited 0 only → stopped
        return ComputedStatus {
            status: "stopped".into(),
            status_detail: "All containers stopped".into(),
            ready: false,
        };
    }

    // 8. Running + health: starting → health_checking
    let health_starting: Vec<&&ContainerStatus> = running.iter()
        .filter(|c| c.status.contains("health: starting"))
        .collect();
    if !health_starting.is_empty() {
        let healthy_count = running.iter().filter(|c| c.status.contains("(healthy)")).count();
        let total_health = healthy_count + health_starting.len();
        return ComputedStatus {
            status: "health_checking".into(),
            status_detail: format!("Health check in progress ({}/{} ready)", healthy_count, total_health),
            ready: false,
        };
    }

    // 9. Running + has_health_check but no health annotations yet → starting
    if has_health_check {
        let any_healthy = running.iter().any(|c| c.status.contains("(healthy)"));
        if !any_healthy {
            return ComputedStatus {
                status: "starting".into(),
                status_detail: "Waiting for health check...".into(),
                ready: false,
            };
        }
    }

    // 10/11. Healthy — check tailscale sidecar
    if has_tailscale {
        if let Some(serving) = sidecar_serving {
            if !serving {
                return ComputedStatus {
                    status: "running".into(),
                    status_detail: "Tailscale sidecar connecting...".into(),
                    ready: false,
                };
            }
        }
    }

    // 11. Fully ready
    let detail = if has_health_check {
        "All containers healthy".into()
    } else {
        "All containers running".into()
    };
    ComputedStatus {
        status: "running".into(),
        status_detail: detail,
        ready: true,
    }
}

/// Build a AppInfo from a definition, state, and container map.
fn build_app_info(
    id: &str,
    def: &AppDefinition,
    svc_state: &InstalledAppState,
    containers: &HashMap<String, Vec<ContainerStatus>>,
    tailscale_tailnet: Option<&str>,
    deploying: bool,
    sidecar_serving: Option<bool>,
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

    let default_ts_hostname = format!("myground-{id}");
    let ts_hostname = svc_state.tailscale_hostname.as_deref().unwrap_or(&default_ts_hostname);
    let tailscale_url = if svc_state.installed && !svc_state.tailscale_disabled {
        tailscale_tailnet.map(|tn| format!("https://{ts_hostname}.{tn}"))
    } else {
        None
    };

    let post_install_notes = resolve_post_install_notes(
        def.metadata.post_install_notes.as_deref(),
        svc_state.installed,
        svc_state.port,
        tailscale_url.as_deref(),
    );

    let domain_url = svc_state.domain.as_ref().map(|d| {
        let fqdn = crate::cloudflare::build_fqdn(&d.subdomain, &d.zone_name);
        format!("https://{fqdn}")
    });

    let uses_host_network = def.compose_template.contains("network_mode: host");
    let supports_tailscale = def.metadata.tailscale_mode != "skip";

    let app_containers = containers.get(id).cloned().unwrap_or_default();
    let has_tailscale = svc_state.installed && !svc_state.tailscale_disabled && supports_tailscale;
    let computed = compute_app_status(
        svc_state.installed,
        deploying,
        &app_containers,
        def.health.is_some(),
        has_tailscale,
        sidecar_serving,
    );

    AppInfo {
        id: id.to_string(),
        name,
        description: def.metadata.description.clone(),
        icon: def.metadata.icon.clone(),
        category: def.metadata.category.clone(),
        installed: svc_state.installed,
        has_storage: !def.storage.is_empty(),
        backup_supported: def.metadata.backup_supported,
        containers: app_containers,
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
        has_health_check: def.health.is_some(),
        deploying,
        status: computed.status,
        status_detail: computed.status_detail,
        ready: computed.ready,
        vpn_enabled: crate::vpn::is_vpn_enabled(svc_state),
        vpn_provider: svc_state
            .vpn
            .as_ref()
            .filter(|v| v.enabled)
            .and_then(|v| v.provider.clone()),
        storage_volumes: def.storage.clone(),
    }
}

// ── Available apps ──────────────────────────────────────────────────────────

#[derive(Serialize, ToSchema)]
pub struct AvailableApp {
    pub id: String,
    #[serde(flatten)]
    pub metadata: AppMetadata,
    pub has_storage: bool,
    pub has_health_check: bool,
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
            has_health_check: def.health.is_some(),
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
    pub has_health_check: bool,
    pub deploying: bool,
    /// Backend-computed status: "not_installed"|"stopped"|"deploying"|"starting"|"health_checking"|"running"|"crashing"
    pub status: String,
    /// Human-readable status detail, e.g. "Health check in progress (1/2 ready)"
    pub status_detail: String,
    /// True only when fully functional (healthy + tailscale serving if applicable)
    pub ready: bool,
    pub vpn_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vpn_provider: Option<String>,
    pub storage_volumes: Vec<crate::registry::StorageVolume>,
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

    // Read deploying set upfront so we can pass it into build_app_info
    let deploying_set = state.deploying.read().unwrap().clone();

    // Collect (id, def, state) tuples so we can check sidecar status
    struct AppEntry {
        id: String,
        def_idx: usize, // index into a defs vec
        svc_state: InstalledAppState,
    }
    let mut defs: Vec<&AppDefinition> = Vec::new();
    let mut entries: Vec<AppEntry> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Registry apps
    for (id, def) in state.registry.iter() {
        let svc_state = if installed.contains(id) {
            config::load_app_state(&state.data_dir, id).unwrap_or_default()
        } else {
            InstalledAppState::default()
        };
        let idx = defs.len();
        defs.push(def);
        entries.push(AppEntry { id: id.clone(), def_idx: idx, svc_state });
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
            let idx = defs.len();
            defs.push(def);
            entries.push(AppEntry { id: id.clone(), def_idx: idx, svc_state });
        }
    }

    // Check sidecar serving for apps that are healthy + have tailscale enabled.
    // We run these concurrently to limit latency.
    let mut sidecar_futures = Vec::new();
    let ts_enabled = ts_cfg.enabled;
    for entry in &entries {
        let is_deploying = deploying_set.contains(&entry.id);
        let def = defs[entry.def_idx];
        let supports_ts = def.metadata.tailscale_mode != "skip";
        let has_ts = entry.svc_state.installed
            && !entry.svc_state.tailscale_disabled
            && supports_ts
            && ts_enabled;

        if !has_ts || is_deploying {
            sidecar_futures.push(None);
            continue;
        }

        // Only check sidecar if app has running containers and is otherwise healthy
        let containers = container_map.get(&entry.id);
        let any_running = containers
            .map(|cs| cs.iter().any(|c| c.state == "running"))
            .unwrap_or(false);
        if any_running {
            sidecar_futures.push(Some(entry.id.clone()));
        } else {
            sidecar_futures.push(None);
        }
    }

    // Run sidecar checks concurrently
    let mut sidecar_results: Vec<Option<bool>> = vec![None; entries.len()];
    let checks: Vec<_> = sidecar_futures
        .iter()
        .enumerate()
        .filter_map(|(i, id)| id.as_ref().map(|id| (i, id.clone())))
        .collect();
    if !checks.is_empty() {
        let futs: Vec<_> = checks.iter()
            .map(|(_, id)| crate::tailscale::is_sidecar_serving(id))
            .collect();
        let results = futures_util::future::join_all(futs).await;
        for ((i, _), result) in checks.iter().zip(results) {
            sidecar_results[*i] = Some(result);
        }
    }

    // Build AppInfo for each entry
    let mut apps: Vec<AppInfo> = Vec::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        let is_deploying = deploying_set.contains(&entry.id);
        let def = defs[entry.def_idx];
        apps.push(build_app_info(
            &entry.id,
            def,
            &entry.svc_state,
            &container_map,
            tailnet,
            is_deploying,
            sidecar_results[i],
        ));
    }

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

// ── VPN test (WebSocket) ──────────────────────────────────────────────

pub async fn vpn_test_ws(
    State(state): State<AppState>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> impl IntoResponse {
    let guard = match state.try_ws_slot("__vpn_test__") {
        Some(g) => g,
        None => {
            return action_err(StatusCode::TOO_MANY_REQUESTS, "VPN test already in progress")
                .into_response()
        }
    };

    ws.on_upgrade(move |socket| handle_vpn_test_stream(socket, state, guard))
        .into_response()
}

async fn handle_vpn_test_stream(
    mut socket: axum::extract::ws::WebSocket,
    state: AppState,
    _guard: crate::state::WsGuard,
) {
    use axum::extract::ws::Message;

    // Wait for the first message — it may contain a VPN config JSON
    let config = if let Some(Ok(Message::Text(text))) = socket.recv().await {
        let text: &str = &text;
        if let Ok(cfg) = serde_json::from_str::<VpnConfig>(text) {
            if cfg.provider.is_some() {
                cfg
            } else {
                config::load_vpn_config(&state.data_dir)
                    .ok()
                    .flatten()
                    .unwrap_or_default()
            }
        } else {
            config::load_vpn_config(&state.data_dir)
                .ok()
                .flatten()
                .unwrap_or_default()
        }
    } else {
        config::load_vpn_config(&state.data_dir)
            .ok()
            .flatten()
            .unwrap_or_default()
    };

    if config.provider.is_none() {
        let _ = socket
            .send(Message::Text("Error: No VPN configuration".into()))
            .await;
        return;
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);

    let test_task = tokio::spawn(async move {
        crate::vpn::test_vpn_connection_streaming(&config, tx).await
    });

    // Forward lines to WebSocket
    while let Some(line) = rx.recv().await {
        if socket.send(Message::Text(line.into())).await.is_err() {
            break;
        }
    }

    // Drop the receiver so cleanup code in the test task doesn't block
    // trying to send log messages to a full channel.
    drop(rx);

    match test_task.await {
        Ok(Ok(())) => {
            let _ = socket.send(Message::Text("__DONE__".into())).await;
        }
        Ok(Err(e)) => {
            let _ = socket.send(Message::Text(format!("__FAIL__{e}").into())).await;
        }
        Err(e) => {
            let _ = socket.send(Message::Text(format!("__FAIL__{e}").into())).await;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cs(name: &str, state: &str, status: &str) -> ContainerStatus {
        ContainerStatus {
            name: name.to_string(),
            state: state.to_string(),
            status: status.to_string(),
        }
    }

    #[test]
    fn status_not_installed() {
        let r = compute_app_status(false, false, &[], false, false, None);
        assert_eq!(r.status, "not_installed");
        assert!(!r.ready);
    }

    #[test]
    fn status_deploying() {
        let r = compute_app_status(true, true, &[], false, false, None);
        assert_eq!(r.status, "deploying");
        assert!(!r.ready);
    }

    #[test]
    fn status_stopped_no_containers() {
        let r = compute_app_status(true, false, &[], false, false, None);
        assert_eq!(r.status, "stopped");
        assert!(!r.ready);
    }

    #[test]
    fn status_crashing_restarting() {
        let containers = vec![
            cs("app", "running", "Restarting (1) 5s ago"),
        ];
        let r = compute_app_status(true, false, &containers, false, false, None);
        assert_eq!(r.status, "crashing");
        assert!(r.status_detail.contains("restarting"));
    }

    #[test]
    fn status_crashing_dead() {
        let containers = vec![
            cs("app", "dead", ""),
        ];
        let r = compute_app_status(true, false, &containers, false, false, None);
        assert_eq!(r.status, "crashing");
        assert!(r.status_detail.contains("crashed"));
    }

    #[test]
    fn status_starting_all_created() {
        let containers = vec![
            cs("app", "created", "Created"),
        ];
        let r = compute_app_status(true, false, &containers, false, false, None);
        assert_eq!(r.status, "starting");
        assert!(r.status_detail.contains("Pulling"));
    }

    #[test]
    fn status_crashing_exited_nonzero() {
        let containers = vec![
            cs("app", "exited", "Exited (1) 30s ago"),
        ];
        let r = compute_app_status(true, false, &containers, false, false, None);
        assert_eq!(r.status, "crashing");
        assert!(r.status_detail.contains("exited with error"));
    }

    #[test]
    fn status_stopped_exited_zero() {
        let containers = vec![
            cs("app", "exited", "Exited (0) 30s ago"),
        ];
        let r = compute_app_status(true, false, &containers, false, false, None);
        assert_eq!(r.status, "stopped");
    }

    #[test]
    fn status_health_checking() {
        let containers = vec![
            cs("app", "running", "Up 10s (health: starting)"),
        ];
        let r = compute_app_status(true, false, &containers, true, false, None);
        assert_eq!(r.status, "health_checking");
        assert!(r.status_detail.contains("Health check"));
        assert!(r.status_detail.contains("0/1"));
    }

    #[test]
    fn status_starting_waiting_for_healthcheck() {
        let containers = vec![
            cs("app", "running", "Up 2s"),
        ];
        let r = compute_app_status(true, false, &containers, true, false, None);
        assert_eq!(r.status, "starting");
        assert!(r.status_detail.contains("Waiting for health check"));
    }

    #[test]
    fn status_running_sidecar_not_serving() {
        let containers = vec![
            cs("app", "running", "Up 30s (healthy)"),
        ];
        let r = compute_app_status(true, false, &containers, true, true, Some(false));
        assert_eq!(r.status, "running");
        assert!(!r.ready);
        assert!(r.status_detail.contains("Tailscale"));
    }

    #[test]
    fn status_fully_ready() {
        let containers = vec![
            cs("app", "running", "Up 30s (healthy)"),
        ];
        let r = compute_app_status(true, false, &containers, true, true, Some(true));
        assert_eq!(r.status, "running");
        assert!(r.ready);
        assert!(r.status_detail.contains("healthy"));
    }

    #[test]
    fn status_running_no_healthcheck() {
        let containers = vec![
            cs("app", "running", "Up 10m"),
        ];
        let r = compute_app_status(true, false, &containers, false, false, None);
        assert_eq!(r.status, "running");
        assert!(r.ready);
    }

    #[test]
    fn status_running_no_tailscale() {
        let containers = vec![
            cs("app", "running", "Up 30s (healthy)"),
        ];
        let r = compute_app_status(true, false, &containers, true, false, None);
        assert_eq!(r.status, "running");
        assert!(r.ready);
    }

    #[test]
    fn status_health_checking_partial() {
        let containers = vec![
            cs("app", "running", "Up 30s (healthy)"),
            cs("db", "running", "Up 10s (health: starting)"),
        ];
        let r = compute_app_status(true, false, &containers, true, false, None);
        assert_eq!(r.status, "health_checking");
        assert!(r.status_detail.contains("1/2"));
    }
}
