use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use crate::config::{self, GlobalConfig, ServiceState};
use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

/// Detect whether `docker compose` (v2) or `docker-compose` (v1) is available.
pub async fn detect_compose_command() -> Result<Vec<String>, ServiceError> {
    // Try v2 first
    let v2 = tokio::process::Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if let Ok(status) = v2 {
        if status.success() {
            return Ok(vec!["docker".to_string(), "compose".to_string()]);
        }
    }

    // Try v1
    let v1 = tokio::process::Command::new("docker-compose")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    if let Ok(status) = v1 {
        if status.success() {
            return Ok(vec!["docker-compose".to_string()]);
        }
    }

    Err(ServiceError::Compose(
        "Neither 'docker compose' nor 'docker-compose' found".to_string(),
    ))
}

/// Generate docker-compose.yml content from a service definition and env vars.
pub fn generate_compose(def: &ServiceDefinition, env: &HashMap<String, String>) -> String {
    let mut result = def.compose_template.clone();
    for (key, value) in env {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    result
}

/// Generate .env file content from defaults merged with overrides.
pub fn generate_env_file(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> String {
    let mut merged = defaults.clone();
    for (k, v) in overrides {
        merged.insert(k.clone(), v.clone());
    }
    let mut lines: Vec<String> = merged.iter().map(|(k, v)| format!("{k}={v}")).collect();
    lines.sort();
    lines.join("\n") + "\n"
}

/// Merge defaults with user overrides.
pub fn merge_env(
    defaults: &HashMap<String, String>,
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut merged = defaults.clone();
    for (k, v) in overrides {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Install a service: write compose file, .env, state, and pull images.
pub async fn install_service(
    base: &Path,
    def: &ServiceDefinition,
    env_overrides: &HashMap<String, String>,
    global_config: &GlobalConfig,
) -> Result<(), ServiceError> {
    let id = &def.metadata.id;

    // Check if already installed
    let existing = config::load_service_state(base, id)?;
    if existing.installed {
        return Err(ServiceError::AlreadyInstalled(id.clone()));
    }

    // Resolve storage paths
    let storage_state = ServiceState::default();
    let storage_paths =
        config::resolve_storage_paths(base, id, def, global_config, &storage_state);

    // Create bind mount directories
    for path in storage_paths.values() {
        std::fs::create_dir_all(path)
            .map_err(|e| ServiceError::Io(format!("Create storage dir {path}: {e}")))?;
    }

    // Merge env: defaults + overrides + STORAGE_ vars
    let mut merged_env = merge_env(&def.defaults, env_overrides);
    for (k, v) in &storage_paths {
        merged_env.insert(k.clone(), v.clone());
    }

    // Write compose file
    let svc_dir = config::service_dir(base, id);
    std::fs::create_dir_all(&svc_dir)
        .map_err(|e| ServiceError::Io(format!("Create service dir: {e}")))?;

    let compose_content = generate_compose(def, &merged_env);
    std::fs::write(svc_dir.join("docker-compose.yml"), &compose_content)
        .map_err(|e| ServiceError::Io(format!("Write compose file: {e}")))?;

    // Write .env (include storage vars so compose interpolation works)
    let mut env_with_storage = env_overrides.clone();
    for (k, v) in &storage_paths {
        env_with_storage.insert(k.clone(), v.clone());
    }
    let env_content = generate_env_file(&def.defaults, &env_with_storage);
    std::fs::write(svc_dir.join(".env"), &env_content)
        .map_err(|e| ServiceError::Io(format!("Write .env: {e}")))?;

    // Build storage_paths for state (volume name → path, not STORAGE_ key → path)
    let mut state_storage_paths = HashMap::new();
    for vol in &def.storage {
        if let Some(path) = storage_paths.get(&format!("STORAGE_{}", vol.name)) {
            state_storage_paths.insert(vol.name.clone(), path.clone());
        }
    }

    // Save state
    let state = ServiceState {
        installed: true,
        env_overrides: env_overrides.clone(),
        storage_paths: state_storage_paths,
    };
    config::save_service_state(base, id, &state)?;

    // Pull images
    let compose_cmd = detect_compose_command().await?;
    run_compose(&compose_cmd, &svc_dir, &["pull"]).await?;

    Ok(())
}

/// Start a service.
pub async fn start_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_compose_command().await?;
    run_compose(&compose_cmd, &svc_dir, &["up", "-d"]).await?;

    Ok(())
}

/// Stop a service.
pub async fn stop_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_compose_command().await?;
    run_compose(&compose_cmd, &svc_dir, &["down"]).await?;

    Ok(())
}

/// Remove a service: compose down, delete service metadata directory.
/// Does NOT delete user data in storage paths — user data is sacred.
pub async fn remove_service(base: &Path, service_id: &str) -> Result<(), ServiceError> {
    let state = config::load_service_state(base, service_id)?;
    if !state.installed {
        return Err(ServiceError::NotInstalled(service_id.to_string()));
    }

    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_compose_command().await?;

    // Try to bring down containers; ignore errors (may already be stopped)
    let _ = run_compose(&compose_cmd, &svc_dir, &["down", "--remove-orphans"]).await;

    // Warn about external storage paths that will be left intact
    for (name, path) in &state.storage_paths {
        if !path.starts_with(&svc_dir.to_string_lossy().to_string()) {
            tracing::info!("Keeping storage data for '{name}' at: {path}");
        }
    }

    // Remove the service metadata directory
    std::fs::remove_dir_all(&svc_dir)
        .map_err(|e| ServiceError::Io(format!("Remove service dir: {e}")))?;

    Ok(())
}

/// Run a docker compose command in a service directory.
async fn run_compose(
    compose_cmd: &[String],
    work_dir: &Path,
    args: &[&str],
) -> Result<String, ServiceError> {
    let (program, base_args) = compose_cmd.split_first().expect("compose_cmd is non-empty");

    let output = tokio::process::Command::new(program)
        .args(base_args)
        .args(args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ServiceError::Compose(format!("Failed to run compose: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ServiceError::Compose(format!(
            "Compose command failed: {stderr}"
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{dummy_service_def, dummy_storage_volumes};

    #[test]
    fn generate_compose_substitutes_vars() {
        let def = dummy_service_def(
            "test",
            "ports:\n  - \"${PORT}:80\"",
            HashMap::from([("PORT".to_string(), "8080".to_string())]),
            Vec::new(),
        );

        let env = HashMap::from([("PORT".to_string(), "9090".to_string())]);
        let result = generate_compose(&def, &env);
        assert_eq!(result, "ports:\n  - \"9090:80\"");
    }

    #[test]
    fn generate_env_file_merges_correctly() {
        let defaults = HashMap::from([
            ("PORT".to_string(), "8080".to_string()),
            ("HOST".to_string(), "localhost".to_string()),
        ]);
        let overrides = HashMap::from([("PORT".to_string(), "9090".to_string())]);

        let result = generate_env_file(&defaults, &overrides);
        assert!(result.contains("PORT=9090"));
        assert!(result.contains("HOST=localhost"));
        assert!(!result.contains("PORT=8080"));
    }

    #[test]
    fn merge_env_applies_overrides() {
        let defaults = HashMap::from([("A".to_string(), "1".to_string())]);
        let overrides = HashMap::from([
            ("A".to_string(), "2".to_string()),
            ("B".to_string(), "3".to_string()),
        ]);
        let merged = merge_env(&defaults, &overrides);
        assert_eq!(merged.get("A").unwrap(), "2");
        assert_eq!(merged.get("B").unwrap(), "3");
    }

    #[test]
    fn generate_compose_with_storage_vars() {
        let def = dummy_service_def(
            "fb",
            "volumes:\n  - ${STORAGE_data}:/srv\n  - ${STORAGE_config}:/config",
            HashMap::new(),
            dummy_storage_volumes(),
        );

        let env = HashMap::from([
            ("STORAGE_data".to_string(), "/mnt/data/fb/data".to_string()),
            (
                "STORAGE_config".to_string(),
                "/mnt/data/fb/config".to_string(),
            ),
        ]);

        let result = generate_compose(&def, &env);
        assert!(result.contains("/mnt/data/fb/data:/srv"));
        assert!(result.contains("/mnt/data/fb/config:/config"));
        assert!(!result.contains("${STORAGE_"));
    }
}
