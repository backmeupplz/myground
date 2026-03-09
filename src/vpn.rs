use std::path::Path;

use crate::config::VpnConfig;
use crate::error::AppError;

/// Inject a gluetun VPN sidecar into a compose YAML.
///
/// - Adds a `gluetun` service with NET_ADMIN cap and /dev/net/tun
/// - Moves ports from the main (first) service to gluetun
/// - Sets `network_mode: service:gluetun` + `depends_on: [gluetun]` on main service
pub fn inject_vpn_sidecar(
    compose_yaml: &str,
    instance_id: &str,
    _vpn_config: &VpnConfig,
) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("No 'services' key in compose YAML".to_string()))?;

    let first_key = services
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| AppError::Io("No entries in compose YAML".to_string()))?;

    // Reject apps with network_mode: host
    {
        let main_svc = services
            .get(&first_key)
            .and_then(|s| s.as_mapping())
            .ok_or_else(|| AppError::Io("Main app entry is not a mapping".to_string()))?;
        let nm_key = serde_yaml::Value::String("network_mode".to_string());
        if let Some(nm) = main_svc.get(&nm_key).and_then(|v| v.as_str()) {
            if nm == "host" {
                return Err(AppError::Io(
                    "VPN sidecar is incompatible with network_mode: host".to_string(),
                ));
            }
        }
    }

    let main_svc = services
        .get_mut(&first_key)
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("Main app entry is not a mapping".to_string()))?;

    // Move ports from main app to gluetun
    let ports_key = serde_yaml::Value::String("ports".to_string());
    let ports_value = main_svc.remove(&ports_key);

    // Set network_mode on main app
    main_svc.insert(
        serde_yaml::Value::String("network_mode".to_string()),
        serde_yaml::Value::String("service:gluetun".to_string()),
    );

    // Add depends_on for gluetun (merge with existing)
    let depends_key = serde_yaml::Value::String("depends_on".to_string());
    let existing_deps = main_svc.remove(&depends_key);
    let mut deps_seq = match existing_deps {
        Some(serde_yaml::Value::Sequence(seq)) => seq,
        _ => Vec::new(),
    };
    if !deps_seq.iter().any(|v| v.as_str() == Some("gluetun")) {
        deps_seq.push(serde_yaml::Value::String("gluetun".to_string()));
    }
    main_svc.insert(depends_key, serde_yaml::Value::Sequence(deps_seq));

    // Build gluetun service
    let container_name = format!("myground-{instance_id}-vpn");
    let mut gluetun = serde_yaml::Mapping::new();
    gluetun.insert(
        serde_yaml::Value::String("image".to_string()),
        serde_yaml::Value::String("qmcgaw/gluetun:latest".to_string()),
    );
    gluetun.insert(
        serde_yaml::Value::String("container_name".to_string()),
        serde_yaml::Value::String(container_name),
    );
    gluetun.insert(
        serde_yaml::Value::String("restart".to_string()),
        serde_yaml::Value::String("unless-stopped".to_string()),
    );
    gluetun.insert(
        serde_yaml::Value::String("cap_add".to_string()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
            "NET_ADMIN".to_string(),
        )]),
    );
    gluetun.insert(
        serde_yaml::Value::String("devices".to_string()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
            "/dev/net/tun:/dev/net/tun".to_string(),
        )]),
    );
    gluetun.insert(
        serde_yaml::Value::String("env_file".to_string()),
        serde_yaml::Value::String("./vpn-sidecar.env".to_string()),
    );

    // Move ports to gluetun
    if let Some(ports) = ports_value {
        gluetun.insert(serde_yaml::Value::String("ports".to_string()), ports);
    }

    // Add gluetun to services
    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .unwrap();
    services.insert(
        serde_yaml::Value::String("gluetun".to_string()),
        serde_yaml::Value::Mapping(gluetun),
    );

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
}

