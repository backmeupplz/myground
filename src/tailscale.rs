use std::path::Path;
use std::process::Stdio;

use crate::config;
use crate::error::ServiceError;

const TSDPROXY_CONTAINER: &str = "myground-tsdproxy";

/// Ensure TSDProxy is running. Writes compose file and starts the container.
pub async fn ensure_tsdproxy(base: &Path) -> Result<(), ServiceError> {
    let ts_cfg = config::load_tailscale_config(base)?
        .ok_or_else(|| ServiceError::Io("Tailscale not configured".to_string()))?;

    let auth_key = ts_cfg
        .auth_key
        .as_deref()
        .ok_or_else(|| ServiceError::Io("No Tailscale auth key set".to_string()))?;

    let tsdproxy_dir = base.join("tsdproxy");
    std::fs::create_dir_all(&tsdproxy_dir)
        .map_err(|e| ServiceError::Io(format!("Create tsdproxy dir: {e}")))?;

    let data_dir = tsdproxy_dir.join("data");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| ServiceError::Io(format!("Create tsdproxy data dir: {e}")))?;

    let compose_content = format!(
        r#"services:
  tsdproxy:
    image: almeidapaulopt/tsdproxy:latest
    container_name: {TSDPROXY_CONTAINER}
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - {data}:/data
    environment:
      TSDPROXY_AUTHKEY: "{auth_key}"
      TSDPROXY_HOSTNAME: "myground-tsdproxy"
"#,
        data = data_dir.display(),
    );

    std::fs::write(tsdproxy_dir.join("docker-compose.yml"), &compose_content)
        .map_err(|e| ServiceError::Io(format!("Write tsdproxy compose: {e}")))?;

    let compose_cmd = crate::compose::detect_command().await?;
    crate::compose::run(&compose_cmd, &tsdproxy_dir, &["up", "-d"]).await?;

    Ok(())
}

/// Stop and remove the TSDProxy container.
pub async fn stop_tsdproxy(base: &Path) -> Result<(), ServiceError> {
    let tsdproxy_dir = base.join("tsdproxy");
    if !tsdproxy_dir.join("docker-compose.yml").exists() {
        return Ok(());
    }

    let compose_cmd = crate::compose::detect_command().await?;
    let _ = crate::compose::run(
        &compose_cmd,
        &tsdproxy_dir,
        &["down", "--remove-orphans", "--volumes"],
    )
    .await;

    Ok(())
}

/// Check if the TSDProxy container is running.
pub async fn is_tsdproxy_running() -> bool {
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", TSDPROXY_CONTAINER])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

/// Try to detect the tailnet name from TSDProxy logs or container state.
pub async fn detect_tailnet() -> Option<String> {
    // Try reading from TSDProxy's tailscale status
    let output = tokio::process::Command::new("docker")
        .args([
            "exec",
            TSDPROXY_CONTAINER,
            "cat",
            "/data/state/tsdproxy.state.conf",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let content = String::from_utf8_lossy(&output.stdout);
    // Look for a tailnet domain like "tail1234b.ts.net" in the output
    for line in content.lines() {
        if let Some(domain) = extract_tailnet_domain(line) {
            return Some(domain);
        }
    }

    // Fallback: check docker logs for the domain
    let logs = tokio::process::Command::new("docker")
        .args(["logs", "--tail", "50", TSDPROXY_CONTAINER])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&logs.stdout),
        String::from_utf8_lossy(&logs.stderr)
    );
    for line in combined.lines() {
        if let Some(domain) = extract_tailnet_domain(line) {
            return Some(domain);
        }
    }

    None
}

/// Extract a tailnet domain (*.ts.net) from a string.
fn extract_tailnet_domain(s: &str) -> Option<String> {
    for word in s.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
        if word.ends_with(".ts.net") && word.contains('.') {
            // Extract just the tailnet part (e.g. "tail1234b.ts.net")
            let parts: Vec<&str> = word.split('.').collect();
            if parts.len() >= 3 {
                // Could be "hostname.tailnet.ts.net" — return "tailnet.ts.net"
                let tailnet = parts[parts.len() - 3..].join(".");
                return Some(tailnet);
            }
        }
    }
    None
}

