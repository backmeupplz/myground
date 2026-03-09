use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use crate::compose;
use crate::config::{self, InstalledAppState};
use crate::error::AppError;
use crate::registry::AppDefinition;

/// Result returned after a successful install.
pub struct InstallResult {
    pub instance_id: String,
    pub port: u16,
}

pub const PORT_RANGE_START: u16 = 9000;
pub const PORT_RANGE_END: u16 = 9999;

// ── Port allocation ─────────────────────────────────────────────────────────

/// Collect all ports already in use by installed apps.
pub fn used_ports(base: &Path) -> HashSet<u16> {
    let mut ports = HashSet::new();

    for id in config::list_installed_apps(base) {
        if let Ok(state) = config::load_app_state(base, &id) {
            if let Some(p) = state.port {
                ports.insert(p);
            }
            for val in state.env_overrides.values() {
                if let Ok(p) = val.parse::<u16>() {
                    ports.insert(p);
                }
            }
        }
    }

    ports
}

/// Check if a port is actually bound on the system (defense-in-depth).
fn is_port_bound(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_err()
        || std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// Allocate the next free port in 9000-9999.
/// Checks both MyGround state files and system-level port availability.
pub fn allocate_port(base: &Path) -> Result<u16, AppError> {
    let in_use = used_ports(base);
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if !in_use.contains(&port) && !is_port_bound(port) {
            return Ok(port);
        }
    }
    Err(AppError::Io("No free ports in range 9000-9999".to_string()))
}

// ── Instance ID management ──────────────────────────────────────────────────

/// Generate the next instance ID for a multi-instance app.
pub fn next_instance_id(base: &Path, base_id: &str) -> String {
    let installed = config::list_installed_apps(base);
    if !installed.contains(&base_id.to_string()) {
        return base_id.to_string();
    }
    for n in 2u32.. {
        let candidate = format!("{base_id}-{n}");
        if !installed.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

/// Determine the instance ID for an app install.
/// All apps support multiple instances: the first install uses the base ID,
/// subsequent installs get `-2`, `-3`, etc.
fn resolve_instance_id(
    base: &Path,
    app_id: &str,
) -> Result<String, AppError> {
    let existing = config::load_app_state(base, app_id);
    if existing.is_ok() && existing.unwrap().installed {
        Ok(next_instance_id(base, app_id))
    } else {
        Ok(app_id.to_string())
    }
}

// ── Definition lookup ───────────────────────────────────────────────────────

/// Look up an app definition by ID: check registry first, then check if
/// the app state has a `definition_id` pointing to a parent template.
pub fn lookup_definition<'a>(
    app_id: &str,
    registry: &'a HashMap<String, AppDefinition>,
    base: &Path,
) -> Result<&'a AppDefinition, AppError> {
    if let Some(def) = registry.get(app_id) {
        return Ok(def);
    }
    let state = config::load_app_state(base, app_id).unwrap_or_default();
    if let Some(ref parent_id) = state.definition_id {
        if let Some(def) = registry.get(parent_id) {
            return Ok(def);
        }
    }
    Err(AppError::NotFound(app_id.to_string()))
}

// ── Compose pipeline helpers ────────────────────────────────────────────────

/// Build the full merged environment for compose template substitution.
///
/// Combines defaults, env_overrides, storage paths, BIND_IP, and SERVER_IP.
pub fn build_merged_env(
    base: &Path,
    id: &str,
    def: &AppDefinition,
    svc_state: &InstalledAppState,
) -> HashMap<String, String> {
    let global_config = config::load_global_config(base).unwrap_or_default();
    let storage_env = config::resolve_storage_paths(base, id, def, &global_config, svc_state);

    let mut merged = compose::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        merged.insert(k.clone(), v.clone());
    }

    // Inject EXIT_PORT from saved app state
    if let Some(port) = svc_state.port {
        merged.insert("EXIT_PORT".to_string(), port.to_string());
    }

    let bind_ip = if svc_state.lan_accessible { "0.0.0.0" } else { "127.0.0.1" };
    merged.insert("BIND_IP".to_string(), bind_ip.to_string());

    if def.compose_template.contains("${SERVER_IP}") {
        if let Some(ip) = crate::stats::get_server_ip() {
            merged.insert("SERVER_IP".to_string(), ip);
        }
    }

    merged
}

