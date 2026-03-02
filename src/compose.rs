use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use crate::config;
use crate::error::ServiceError;
use crate::registry::ServiceDefinition;

/// Detect whether `docker compose` (v2) or `docker-compose` (v1) is available.
pub async fn detect_command() -> Result<Vec<String>, ServiceError> {
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
    let merged = merge_env(defaults, overrides);
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

/// Validate that a composed YAML string is structurally valid.
pub fn validate_compose(yaml: &str) -> Result<(), ServiceError> {
    let _: serde_yaml::Value = serde_yaml::from_str(yaml)
        .map_err(|e| ServiceError::Io(format!("Invalid compose YAML after substitution: {e}")))?;
    Ok(())
}

/// Restrict file permissions to owner-only (0o600).
pub fn restrict_file_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

/// Validate that an env value does not contain newlines or other control characters
/// that could break .env files or YAML templates.
pub fn validate_env_value(value: &str) -> Result<(), ServiceError> {
    if value.contains('\n') || value.contains('\r') {
        return Err(ServiceError::Io(
            "Env value must not contain newline characters".to_string(),
        ));
    }
    if value.chars().any(|c| c.is_control() && c != '\t') {
        return Err(ServiceError::Io(
            "Env value must not contain control characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate that an env key contains only `[A-Z0-9_]` characters.
pub fn validate_env_key(key: &str) -> Result<(), ServiceError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(ServiceError::Io(format!(
            "Invalid env key '{key}': must contain only A-Z, 0-9, _"
        )));
    }
    Ok(())
}

/// Run a docker compose command in a service directory.
pub async fn run(
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

/// Run a docker compose command, streaming stdout+stderr lines via a channel.
pub async fn run_streaming(
    compose_cmd: &[String],
    work_dir: &Path,
    args: &[&str],
    tx: &tokio::sync::mpsc::Sender<String>,
) -> Result<(), ServiceError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let (program, base_args) = compose_cmd.split_first().expect("compose_cmd is non-empty");

    let mut child = tokio::process::Command::new(program)
        .args(base_args)
        .args(args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ServiceError::Compose(format!("Failed to run compose: {e}")))?;

    let stderr = child.stderr.take().unwrap();
    let stdout = child.stdout.take().unwrap();

    let tx2 = tx.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx2.send(line).await.is_err() {
                break;
            }
        }
    });

    let tx3 = tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx3.send(line).await.is_err() {
                break;
            }
        }
    });

    let status = child
        .wait()
        .await
        .map_err(|e| ServiceError::Compose(format!("compose wait: {e}")))?;

    let _ = stderr_task.await;
    let _ = stdout_task.await;

    if !status.success() {
        return Err(ServiceError::Compose("Compose command failed".to_string()));
    }

    Ok(())
}

/// Deploy (pull + start) a service, streaming progress lines via a channel.
/// After a successful deploy, records the primary image digest in ServiceState.
pub async fn deploy_streaming(
    base: &Path,
    service_id: &str,
    tx: tokio::sync::mpsc::Sender<String>,
) -> Result<(), ServiceError> {
    let svc_dir = config::service_dir(base, service_id);
    let compose_cmd = detect_command().await?;

    let _ = tx.send("Pulling images...".to_string()).await;
    run_streaming(&compose_cmd, &svc_dir, &["pull"], &tx).await?;

    let _ = tx.send("Starting containers...".to_string()).await;
    run_streaming(&compose_cmd, &svc_dir, &["up", "-d"], &tx).await?;

    // Record the image digest for update tracking
    let compose_path = svc_dir.join("docker-compose.yml");
    if compose_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&compose_path) {
            if let Some(image_ref) = crate::updates::extract_primary_image(&content) {
                if let Ok(digest) = crate::updates::get_image_digest(&image_ref).await {
                    if let Ok(mut svc_state) = config::load_service_state(base, service_id) {
                        svc_state.image_digest = Some(digest);
                        svc_state.update_available = false;
                        let _ = config::save_service_state(base, service_id, &svc_state);
                    }
                }
            }
        }
    }

    Ok(())
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

    #[test]
    fn validate_env_value_accepts_normal() {
        assert!(validate_env_value("hello world").is_ok());
        assert!(validate_env_value("p@ssw0rd!#$%^&*").is_ok());
        assert!(validate_env_value("").is_ok());
        assert!(validate_env_value("with\ttab").is_ok());
    }

    #[test]
    fn validate_env_value_rejects_newlines() {
        assert!(validate_env_value("line1\nline2").is_err());
        assert!(validate_env_value("line1\r\nline2").is_err());
        assert!(validate_env_value("line1\rline2").is_err());
    }

    #[test]
    fn validate_env_value_rejects_control_chars() {
        assert!(validate_env_value("null\x00byte").is_err());
        assert!(validate_env_value("bell\x07char").is_err());
    }

    #[test]
    fn validate_compose_accepts_valid_yaml() {
        assert!(validate_compose("services:\n  app:\n    image: nginx\n").is_ok());
    }

    #[test]
    fn validate_compose_rejects_invalid_yaml() {
        assert!(validate_compose("{{invalid: yaml: [").is_err());
    }

    #[test]
    fn validate_env_key_accepts_valid() {
        assert!(validate_env_key("PORT").is_ok());
        assert!(validate_env_key("MY_VAR_123").is_ok());
        assert!(validate_env_key("A").is_ok());
    }

    #[test]
    fn validate_env_key_rejects_empty() {
        assert!(validate_env_key("").is_err());
    }

    #[test]
    fn validate_env_key_rejects_lowercase() {
        assert!(validate_env_key("port").is_err());
        assert!(validate_env_key("myVar").is_err());
    }

    #[test]
    fn validate_env_key_rejects_special_chars() {
        assert!(validate_env_key("MY-VAR").is_err());
        assert!(validate_env_key("MY.VAR").is_err());
        assert!(validate_env_key("MY VAR").is_err());
    }

    #[test]
    fn restrict_file_permissions_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "test").unwrap();
        restrict_file_permissions(&path);
        // File should still be readable by owner
        assert!(std::fs::read_to_string(&path).is_ok());
    }

    #[test]
    fn generate_env_file_sorted_output() {
        let defaults = HashMap::from([
            ("ZEBRA".to_string(), "1".to_string()),
            ("ALPHA".to_string(), "2".to_string()),
        ]);
        let result = generate_env_file(&defaults, &HashMap::new());
        let lines: Vec<&str> = result.trim().split('\n').collect();
        assert_eq!(lines[0], "ALPHA=2");
        assert_eq!(lines[1], "ZEBRA=1");
    }
}
