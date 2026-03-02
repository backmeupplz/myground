use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use crate::compose;
use crate::config::{self, ServiceState};
use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

/// Result returned after a successful install.
pub struct InstallResult {
    pub instance_id: String,
    pub port: u16,
}

pub const PORT_RANGE_START: u16 = 9000;
pub const PORT_RANGE_END: u16 = 9999;

// ── Port allocation ─────────────────────────────────────────────────────────

/// Collect all ports already in use by installed services and registry defaults.
pub fn used_ports(base: &Path, registry: &HashMap<String, ServiceDefinition>) -> HashSet<u16> {
    let mut ports = HashSet::new();

    for def in registry.values() {
        for val in def.defaults.values() {
            if let Ok(p) = val.parse::<u16>() {
                ports.insert(p);
            }
        }
    }

    for id in config::list_installed_services(base) {
        if let Ok(state) = config::load_service_state(base, &id) {
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

/// Allocate the next free port in 9000-9999.
pub fn allocate_port(base: &Path, registry: &HashMap<String, ServiceDefinition>) -> Result<u16, ServiceError> {
    let in_use = used_ports(base, registry);
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if !in_use.contains(&port) {
            return Ok(port);
        }
    }
    Err(ServiceError::Io("No free ports in range 9000-9999".to_string()))
}

// ── Instance ID management ──────────────────────────────────────────────────

/// Generate the next instance ID for a multi-instance service.
pub fn next_instance_id(base: &Path, base_id: &str) -> String {
    let installed = config::list_installed_services(base);
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

/// Determine the instance ID for a service install.
/// All services support multiple instances: the first install uses the base ID,
/// subsequent installs get `-2`, `-3`, etc.
fn resolve_instance_id(
    base: &Path,
    service_id: &str,
) -> Result<String, ServiceError> {
    let existing = config::load_service_state(base, service_id);
    if existing.is_ok() && existing.unwrap().installed {
        Ok(next_instance_id(base, service_id))
    } else {
        Ok(service_id.to_string())
    }
}

// ── Definition lookup ───────────────────────────────────────────────────────

/// Look up a service definition by ID: check registry first, then check if
/// the service state has a `definition_id` pointing to a parent template.
pub fn lookup_definition<'a>(
    service_id: &str,
    registry: &'a HashMap<String, ServiceDefinition>,
    base: &Path,
) -> Result<&'a ServiceDefinition, ServiceError> {
    if let Some(def) = registry.get(service_id) {
        return Ok(def);
    }
    let state = config::load_service_state(base, service_id).unwrap_or_default();
    if let Some(ref parent_id) = state.definition_id {
        if let Some(def) = registry.get(parent_id) {
            return Ok(def);
        }
    }
    Err(ServiceError::NotFound(service_id.to_string()))
}

// ── Install helpers ─────────────────────────────────────────────────────────

/// Write docker-compose.yml and .env files for a service.
/// If Tailscale is enabled, injects TSDProxy labels into the compose file.
fn write_service_files(
    base: &Path,
    svc_dir: &Path,
    instance_id: &str,
    def: &ServiceDefinition,
    merged_env: &HashMap<String, String>,
    env_overrides: &HashMap<String, String>,
    storage_env: &HashMap<String, String>,
    tailscale_auth_key: Option<&str>,
) -> Result<(), ServiceError> {
    std::fs::create_dir_all(svc_dir)
        .map_err(|e| ServiceError::Io(format!("Create service dir: {e}")))?;

    let mut compose_content = compose::generate_compose(def, merged_env);

    // Inject Tailscale sidecar if Tailscale is enabled and service hasn't opted out
    if let Ok(Some(ts_cfg)) = config::load_tailscale_config(base) {
        if ts_cfg.enabled {
            let svc_state = config::load_service_state(base, instance_id).unwrap_or_default();
            if !svc_state.tailscale_disabled {
                let mode = &def.metadata.tailscale_mode;
                if mode != "skip" {
                    let port = crate::tailscale::extract_container_port(&compose_content).unwrap_or(80);
                    let proxy_target = if mode == "network" {
                        format!("http://myground-{instance_id}:{port}")
                    } else {
                        format!("http://127.0.0.1:{port}")
                    };
                    match crate::tailscale::inject_tailscale_sidecar(
                        &compose_content, instance_id, port, mode, tailscale_auth_key,
                        svc_state.tailscale_hostname.as_deref(),
                    ) {
                        Ok(injected) => {
                            compose_content = injected;
                            let _ = crate::tailscale::write_serve_config(svc_dir, port, &proxy_target);
                        }
                        Err(e) => tracing::warn!("Sidecar inject failed for {instance_id}: {e}"),
                    }
                }
            }
        }
    }

    std::fs::write(svc_dir.join("docker-compose.yml"), &compose_content)
        .map_err(|e| ServiceError::Io(format!("Write compose file: {e}")))?;

    let mut env_with_storage = env_overrides.clone();
    for (k, v) in storage_env {
        env_with_storage.insert(k.clone(), v.clone());
    }
    let env_content = compose::generate_env_file(&def.defaults, &env_with_storage);
    std::fs::write(svc_dir.join(".env"), &env_content)
        .map_err(|e| ServiceError::Io(format!("Write .env: {e}")))?;

    Ok(())
}

/// Auto-generate a display name for multi-instance services.
/// e.g. instance "filebrowser-3" of "filebrowser" → "File Browser 3"
fn auto_display_name(service_id: &str, instance_id: &str, base_name: &str) -> Option<String> {
    if instance_id == service_id {
        return None;
    }
    let suffix = instance_id.strip_prefix(service_id)?.strip_prefix('-')?;
    Some(format!("{base_name} {suffix}"))
}

// ── Install ─────────────────────────────────────────────────────────────────

/// Install a service: setup files + pull + start (blocking).
///
/// For streaming progress, use `install_service_setup` + `compose::deploy_streaming`.
pub async fn install_service(
    base: &Path,
    registry: &HashMap<String, ServiceDefinition>,
    service_id: &str,
    storage_path: Option<&str>,
    variables: Option<&HashMap<String, String>>,
    tailscale_auth_key: Option<&str>,
) -> Result<InstallResult, ServiceError> {
    let result = install_service_setup(base, registry, service_id, storage_path, variables, None, tailscale_auth_key)?;

    let svc_dir = config::service_dir(base, &result.instance_id);
    let compose_cmd = compose::detect_command().await?;
    compose::run(&compose_cmd, &svc_dir, &["pull"]).await?;
    compose::run(&compose_cmd, &svc_dir, &["up", "-d"]).await?;

    Ok(result)
}

/// Setup-only install: write files, save state, allocate port. Does NOT pull or start.
pub fn install_service_setup(
    base: &Path,
    registry: &HashMap<String, ServiceDefinition>,
    service_id: &str,
    storage_path: Option<&str>,
    variables: Option<&HashMap<String, String>>,
    display_name: Option<&str>,
    tailscale_auth_key: Option<&str>,
) -> Result<InstallResult, ServiceError> {
    let def = registry
        .get(service_id)
        .ok_or_else(|| ServiceError::NotFound(service_id.to_string()))?;

    let instance_id = resolve_instance_id(base, service_id)?;
    let port = allocate_port(base, registry)?;

    // Build env overrides with allocated port + install variables
    let mut env_overrides = HashMap::new();
    for key in def.defaults.keys() {
        if key.ends_with("_PORT") {
            env_overrides.insert(key.clone(), port.to_string());
        }
    }
    if let Some(vars) = variables {
        for (k, v) in vars {
            env_overrides.insert(k.clone(), v.clone());
        }
    }

    // Build storage path overrides — no myground/ prefix, just volume subdirs
    let mut storage_overrides = HashMap::new();
    if let Some(sp) = storage_path {
        if def.storage.len() == 1 {
            // Single volume: use path directly
            storage_overrides.insert(def.storage[0].name.clone(), format!("{sp}/"));
        } else {
            // Multiple volumes: subdirectory per volume name
            for vol in &def.storage {
                storage_overrides.insert(vol.name.clone(), format!("{sp}/{}/", vol.name));
            }
        }
    }

    // Resolve and create storage directories
    let global_config = config::load_global_config(base).unwrap_or_default();
    let pre_state = ServiceState {
        storage_paths: storage_overrides.clone(),
        ..Default::default()
    };
    let storage_env =
        config::resolve_storage_paths(base, &instance_id, def, &global_config, &pre_state);

    for path in storage_env.values() {
        std::fs::create_dir_all(path)
            .map_err(|e| ServiceError::Io(format!("Create storage dir: {e}")))?;
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

    // For multi-instance, adjust container names in compose template
    let prefix = crate::docker::CONTAINER_PREFIX;
    let adjusted_def = if instance_id != service_id {
        ServiceDefinition {
            compose_template: def.compose_template.replace(
                &format!("{prefix}{service_id}"),
                &format!("{prefix}{instance_id}"),
            ),
            ..def.clone()
        }
    } else {
        def.clone()
    };

    // Write compose + .env files
    let svc_dir = config::service_dir(base, &instance_id);
    write_service_files(base, &svc_dir, &instance_id, &adjusted_def, &merged_env, &env_overrides, &storage_env, tailscale_auth_key)?;

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
    let state = ServiceState {
        installed: true,
        env_overrides,
        storage_paths: state_storage_paths,
        port: Some(port),
        definition_id: if instance_id != service_id {
            Some(service_id.to_string())
        } else {
            None
        },
        display_name: display_name
            .map(|s| s.to_string())
            .or_else(|| auto_display_name(service_id, &instance_id, &def.metadata.name)),
        backup: None,
        backup_password: None,
        last_backup_at: None,
        tailscale_disabled: false,
        tailscale_hostname: None,
        image_digest: None,
        update_available: false,
        last_update_check: None,
    };
    config::save_service_state(base, &instance_id, &state)?;

    Ok(InstallResult {
        instance_id,
        port,
    })
}

// ── Lifecycle ───────────────────────────────────────────────────────────────

/// Verify a service is installed, returning its state.
fn require_installed(base: &Path, service_id: &str) -> Result<ServiceState, ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }
    Ok(state)
}