/// Determine effective Tailscale mode, accounting for VPN.
///
/// When VPN is active, "sidecar" mode is forced to "network" because
/// only one `network_mode` is allowed per container.
pub fn effective_tailscale_mode<'a>(mode: &'a str, vpn_active: bool) -> &'a str {
    if vpn_active && mode == "sidecar" { "network" } else { mode }
}

/// Build the Tailscale proxy target URL.
///
/// `main_service` is the Docker Compose service name (YAML key) of the main app
/// service.  In "network" mode the sidecar and main service share a Docker
/// network, so the compose service name is used for DNS resolution.
pub fn tailscale_proxy_target(
    id: &str,
    port: u16,
    effective_mode: &str,
    vpn_active: bool,
    main_service: Option<&str>,
) -> String {
    if effective_mode == "network" {
        if vpn_active {
            format!("http://myground-{id}-vpn:{port}")
        } else {
            // Use the compose service name for DNS resolution on the shared
            // ts-net-{id} network.  Docker Compose creates DNS aliases for
            // service names on every attached network, so this always works.
            // Falls back to container-name format if service name is unknown.
            let host = main_service
                .unwrap_or_else(|| id);
            format!("http://{host}:{port}")
        }
    } else {
        format!("http://127.0.0.1:{port}")
    }
}

/// Apply all sidecar injections (VPN → Tailscale → GPU) to compose content.
///
/// Returns an error only if VPN injection fails (e.g. host-network app).
/// Tailscale and GPU failures are logged and swallowed.
pub fn inject_all_sidecars(
    compose_content: &str,
    base: &Path,
    id: &str,
    def: &AppDefinition,
    svc_state: &InstalledAppState,
    svc_dir: &Path,
    tailscale_auth_key: Option<&str>,
) -> Result<String, AppError> {
    let mut content = compose_content.to_string();
    let vpn_active = crate::vpn::is_vpn_enabled(svc_state);

    // 1. VPN sidecar
    if vpn_active {
        if let Some(ref vpn_cfg) = svc_state.vpn {
            content = crate::vpn::inject_vpn_sidecar(&content, id, vpn_cfg)?;
            crate::vpn::write_vpn_env(
                svc_dir,
                vpn_cfg,
                def.metadata.vpn_port_forward_command.as_deref(),
            )?;
        }
    }

    // 2. Tailscale sidecar
    if let Ok(Some(ts_cfg)) = config::load_tailscale_config(base) {
        if ts_cfg.enabled && !svc_state.tailscale_disabled {
            let mode = &def.metadata.tailscale_mode;
            let eff_mode = effective_tailscale_mode(mode, vpn_active);
            if eff_mode != "skip" {
                let toml_port = def.health.as_ref().and_then(|h| h.container_port).unwrap_or(80);
                let main_svc = crate::tailscale::extract_main_service_name(&content);
                let port = crate::tailscale::extract_main_service_container_port(&content)
                    .unwrap_or(toml_port);
                let proxy_target = tailscale_proxy_target(id, port, eff_mode, vpn_active, main_svc.as_deref());
                match crate::tailscale::inject_tailscale_sidecar(
                    &content, id, port, eff_mode, tailscale_auth_key,
                    svc_state.tailscale_hostname.as_deref(),
                ) {
                    Ok(injected) => {
                        content = injected;
                        let _ = crate::tailscale::write_serve_config(svc_dir, &proxy_target);
                        let env_path = svc_dir.join("ts-sidecar.env");
                        if let Some(key) = tailscale_auth_key {
                            let _ = std::fs::write(&env_path, format!("TS_AUTHKEY={key}\n"));
                            compose::restrict_file_permissions(&env_path);
                        } else if !env_path.exists() {
                            let _ = std::fs::write(&env_path, "");
                            compose::restrict_file_permissions(&env_path);
                        }
                    }
                    Err(e) => tracing::warn!("Tailscale sidecar inject failed for {id}: {e}"),
                }
            }
        }
    }

    // 3. GPU passthrough
    if let Some(ref gpu_mode) = svc_state.gpu_mode {
        if !def.metadata.gpu_apps.is_empty() {
            if let Ok(injected) = crate::gpu::inject_gpu(&content, &def.metadata.gpu_apps, gpu_mode) {
                content = injected;
            }
        }
    }

    Ok(content)
}