/// Remove the gluetun VPN sidecar from a compose YAML.
///
/// Restores ports to the main service, removes `network_mode` and `depends_on` for gluetun.
pub fn remove_vpn_sidecar(compose_yaml: &str) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("No 'services' key in compose YAML".to_string()))?;

    let gluetun_key = serde_yaml::Value::String("gluetun".to_string());
    if !services.contains_key(&gluetun_key) {
        return serde_yaml::to_string(&doc)
            .map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")));
    }

    // Get gluetun's ports (to restore to main app)
    let gluetun_ports = services
        .get(&gluetun_key)
        .and_then(|s| s.get("ports"))
        .cloned();

    // Remove gluetun service
    services.remove(&gluetun_key);

    // Fix up the main (first) service
    let first_key = services.keys().next().cloned();
    if let Some(key) = first_key {
        if let Some(main_svc) = services.get_mut(&key).and_then(|s| s.as_mapping_mut()) {
            let nm_key = serde_yaml::Value::String("network_mode".to_string());
            let is_vpn_mode = main_svc
                .get(&nm_key)
                .and_then(|v| v.as_str())
                .map(|s| s == "service:gluetun")
                .unwrap_or(false);

            if is_vpn_mode {
                main_svc.remove(&nm_key);

                // Remove gluetun from depends_on
                let deps_key = serde_yaml::Value::String("depends_on".to_string());
                if let Some(deps) = main_svc.get_mut(&deps_key) {
                    if let Some(seq) = deps.as_sequence_mut() {
                        seq.retain(|v| v.as_str() != Some("gluetun"));
                        if seq.is_empty() {
                            main_svc.remove(&deps_key);
                        }
                    }
                }

                // Restore ports
                if let Some(ports) = gluetun_ports {
                    main_svc.insert(
                        serde_yaml::Value::String("ports".to_string()),
                        ports,
                    );
                }
            }
        }
    }

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
}

/// Write VPN environment variables to `vpn-sidecar.env`.
pub fn write_vpn_env(
    svc_dir: &Path,
    vpn_config: &VpnConfig,
    vpn_port_forward_command: Option<&str>,
) -> Result<(), AppError> {
    let mut lines = Vec::new();

    if let Some(ref provider) = vpn_config.provider {
        crate::compose::validate_env_value(provider)?;
        lines.push(format!("VPN_SERVICE_PROVIDER={provider}"));
    }
    if let Some(ref vpn_type) = vpn_config.vpn_type {
        crate::compose::validate_env_value(vpn_type)?;
        lines.push(format!("VPN_TYPE={vpn_type}"));
    }
    if let Some(ref countries) = vpn_config.server_countries {
        crate::compose::validate_env_value(countries)?;
        lines.push(format!("SERVER_COUNTRIES={countries}"));
    }
    if vpn_config.port_forwarding {
        lines.push("VPN_PORT_FORWARDING=on".to_string());
        if let Some(cmd) = vpn_port_forward_command {
            lines.push(format!("VPN_PORT_FORWARDING_UP_COMMAND={cmd}"));
        }
    }
    // Write additional env vars (credentials, etc.) — validated to prevent injection.
    for (k, v) in &vpn_config.env_vars {
        crate::compose::validate_env_key(k)?;
        crate::compose::validate_env_value(v)?;
        lines.push(format!("{k}={v}"));
    }

    let content = lines.join("\n") + "\n";
    let env_path = svc_dir.join("vpn-sidecar.env");
    std::fs::write(&env_path, content)
        .map_err(|e| AppError::Io(format!("Write vpn-sidecar.env: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

const VPN_TEST_CONTAINER: &str = "myground-vpn-test";

/// Test a VPN connection by spinning up a temporary gluetun container.
/// Streams log lines through `tx`. Sends `__DONE__` on success or `__FAIL__` on failure.
pub async fn test_vpn_connection_streaming(
    config: &VpnConfig,
    tx: tokio::sync::mpsc::Sender<String>,
) -> Result<(), AppError> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};

    // Clean up any leftover test container first
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", VPN_TEST_CONTAINER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    let _ = tx.send("Preparing VPN test...".to_string()).await;

    // Write temp env file
    let tmp_dir = std::env::temp_dir().join(format!("myground-vpn-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| AppError::Io(format!("Create temp dir: {e}")))?;
    write_vpn_env(&tmp_dir, config, None)?;
    let env_path = tmp_dir.join("vpn-sidecar.env");

    let _ = tx.send("Starting gluetun container...".to_string()).await;

    // Start gluetun container
    let args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        VPN_TEST_CONTAINER.to_string(),
        "--cap-add=NET_ADMIN".to_string(),
        "--device=/dev/net/tun:/dev/net/tun".to_string(),
        format!("--env-file={}", env_path.display()),
        "qmcgaw/gluetun:latest".to_string(),
    ];

    let start = tokio::process::Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| AppError::Io(format!("docker run: {e}")))?;

    if !start.status.success() {
        let stderr = String::from_utf8_lossy(&start.stderr);
        let _ = tx.send(format!("Failed to start container: {stderr}")).await;
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(AppError::Io(format!("Failed to start VPN test container: {stderr}")));
    }

    let _ = tx.send("Container started, streaming logs...".to_string()).await;

    // Stream logs in real time using `docker logs -f`
    let mut log_child = tokio::process::Command::new("docker")
        .args(["logs", "-f", VPN_TEST_CONTAINER])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Io(format!("docker logs: {e}")))?;

    let connected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);

    // Use a notify to signal when VPN is connected
    let notify = std::sync::Arc::new(tokio::sync::Notify::new());

    let stdout = log_child.stdout.take();
    let stderr = log_child.stderr.take();

    // Forward lines from both stdout and stderr, detect "Public IP address is"
    // which means the VPN tunnel is up and working.
    let (c1, n1, tx1) = (connected.clone(), notify.clone(), tx.clone());
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.contains("Public IP address is") {
                    c1.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = tx1.send(line).await;
                    n1.notify_one();
                    break;
                }
                if tx1.send(line).await.is_err() {
                    break;
                }
            }
        }
    });

    let (c2, n2, tx2) = (connected.clone(), notify.clone(), tx.clone());
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.contains("Public IP address is") {
                    c2.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = tx2.send(line).await;
                    n2.notify_one();
                    break;
                }
                if tx2.send(line).await.is_err() {
                    break;
                }
            }
        }
    });

    // Wait for either success detection or timeout
    tokio::select! {
        _ = notify.notified() => {}
        _ = tokio::time::sleep_until(deadline) => {
            let _ = tx.send("Timeout: VPN did not connect within 60 seconds".to_string()).await;
        }
    }

    // Kill the log stream
    let _ = log_child.kill().await;
    stdout_task.abort();
    stderr_task.abort();

    // Clean up container
    let _ = tx.send("Cleaning up test container...".to_string()).await;
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", VPN_TEST_CONTAINER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if connected.load(std::sync::atomic::Ordering::Relaxed) {
        let _ = tx.send("VPN connection successful!".to_string()).await;
        Ok(())
    } else {
        let _ = tx.send("VPN connection failed.".to_string()).await;
        Err(AppError::Io("VPN connection failed".to_string()))
    }
}