/// Start a service.
pub async fn start_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    require_installed(base, service_id)?;
    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = compose::detect_command().await?;
    compose::run(&compose_cmd, &svc_dir, &["up", "-d"]).await?;
    Ok(())
}

/// Stop a service.
pub async fn stop_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    require_installed(base, service_id)?;
    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = compose::detect_command().await?;
    compose::run(&compose_cmd, &svc_dir, &["down"]).await?;
    Ok(())
}

/// Remove a service: compose down, delete service metadata directory.
/// Does NOT delete user data in storage paths — user data is sacred.
pub async fn remove_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = require_installed(base, service_id)?;
    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = compose::detect_command().await?;

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

    // Remove service metadata files; best-effort remove the whole directory
    if std::fs::remove_dir_all(&svc_dir).is_err() {
        let mut cleared = config::ServiceState::default();
        cleared.installed = false;
        let _ = config::save_service_state(base, service_id, &cleared);
        let _ = std::fs::remove_file(svc_dir.join("docker-compose.yml"));
        let _ = std::fs::remove_file(svc_dir.join(".env"));
    }

    Ok(())
}

/// Nuke everything: stop all services, remove all containers, delete all data.
pub async fn nuke_all(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    // Clean up Tailscale exit node
    let exit_actions = crate::tailscale::cleanup_exit_node(base).await;
    actions.extend(exit_actions);

    // Clean up old TSDProxy if it exists (migration leftovers)
    let ts_actions = crate::tailscale::cleanup_tsdproxy(base).await;
    actions.extend(ts_actions);

    let installed = config::list_installed_services(base);
    if let Ok(compose_cmd) = compose::detect_command().await {
        for id in &installed {
            let svc_dir = config::service_dir(base, id);
            if svc_dir.join("docker-compose.yml").exists() {
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
    use crate::testutil::dummy_service_def;

    #[test]
    fn used_ports_returns_empty_when_no_services() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        let registry = HashMap::new();
        assert!(used_ports(base, &registry).is_empty());
    }

    #[test]
    fn used_ports_includes_registry_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_service_def(
            "whoami",
            "",
            HashMap::from([("WHOAMI_PORT".to_string(), "8081".to_string())]),
            Vec::new(),
        );
        registry.insert("whoami".to_string(), def);

        let ports = used_ports(base, &registry);
        assert!(ports.contains(&8081));
    }

    #[test]
    fn allocate_port_returns_first_free_in_range() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();
        let registry = HashMap::new();

        let port = allocate_port(base, &registry).unwrap();
        assert_eq!(port, PORT_RANGE_START);
    }

    #[test]
    fn allocate_port_skips_used_ports() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_service_def(
            "test",
            "",
            HashMap::from([("TEST_PORT".to_string(), "9000".to_string())]),
            Vec::new(),
        );
        registry.insert("test".to_string(), def);

        let port = allocate_port(base, &registry).unwrap();
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

        let state = config::ServiceState {
            installed: true,
            ..Default::default()
        };
        config::save_service_state(base, "filebrowser", &state).unwrap();

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

        let state = config::ServiceState { installed: true, ..Default::default() };
        config::save_service_state(base, "whoami", &state).unwrap();

        let result = resolve_instance_id(base, "whoami").unwrap();
        assert_eq!(result, "whoami-2");
    }

    #[test]
    fn lookup_definition_direct_hit() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        config::ensure_data_dir(base).unwrap();

        let mut registry = HashMap::new();
        let def = dummy_service_def("whoami", "", HashMap::new(), Vec::new());
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
        let def = dummy_service_def("filebrowser", "", HashMap::new(), Vec::new());
        registry.insert("filebrowser".to_string(), def);

        let state = config::ServiceState {
            installed: true,
            definition_id: Some("filebrowser".to_string()),
            ..Default::default()
        };
        config::save_service_state(base, "filebrowser-2", &state).unwrap();

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