/// Regenerate an app's compose file with all sidecars, validate, and write.
///
/// Returns the app's service directory path on success.
/// Does NOT restart the app — caller handles that.
pub fn regenerate_compose(
    base: &Path,
    id: &str,
    def: &AppDefinition,
    svc_state: &InstalledAppState,
) -> Result<std::path::PathBuf, AppError> {
    let merged_env = build_merged_env(base, id, def, svc_state);
    let svc_dir = config::app_dir(base, id);
    let compose_content = compose::generate_compose(def, &merged_env);

    let final_content = inject_all_sidecars(
        &compose_content, base, id, def, svc_state, &svc_dir, None,
    )?;

    compose::validate_compose(&final_content)?;

    let compose_path = svc_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &final_content)
        .map_err(|e| AppError::Io(format!("Write compose file: {e}")))?;
    compose::restrict_file_permissions(&compose_path);

    Ok(svc_dir)
}

// ── Install helpers ─────────────────────────────────────────────────────────

/// Write docker-compose.yml and .env files for an app.
/// If Tailscale is enabled, injects TSDProxy labels into the compose file.
fn write_app_files(
    base: &Path,
    svc_dir: &Path,
    instance_id: &str,
    def: &AppDefinition,
    merged_env: &HashMap<String, String>,
    env_overrides: &HashMap<String, String>,
    storage_env: &HashMap<String, String>,
    tailscale_auth_key: Option<&str>,
) -> Result<(), AppError> {
    std::fs::create_dir_all(svc_dir)
        .map_err(|e| AppError::Io(format!("Create app dir: {e}")))?;

    let mut compose_content = compose::generate_compose(def, merged_env);

    // Apply all sidecar injections (VPN → Tailscale → GPU)
    let svc_state = config::load_app_state(base, instance_id).unwrap_or_default();
    match inject_all_sidecars(&compose_content, base, instance_id, def, &svc_state, svc_dir, tailscale_auth_key) {
        Ok(injected) => compose_content = injected,
        Err(e) => tracing::warn!("Sidecar injection failed for {instance_id}: {e}"),
    }

    compose::validate_compose(&compose_content)?;

    let compose_path = svc_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_content)
        .map_err(|e| AppError::Io(format!("Write compose file: {e}")))?;
    compose::restrict_file_permissions(&compose_path);

    let mut env_with_storage = env_overrides.clone();
    for (k, v) in storage_env {
        env_with_storage.insert(k.clone(), v.clone());
    }
    let env_content = compose::generate_env_file(&def.defaults, &env_with_storage);
    let env_path = svc_dir.join(".env");
    std::fs::write(&env_path, &env_content)
        .map_err(|e| AppError::Io(format!("Write .env: {e}")))?;
    compose::restrict_file_permissions(&env_path);

    Ok(())
}

/// Auto-generate a display name for multi-instance apps.
/// e.g. instance "filebrowser-3" of "filebrowser" → "File Browser 3"
fn auto_display_name(app_id: &str, instance_id: &str, base_name: &str) -> Option<String> {
    if instance_id == app_id {
        return None;
    }
    let suffix = instance_id.strip_prefix(app_id)?.strip_prefix('-')?;
    Some(format!("{base_name} {suffix}"))
}

// ── Install ─────────────────────────────────────────────────────────────────

/// Install an app: setup files + pull + start (blocking).
///
/// For streaming progress, use `install_app_setup` + `compose::deploy_streaming`.
pub async fn install_app(
    base: &Path,
    registry: &HashMap<String, AppDefinition>,
    app_id: &str,
    storage_path: Option<&str>,
    variables: Option<&HashMap<String, String>>,
    tailscale_auth_key: Option<&str>,
) -> Result<InstallResult, AppError> {
    let result = install_app_setup(base, registry, app_id, storage_path, variables, None, tailscale_auth_key)?;

    let svc_dir = config::app_dir(base, &result.instance_id);
    let compose_cmd = compose::detect_command().await?;
    compose::run(&compose_cmd, &svc_dir, &["pull"]).await?;
    compose::run(&compose_cmd, &svc_dir, &["up", "-d"]).await?;

    Ok(result)
}