/// Inject TSDProxy labels into a compose YAML string for a specific service.
pub fn inject_tsdproxy_labels(
    compose_yaml: &str,
    service_name: &str,
    container_port: u16,
) -> Result<String, ServiceError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| ServiceError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| ServiceError::Io("No 'services' key in compose YAML".to_string()))?;

    // Get the first service entry
    if let Some((_key, svc)) = services.iter_mut().next() {
        let svc_map = svc
            .as_mapping_mut()
            .ok_or_else(|| ServiceError::Io("Service is not a mapping".to_string()))?;

        let labels_key = serde_yaml::Value::String("labels".to_string());

        let labels = svc_map
            .entry(labels_key)
            .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));

        if let Some(labels_map) = labels.as_mapping_mut() {
            labels_map.insert(
                serde_yaml::Value::String("tsdproxy.enable".to_string()),
                serde_yaml::Value::String("true".to_string()),
            );
            labels_map.insert(
                serde_yaml::Value::String("tsdproxy.name".to_string()),
                serde_yaml::Value::String(service_name.to_string()),
            );
            labels_map.insert(
                serde_yaml::Value::String("tsdproxy.container_port".to_string()),
                serde_yaml::Value::String(container_port.to_string()),
            );
        }
    }

    serde_yaml::to_string(&doc).map_err(|e| ServiceError::Io(format!("Serialize compose YAML: {e}")))
}

/// Extract the first container port from a compose YAML (from the port mapping).
pub fn extract_container_port(compose_yaml: &str) -> Option<u16> {
    let doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml).ok()?;
    let services = doc.get("services")?.as_mapping()?;
    let (_key, svc) = services.iter().next()?;
    let ports = svc.get("ports")?.as_sequence()?;
    let first_port = ports.first()?.as_str()?;

    // Port format: "HOST:CONTAINER" or "HOST:CONTAINER/tcp"
    let container_part = first_port.split(':').last()?;
    let port_str = container_part.split('/').next()?;
    port_str.parse().ok()
}

/// Remove TSDProxy labels from a compose YAML string.
pub fn remove_tsdproxy_labels(compose_yaml: &str) -> Result<String, ServiceError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| ServiceError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| ServiceError::Io("No 'services' key in compose YAML".to_string()))?;

    for (_key, svc) in services.iter_mut() {
        if let Some(svc_map) = svc.as_mapping_mut() {
            let labels_key = serde_yaml::Value::String("labels".to_string());
            if let Some(labels) = svc_map.get_mut(&labels_key) {
                if let Some(labels_map) = labels.as_mapping_mut() {
                    labels_map.remove(&serde_yaml::Value::String("tsdproxy.enable".to_string()));
                    labels_map.remove(&serde_yaml::Value::String("tsdproxy.name".to_string()));
                    labels_map.remove(&serde_yaml::Value::String(
                        "tsdproxy.container_port".to_string(),
                    ));
                    // If labels map is now empty, remove the labels key entirely
                    if labels_map.is_empty() {
                        svc_map.remove(&labels_key);
                    }
                }
            }
        }
    }

    serde_yaml::to_string(&doc).map_err(|e| ServiceError::Io(format!("Serialize compose YAML: {e}")))
}

/// Clean up TSDProxy: stop container and remove data directory.
pub async fn cleanup_tsdproxy(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    // Stop TSDProxy via compose
    let _ = stop_tsdproxy(base).await;

    // Force-remove container
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", TSDPROXY_CONTAINER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;
    actions.push(format!("Removed TSDProxy container: {TSDPROXY_CONTAINER}"));

    // Remove tsdproxy directory
    let tsdproxy_dir = base.join("tsdproxy");
    if tsdproxy_dir.exists() {
        if std::fs::remove_dir_all(&tsdproxy_dir).is_ok() {
            actions.push(format!("Removed TSDProxy data: {}", tsdproxy_dir.display()));
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_labels_into_simple_compose() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    container_name: myground-whoami
    ports:
      - "9000:80"
"#;
        let result = inject_tsdproxy_labels(yaml, "whoami", 80).unwrap();
        assert!(result.contains("tsdproxy.enable"));
        assert!(result.contains("tsdproxy.name"));
        assert!(result.contains("tsdproxy.container_port"));
    }

    #[test]
    fn remove_labels_from_compose() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    labels:
      tsdproxy.enable: 'true'
      tsdproxy.name: whoami
      tsdproxy.container_port: '80'
    ports:
      - "9000:80"
"#;
        let result = remove_tsdproxy_labels(yaml).unwrap();
        assert!(!result.contains("tsdproxy.enable"));
        assert!(!result.contains("tsdproxy.name"));
    }

    #[test]
    fn extract_port_from_compose() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    ports:
      - "9000:80"
"#;
        assert_eq!(extract_container_port(yaml), Some(80));
    }

    #[test]
    fn extract_port_with_protocol() {
        let yaml = r#"services:
  test:
    image: test
    ports:
      - "9000:8080/tcp"
"#;
        assert_eq!(extract_container_port(yaml), Some(8080));
    }

    #[test]
    fn extract_tailnet_domain_from_string() {
        assert_eq!(
            extract_tailnet_domain("Connected to myhost.tail1234b.ts.net"),
            Some("tail1234b.ts.net".to_string())
        );
        assert_eq!(extract_tailnet_domain("no domain here"), None);
    }
}