/// Check if VPN is enabled for an app's state.
pub fn is_vpn_enabled(state: &crate::config::InstalledAppState) -> bool {
    state.vpn.as_ref().map(|v| v.enabled).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const BASIC_COMPOSE: &str = r#"services:
  whoami:
    image: traefik/whoami
    container_name: myground-whoami
    ports:
      - "9000:80"
"#;

    fn test_vpn_config() -> VpnConfig {
        VpnConfig {
            enabled: true,
            provider: Some("protonvpn".to_string()),
            vpn_type: Some("openvpn".to_string()),
            server_countries: Some("Netherlands".to_string()),
            port_forwarding: true,
            env_vars: HashMap::from([
                ("OPENVPN_USER".to_string(), "user123".to_string()),
                ("OPENVPN_PASSWORD".to_string(), "pass456".to_string()),
            ]),
        }
    }

    #[test]
    fn inject_adds_gluetun_service() {
        let config = test_vpn_config();
        let result = inject_vpn_sidecar(BASIC_COMPOSE, "whoami", &config).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let gluetun = &doc["services"]["gluetun"];
        assert_eq!(gluetun["image"].as_str(), Some("qmcgaw/gluetun:latest"));
        assert_eq!(
            gluetun["container_name"].as_str(),
            Some("myground-whoami-vpn")
        );
        assert_eq!(
            gluetun["env_file"].as_str(),
            Some("./vpn-sidecar.env")
        );

        // cap_add
        let caps = gluetun["cap_add"].as_sequence().unwrap();
        assert!(caps.iter().any(|v| v.as_str() == Some("NET_ADMIN")));

        // devices
        let devs = gluetun["devices"].as_sequence().unwrap();
        assert!(devs
            .iter()
            .any(|v| v.as_str() == Some("/dev/net/tun:/dev/net/tun")));
    }

    #[test]
    fn inject_moves_ports_to_gluetun() {
        let config = test_vpn_config();
        let result = inject_vpn_sidecar(BASIC_COMPOSE, "whoami", &config).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = &doc["services"]["whoami"];
        assert!(main.get("ports").is_none());

        let gluetun = &doc["services"]["gluetun"];
        let ports = gluetun["ports"].as_sequence().unwrap();
        assert!(!ports.is_empty());
    }

    #[test]
    fn inject_sets_network_mode_and_depends_on() {
        let config = test_vpn_config();
        let result = inject_vpn_sidecar(BASIC_COMPOSE, "whoami", &config).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = &doc["services"]["whoami"];
        assert_eq!(
            main["network_mode"].as_str(),
            Some("service:gluetun")
        );
        let deps = main["depends_on"].as_sequence().unwrap();
        assert!(deps.iter().any(|v| v.as_str() == Some("gluetun")));
    }

    #[test]
    fn inject_rejects_host_network() {
        let yaml = r#"services:
  beszel:
    image: henrygd/beszel
    container_name: myground-beszel
    network_mode: host
"#;
        let config = test_vpn_config();
        let result = inject_vpn_sidecar(yaml, "beszel", &config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("incompatible with network_mode: host"));
    }

    #[test]
    fn remove_restores_original() {
        let config = test_vpn_config();
        let injected = inject_vpn_sidecar(BASIC_COMPOSE, "whoami", &config).unwrap();
        let restored = remove_vpn_sidecar(&injected).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();
        let main = &doc["services"]["whoami"];
        assert!(main.get("ports").is_some());
        assert!(main.get("network_mode").is_none());
        assert!(main.get("depends_on").is_none());
        assert!(doc["services"].get("gluetun").is_none());
    }

    #[test]
    fn remove_noop_when_no_gluetun() {
        let result = remove_vpn_sidecar(BASIC_COMPOSE).unwrap();
        assert!(!result.contains("gluetun"));
    }

    #[test]
    fn write_vpn_env_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_vpn_config();
        write_vpn_env(dir.path(), &config, None).unwrap();

        let content = std::fs::read_to_string(dir.path().join("vpn-sidecar.env")).unwrap();
        assert!(content.contains("VPN_SERVICE_PROVIDER=protonvpn"));
        assert!(content.contains("VPN_TYPE=openvpn"));
        assert!(content.contains("SERVER_COUNTRIES=Netherlands"));
        assert!(content.contains("VPN_PORT_FORWARDING=on"));
        assert!(content.contains("OPENVPN_USER=user123"));
        assert!(content.contains("OPENVPN_PASSWORD=pass456"));
    }

    #[test]
    fn write_vpn_env_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let config = VpnConfig::default();
        write_vpn_env(dir.path(), &config, None).unwrap();

        let content = std::fs::read_to_string(dir.path().join("vpn-sidecar.env")).unwrap();
        assert!(!content.contains("VPN_SERVICE_PROVIDER"));
        assert!(!content.contains("VPN_PORT_FORWARDING"));
    }

    #[test]
    fn write_vpn_env_with_port_forward_command() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_vpn_config();
        let cmd = "/bin/sh -c 'wget -qO- http://127.0.0.1:8080/api/v2/app/setPreferences'";
        write_vpn_env(dir.path(), &config, Some(cmd)).unwrap();

        let content = std::fs::read_to_string(dir.path().join("vpn-sidecar.env")).unwrap();
        assert!(content.contains("VPN_PORT_FORWARDING=on"));
        assert!(content.contains(&format!("VPN_PORT_FORWARDING_UP_COMMAND={cmd}")));
    }

    #[test]
    fn write_vpn_env_no_port_forward_command_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = test_vpn_config();
        config.port_forwarding = false;
        write_vpn_env(dir.path(), &config, Some("some command")).unwrap();

        let content = std::fs::read_to_string(dir.path().join("vpn-sidecar.env")).unwrap();
        assert!(!content.contains("VPN_PORT_FORWARDING"));
        assert!(!content.contains("VPN_PORT_FORWARDING_UP_COMMAND"));
    }

    #[test]
    fn is_vpn_enabled_checks_state() {
        let mut state = crate::config::InstalledAppState::default();
        assert!(!is_vpn_enabled(&state));

        state.vpn = Some(VpnConfig {
            enabled: false,
            ..Default::default()
        });
        assert!(!is_vpn_enabled(&state));

        state.vpn = Some(VpnConfig {
            enabled: true,
            ..Default::default()
        });
        assert!(is_vpn_enabled(&state));
    }

    #[test]
    fn inject_multi_service_only_first() {
        let yaml = r#"services:
  app:
    image: nextcloud:latest
    container_name: myground-nextcloud
    ports:
      - "9000:80"
  db:
    image: postgres:16
    container_name: myground-nextcloud-db
"#;
        let config = test_vpn_config();
        let result = inject_vpn_sidecar(yaml, "nextcloud", &config).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        // Main app gets network_mode
        assert_eq!(
            doc["services"]["app"]["network_mode"].as_str(),
            Some("service:gluetun")
        );
        // DB does NOT get network_mode
        assert!(doc["services"]["db"].get("network_mode").is_none());
    }
}