/// Setup-only install: write files, save state, allocate port. Does NOT pull or start.
pub fn install_app_setup(
    base: &Path,
    registry: &HashMap<String, AppDefinition>,
    app_id: &str,
    storage_path: Option<&str>,
    variables: Option<&HashMap<String, String>>,
    display_name: Option<&str>,
    tailscale_auth_key: Option<&str>,
) -> Result<InstallResult, AppError> {
    let def = registry
        .get(app_id)
        .ok_or_else(|| AppError::NotFound(app_id.to_string()))?;

    let instance_id = resolve_instance_id(base, app_id)?;
    let port = allocate_port(base)?;

    // Build env overrides with allocated port + install variables
    let mut env_overrides = HashMap::new();
    env_overrides.insert("EXIT_PORT".to_string(), port.to_string());
    if let Some(vars) = variables {
        for (k, v) in vars {
            compose::validate_env_key(k)?;
            compose::validate_env_value(v)?;
            // Path-type variables are used as Docker bind-mount volumes — validate
            // them the same way we validate storage paths to block sensitive dirs.
            if def.install_variables.iter().any(|iv| iv.key == *k && iv.input_type == "path") {
                config::validate_storage_path(v)?;
            }
            env_overrides.insert(k.clone(), v.clone());
        }
    }

    // Validate storage path against traversal
    if let Some(sp) = storage_path {
        config::validate_storage_path(sp)?;
    }

    // Build storage path overrides — no myground/ prefix, just volume subdirs
    let mut storage_overrides = HashMap::new();
    if let Some(sp) = storage_path {
        let sp = sp.trim_end_matches('/');
        for vol in &def.storage {
            storage_overrides.insert(vol.name.clone(), format!("{sp}/{}/", vol.name));
        }
    }

    // Resolve and create storage directories
    let global_config = config::load_global_config(base).unwrap_or_default();
    let pre_state = InstalledAppState {
        storage_paths: storage_overrides.clone(),
        ..Default::default()
    };
    let storage_env =
        config::resolve_storage_paths(base, &instance_id, def, &global_config, &pre_state);

    for path in storage_env.values() {
        std::fs::create_dir_all(path)
            .map_err(|e| AppError::Io(format!("Create storage dir: {e}")))?;
    }

    // Build full environment
    let mut merged_env = compose::merge_env(&def.defaults, &env_overrides);
    for (k, v) in &storage_env {
        merged_env.insert(k.clone(), v.clone());
    }

    // Inject SERVER_IP for templates that need it (e.g. Pi-hole DNS port binding)
    if def.compose_template.contains("${SERVER_IP}") {
        if let Some(ip) = crate::stats::get_server_ip() {
            merged_env.insert("SERVER_IP".to_string(), ip);
        }
    }

    // Default to localhost-only binding (security hardening)
    merged_env.insert("BIND_IP".to_string(), "127.0.0.1".to_string());

    // For multi-instance, adjust container names in compose template
    let prefix = crate::docker::CONTAINER_PREFIX;
    let adjusted_def = if instance_id != app_id {
        AppDefinition {
            compose_template: def.compose_template.replace(
                &format!("{prefix}{app_id}"),
                &format!("{prefix}{instance_id}"),
            ),
            ..def.clone()
        }
    } else {
        def.clone()
    };

    // Write compose + .env files
    let svc_dir = config::app_dir(base, &instance_id);
    write_app_files(base, &svc_dir, &instance_id, &adjusted_def, &merged_env, &env_overrides, &storage_env, tailscale_auth_key)?;

    // Build state storage_paths (vol name → resolved path)
    let mut state_storage_paths = storage_overrides;
    for vol in &def.storage {
        state_storage_paths
            .entry(vol.name.clone())
            .or_insert_with(|| {
                storage_env
                    .get(&format!("STORAGE_{}", vol.name))
                    .cloned()
                    .unwrap_or_default()
            });
    }

    // Save state
    let state = InstalledAppState {
        installed: true,
        env_overrides,
        storage_paths: state_storage_paths,
        port: Some(port),
        definition_id: if instance_id != app_id {
            Some(app_id.to_string())
        } else {
            None
        },
        display_name: display_name
            .map(|s| s.to_string())
            .or_else(|| auto_display_name(app_id, &instance_id, &def.metadata.name)),
        backup_jobs: Vec::new(),
        _backup_legacy: None,
        backup_password: None,
        last_backup_at: None,
        tailscale_disabled: false,
        tailscale_hostname: None,
        lan_accessible: false,
        gpu_mode: None,
        image_digest: None,
        latest_image_digest: None,
        update_available: false,
        last_update_check: None,
        domain: None,
        vpn: None,
    };
    config::save_app_state(base, &instance_id, &state)?;

    Ok(InstallResult {
        instance_id,
        port,
    })
}

// ── Lifecycle ───────────────────────────────────────────────────────────────

/// Verify an app is installed, returning its state.
fn require_installed(base: &Path, app_id: &str) -> Result<InstalledAppState, AppError> {
    let state = config::load_app_state(base, app_id)?;
    if !state.installed {
        return Err(AppError::NotInstalled(app_id.to_string()));
    }
    Ok(state)
}

/// Start an app.
pub async fn start_app(base: &Path, app_id: &str) -> Result<(), AppError> {
    require_installed(base, app_id)?;
    let svc_dir = config::app_dir(base, app_id);
    let compose_cmd = compose::detect_command().await?;
    compose::run(&compose_cmd, &svc_dir, &["up", "-d"]).await?;
    Ok(())
}

/// Stop an app.
pub async fn stop_app(base: &Path, app_id: &str) -> Result<(), AppError> {
    require_installed(base, app_id)?;
    let svc_dir = config::app_dir(base, app_id);
    let compose_cmd = compose::detect_command().await?;
    compose::run(&compose_cmd, &svc_dir, &["down"]).await?;
    Ok(())
}

/// Remove an app: compose down, delete app metadata directory.
/// Does NOT delete user data in storage paths — user data is sacred.
pub async fn remove_app(base: &Path, app_id: &str) -> Result<(), AppError> {
    let state = require_installed(base, app_id)?;
    let svc_dir = config::app_dir(base, app_id);
    let compose_cmd = compose::detect_command().await?;

    // Clean up Cloudflare domain binding if present (non-fatal)
    if state.domain.is_some() {
        if let Err(e) = crate::cloudflare::unbind_domain(base, app_id).await {
            tracing::warn!("Failed to clean up domain binding for {app_id}: {e}");
        }
    }

    // Log out Tailscale sidecar so the machine is removed from tailnet (best-effort)
    crate::tailscale::logout_sidecar(app_id).await;

    // Try to bring down containers and remove named volumes; ignore errors (may already be stopped)
    let _ = compose::run(
        &compose_cmd,
        &svc_dir,
        &["down", "--remove-orphans", "--volumes"],
    )
    .await;

    // Warn about external storage paths that will be left intact
    for (name, path) in &state.storage_paths {
        if !path.starts_with(&svc_dir.to_string_lossy().to_string()) {
            tracing::info!("Keeping storage data for '{name}' at: {path}");
        }
    }

    // Remove app metadata files; best-effort remove the whole directory
    if std::fs::remove_dir_all(&svc_dir).is_err() {
        let mut cleared = config::InstalledAppState::default();
        cleared.installed = false;
        let _ = config::save_app_state(base, app_id, &cleared);
        let _ = std::fs::remove_file(svc_dir.join("docker-compose.yml"));
        let _ = std::fs::remove_file(svc_dir.join(".env"));
    }

    Ok(())
}

/// Nuke everything: stop all apps, remove all containers, delete all data.
pub async fn nuke_all(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    // Clean up Tailscale exit node
    let exit_actions = crate::tailscale::cleanup_exit_node(base).await;
    actions.extend(exit_actions);

    // Clean up old TSDProxy if it exists (migration leftovers)
    let ts_actions = crate::tailscale::cleanup_tsdproxy(base).await;
    actions.extend(ts_actions);

    // Clean up Cloudflare tunnel
    let cf_actions = crate::cloudflare::cleanup_cloudflared(base).await;
    actions.extend(cf_actions);

    let installed = config::list_installed_apps(base);
    if let Ok(compose_cmd) = compose::detect_command().await {
        for id in &installed {
            let svc_dir = config::app_dir(base, id);
            if svc_dir.join("docker-compose.yml").exists() {
                crate::tailscale::logout_sidecar(id).await;
                let result =
                    compose::run(&compose_cmd, &svc_dir, &["down", "--remove-orphans", "--volumes"])
                        .await;
                match result {
                    Ok(_) => actions.push(format!("Stopped and removed containers for {id}")),
                    Err(e) => actions.push(format!("Warning: compose down for {id}: {e}")),
                }
            }
        }
    }

    // Force-remove any straggling myground-* containers
    let filter = format!("name={}", crate::docker::CONTAINER_PREFIX);
    if let Ok(output) = tokio::process::Command::new("docker")
        .args(["ps", "-a", "--filter", &filter, "--format", "{{.Names}}"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        let names = String::from_utf8_lossy(&output.stdout);
        for name in names.lines().filter(|n| !n.is_empty()) {
            let _ = tokio::process::Command::new("docker")
                .args(["rm", "-f", name])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .await;
            actions.push(format!("Removed straggling container: {name}"));
        }
    }

    // Remove the entire data directory
    if base.exists() {
        match std::fs::remove_dir_all(base) {
            Ok(()) => actions.push(format!("Removed data directory: {}", base.display())),
            Err(e) => actions.push(format!(
                "Warning: failed to remove {}: {e}",
                base.display()
            )),
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::dummy_app_def;

    #[test]
    fn used_ports_returns_empty_when_no_apps() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        assert!(used_ports(base).is_empty());
    }

    #[test]
    fn used_ports_includes_installed_app_ports() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let state = config::InstalledAppState {
            installed: true,
            port: Some(9005),
            ..Default::default()
        };
        config::save_app_state(base, "whoami", &state).unwrap();

        let ports = used_ports(base);
        assert!(ports.contains(&9005));
    }

    #[test]
    fn allocate_port_returns_first_free_in_range() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        let port = allocate_port(base).unwrap();
        assert_eq!(port, PORT_RANGE_START);
    }

    #[test]
    fn allocate_port_skips_used_ports() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let state = config::InstalledAppState {
            installed: true,
            port: Some(9000),
            ..Default::default()
        };
        config::save_app_state(base, "test", &state).unwrap();

        let port = allocate_port(base).unwrap();
        assert_eq!(port, 9001);
    }

    #[test]
    fn next_instance_id_returns_base_when_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        assert_eq!(next_instance_id(base, "filebrowser"), "filebrowser");
    }

    #[test]
    fn next_instance_id_returns_dash_2_when_installed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let state = config::InstalledAppState {
            installed: true,
            ..Default::default()
        };
        config::save_app_state(base, "filebrowser", &state).unwrap();

        assert_eq!(next_instance_id(base, "filebrowser"), "filebrowser-2");
    }

    #[test]
    fn auto_display_name_base_returns_none() {
        assert_eq!(auto_display_name("filebrowser", "filebrowser", "File Browser"), None);
    }

    #[test]
    fn auto_display_name_instance_returns_suffixed() {
        assert_eq!(
            auto_display_name("filebrowser", "filebrowser-2", "File Browser"),
            Some("File Browser 2".to_string())
        );
        assert_eq!(
            auto_display_name("filebrowser", "filebrowser-3", "File Browser"),
            Some("File Browser 3".to_string())
        );
    }

    #[test]
    fn auto_display_name_unrelated_returns_none() {
        assert_eq!(auto_display_name("immich", "filebrowser-2", "Immich"), None);
    }

    #[test]
    fn resolve_instance_id_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let result = resolve_instance_id(base, "whoami").unwrap();
        assert_eq!(result, "whoami");
    }

    #[test]
    fn resolve_instance_id_increments() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let result = resolve_instance_id(base, "whoami").unwrap();
        assert_eq!(result, "whoami");

        let state = config::InstalledAppState { installed: true, ..Default::default() };
        config::save_app_state(base, "whoami", &state).unwrap();

        let result = resolve_instance_id(base, "whoami").unwrap();
        assert_eq!(result, "whoami-2");
    }

    #[test]
    fn lookup_definition_direct_hit() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_app_def("whoami", "", HashMap::new(), Vec::new());
        registry.insert("whoami".to_string(), def);

        let result = lookup_definition("whoami", &registry, base);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().metadata.id, "whoami");
    }

    #[test]
    fn lookup_definition_via_parent() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_app_def("filebrowser", "", HashMap::new(), Vec::new());
        registry.insert("filebrowser".to_string(), def);

        let state = config::InstalledAppState {
            installed: true,
            definition_id: Some("filebrowser".to_string()),
            ..Default::default()
        };
        config::save_app_state(base, "filebrowser-2", &state).unwrap();

        let result = lookup_definition("filebrowser-2", &registry, base);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().metadata.id, "filebrowser");
    }

    #[test]
    fn lookup_definition_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        let registry = HashMap::new();

        let result = lookup_definition("nonexistent", &registry, base);
        assert!(result.is_err());
    }

}
