use std::path::Path;
use std::process::Stdio;

use crate::config;
use crate::error::ServiceError;

const EXIT_NODE_CONTAINER: &str = "myground-tailscale-exit";

// ── Exit Node ───────────────────────────────────────────────────────────────

/// Generate docker-compose.yml content for the exit node.
fn generate_exit_node_compose(auth_key: Option<&str>, pihole_ip: Option<&str>) -> String {
    let auth_line = match auth_key {
        Some(key) => format!("\n      TS_AUTHKEY: \"{key}\""),
        None => String::new(),
    };

    let dns_line = match pihole_ip {
        Some(ip) => format!("\n    dns:\n      - \"{ip}\""),
        None => String::new(),
    };

    format!(
        r#"services:
  tailscale-exit:
    image: tailscale/tailscale:latest
    container_name: {EXIT_NODE_CONTAINER}
    hostname: myground-exit
    environment:
      TS_STATE_DIR: /var/lib/tailscale
      TS_EXTRA_ARGS: "--advertise-exit-node --accept-dns=false"{auth_line}
    volumes:
      - ./state:/var/lib/tailscale
    cap_add:
      - net_admin
      - sys_module
    restart: unless-stopped{dns_line}
"#
    )
}

/// Ensure the exit node is running. Creates compose file and starts the container.
pub async fn ensure_exit_node(base: &Path, auth_key: Option<&str>) -> Result<(), ServiceError> {
    let exit_dir = base.join("tailscale-exit");
    std::fs::create_dir_all(&exit_dir)
        .map_err(|e| ServiceError::Io(format!("Create tailscale-exit dir: {e}")))?;

    let state_dir = exit_dir.join("state");
    std::fs::create_dir_all(&state_dir)
        .map_err(|e| ServiceError::Io(format!("Create tailscale state dir: {e}")))?;

    let pihole_ip = get_pihole_ip().await;
    let compose = generate_exit_node_compose(auth_key, pihole_ip.as_deref());

    std::fs::write(exit_dir.join("docker-compose.yml"), &compose)
        .map_err(|e| ServiceError::Io(format!("Write exit node compose: {e}")))?;

    let compose_cmd = crate::compose::detect_command().await?;
    crate::compose::run(&compose_cmd, &exit_dir, &["up", "-d"]).await?;

    Ok(())
}

/// Stop the exit node.
pub async fn stop_exit_node(base: &Path) -> Result<(), ServiceError> {
    let exit_dir = base.join("tailscale-exit");
    if !exit_dir.join("docker-compose.yml").exists() {
        return Ok(());
    }

    let compose_cmd = crate::compose::detect_command().await?;
    let _ = crate::compose::run(
        &compose_cmd,
        &exit_dir,
        &["down", "--remove-orphans"],
    )
    .await;

    Ok(())
}

/// Check if the exit node container is running.
pub async fn is_exit_node_running() -> bool {
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", EXIT_NODE_CONTAINER])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

/// Update exit node DNS based on Pi-hole availability.
pub async fn update_exit_node_dns(base: &Path) -> Result<(), ServiceError> {
    let exit_dir = base.join("tailscale-exit");
    if !exit_dir.join("docker-compose.yml").exists() {
        return Ok(());
    }

    let pihole_ip = get_pihole_ip().await;
    let compose = generate_exit_node_compose(None, pihole_ip.as_deref());

    std::fs::write(exit_dir.join("docker-compose.yml"), &compose)
        .map_err(|e| ServiceError::Io(format!("Write exit node compose: {e}")))?;

    let compose_cmd = crate::compose::detect_command().await?;
    crate::compose::run(&compose_cmd, &exit_dir, &["up", "-d"]).await?;

    Ok(())
}

/// Clean up exit node: stop container, remove directory.
pub async fn cleanup_exit_node(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    let _ = stop_exit_node(base).await;

    // Force-remove container
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", EXIT_NODE_CONTAINER])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;
    actions.push(format!("Removed exit node container: {EXIT_NODE_CONTAINER}"));

    let exit_dir = base.join("tailscale-exit");
    if exit_dir.exists() {
        if std::fs::remove_dir_all(&exit_dir).is_ok() {
            actions.push(format!("Removed exit node data: {}", exit_dir.display()));
        }
    }

    actions
}

// ── Pi-hole DNS ─────────────────────────────────────────────────────────────

/// Get Pi-hole container IP address (if running).
async fn get_pihole_ip() -> Option<String> {
    let output = tokio::process::Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            "myground-pihole",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ip.is_empty() {
        None
    } else {
        Some(ip)
    }
}

// ── Tailnet detection ───────────────────────────────────────────────────────

/// Try to detect the tailnet name from the exit node container.
pub async fn detect_tailnet() -> Option<String> {
    // Try tailscale status from exit node
    let output = tokio::process::Command::new("docker")
        .args(["exec", EXIT_NODE_CONTAINER, "tailscale", "status", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    let content = String::from_utf8_lossy(&output.stdout);
    // Look for "MagicDNSSuffix" in the JSON output
    for line in content.lines() {
        if let Some(domain) = extract_tailnet_domain(line) {
            return Some(domain);
        }
    }

    // Fallback: check docker logs
    let logs = tokio::process::Command::new("docker")
        .args(["logs", "--tail", "50", EXIT_NODE_CONTAINER])
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
            let parts: Vec<&str> = word.split('.').collect();
            if parts.len() >= 3 {
                let tailnet = parts[parts.len() - 3..].join(".");
                return Some(tailnet);
            }
        }
    }
    None
}

// ── Port extraction ─────────────────────────────────────────────────────────

/// Extract the first container port from a compose YAML (from the port mapping).
pub fn extract_container_port(compose_yaml: &str) -> Option<u16> {
    let doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml).ok()?;
    let services = doc.get("services")?.as_mapping()?;
    let (_key, svc) = services.iter().next()?;
    let ports = svc.get("ports")?.as_sequence()?;
    let first_port = ports.first()?.as_str()?;

    // Port format: "HOST:CONTAINER" or "HOST:CONTAINER/tcp" or "IP:HOST:CONTAINER"
    let container_part = first_port.split(':').last()?;
    let port_str = container_part.split('/').next()?;
    port_str.parse().ok()
}

// ── Sidecar injection ───────────────────────────────────────────────────────

/// Container name for a service's Tailscale sidecar.
fn sidecar_container_name(instance_id: &str) -> String {
    format!("myground-{instance_id}-ts")
}

/// Generate TS_SERVE_CONFIG JSON for a sidecar.
pub fn generate_serve_config(proxy_target: &str, container_port: u16) -> String {
    serde_json::json!({
        "TCP": {
            "443": {
                "HTTPS": true
            }
        },
        "Web": {
            format!("${{TS_CERT_DOMAIN}}:{container_port}"): {
                "Handlers": {
                    "/": {
                        "Proxy": proxy_target
                    }
                }
            }
        }
    })
    .to_string()
}

/// Write ts-serve.json alongside the compose file.
pub fn write_serve_config(
    svc_dir: &Path,
    container_port: u16,
    proxy_target: &str,
) -> Result<(), ServiceError> {
    let config = generate_serve_config(proxy_target, container_port);
    std::fs::write(svc_dir.join("ts-serve.json"), config)
        .map_err(|e| ServiceError::Io(format!("Write ts-serve.json: {e}")))?;
    Ok(())
}

/// Inject a Tailscale sidecar into a compose YAML.
///
/// - `mode = "sidecar"`: main service uses `network_mode: service:ts-sidecar`, ports move to sidecar
/// - `mode = "network"`: main service keeps its own network, sidecar joins a shared Docker network
pub fn inject_tailscale_sidecar(
    compose_yaml: &str,
    instance_id: &str,
    _container_port: u16,
    mode: &str,
    auth_key: Option<&str>,
) -> Result<String, ServiceError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| ServiceError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| ServiceError::Io("No 'services' key in compose YAML".to_string()))?;

    let sidecar_name = sidecar_container_name(instance_id);
    let ts_hostname = format!("myground-{instance_id}");
    let volume_name = format!("ts-{instance_id}-state");

    if mode == "sidecar" {
        // Move ports from main service to sidecar
        let first_key = services
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| ServiceError::Io("No services in compose YAML".to_string()))?;

        let main_svc = services
            .get_mut(&first_key)
            .and_then(|s| s.as_mapping_mut())
            .ok_or_else(|| ServiceError::Io("Main service is not a mapping".to_string()))?;

        // Extract ports from main service
        let ports_key = serde_yaml::Value::String("ports".to_string());
        let ports_value = main_svc.remove(&ports_key);

        // Set network_mode on main service
        main_svc.insert(
            serde_yaml::Value::String("network_mode".to_string()),
            serde_yaml::Value::String("service:ts-sidecar".to_string()),
        );

        // Add depends_on
        let depends = serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
            "ts-sidecar".to_string(),
        )]);
        main_svc.insert(
            serde_yaml::Value::String("depends_on".to_string()),
            depends,
        );

        // Build sidecar service
        let mut sidecar = serde_yaml::Mapping::new();
        sidecar.insert(
            serde_yaml::Value::String("image".to_string()),
            serde_yaml::Value::String("tailscale/tailscale:latest".to_string()),
        );
        sidecar.insert(
            serde_yaml::Value::String("container_name".to_string()),
            serde_yaml::Value::String(sidecar_name),
        );
        sidecar.insert(
            serde_yaml::Value::String("hostname".to_string()),
            serde_yaml::Value::String(ts_hostname),
        );
        sidecar.insert(
            serde_yaml::Value::String("restart".to_string()),
            serde_yaml::Value::String("unless-stopped".to_string()),
        );

        // Environment
        let mut env = serde_yaml::Mapping::new();
        env.insert(
            serde_yaml::Value::String("TS_STATE_DIR".to_string()),
            serde_yaml::Value::String("/var/lib/tailscale".to_string()),
        );
        env.insert(
            serde_yaml::Value::String("TS_SERVE_CONFIG".to_string()),
            serde_yaml::Value::String("/config/ts-serve.json".to_string()),
        );
        if let Some(key) = auth_key {
            env.insert(
                serde_yaml::Value::String("TS_AUTHKEY".to_string()),
                serde_yaml::Value::String(key.to_string()),
            );
        }
        sidecar.insert(
            serde_yaml::Value::String("environment".to_string()),
            serde_yaml::Value::Mapping(env),
        );

        // Volumes
        let volumes = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String(format!("{volume_name}:/var/lib/tailscale")),
            serde_yaml::Value::String("./ts-serve.json:/config/ts-serve.json:ro".to_string()),
        ]);
        sidecar.insert(
            serde_yaml::Value::String("volumes".to_string()),
            volumes,
        );

        // Ports (moved from main service)
        if let Some(ports) = ports_value {
            sidecar.insert(ports_key, ports);
        }

        services.insert(
            serde_yaml::Value::String("ts-sidecar".to_string()),
            serde_yaml::Value::Mapping(sidecar),
        );
    } else if mode == "network" {
        // Network mode: add sidecar on a shared Docker network, main keeps its ports
        let network_name = format!("ts-net-{instance_id}");

        // Add network to main service
        let first_key = services
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| ServiceError::Io("No services in compose YAML".to_string()))?;

        let main_svc = services
            .get_mut(&first_key)
            .and_then(|s| s.as_mapping_mut())
            .ok_or_else(|| ServiceError::Io("Main service is not a mapping".to_string()))?;

        // Only add networks if the main service doesn't already use network_mode: host
        let has_network_mode = main_svc
            .get(&serde_yaml::Value::String("network_mode".to_string()))
            .is_some();

        if !has_network_mode {
            let networks = serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
                network_name.clone(),
            )]);
            main_svc.insert(
                serde_yaml::Value::String("networks".to_string()),
                networks,
            );
        }

        // Build sidecar service
        let mut sidecar = serde_yaml::Mapping::new();
        sidecar.insert(
            serde_yaml::Value::String("image".to_string()),
            serde_yaml::Value::String("tailscale/tailscale:latest".to_string()),
        );
        sidecar.insert(
            serde_yaml::Value::String("container_name".to_string()),
            serde_yaml::Value::String(sidecar_name),
        );
        sidecar.insert(
            serde_yaml::Value::String("hostname".to_string()),
            serde_yaml::Value::String(ts_hostname),
        );
        sidecar.insert(
            serde_yaml::Value::String("restart".to_string()),
            serde_yaml::Value::String("unless-stopped".to_string()),
        );

        // Environment
        let mut env = serde_yaml::Mapping::new();
        env.insert(
            serde_yaml::Value::String("TS_STATE_DIR".to_string()),
            serde_yaml::Value::String("/var/lib/tailscale".to_string()),
        );
        env.insert(
            serde_yaml::Value::String("TS_SERVE_CONFIG".to_string()),
            serde_yaml::Value::String("/config/ts-serve.json".to_string()),
        );
        if let Some(key) = auth_key {
            env.insert(
                serde_yaml::Value::String("TS_AUTHKEY".to_string()),
                serde_yaml::Value::String(key.to_string()),
            );
        }
        sidecar.insert(
            serde_yaml::Value::String("environment".to_string()),
            serde_yaml::Value::Mapping(env),
        );

        // Volumes
        let volumes = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String(format!("{volume_name}:/var/lib/tailscale")),
            serde_yaml::Value::String("./ts-serve.json:/config/ts-serve.json:ro".to_string()),
        ]);
        sidecar.insert(
            serde_yaml::Value::String("volumes".to_string()),
            volumes,
        );

        // Sidecar always gets the shared network
        let sidecar_networks = serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
            network_name.clone(),
        )]);
        sidecar.insert(
            serde_yaml::Value::String("networks".to_string()),
            sidecar_networks,
        );

        services.insert(
            serde_yaml::Value::String("ts-sidecar".to_string()),
            serde_yaml::Value::Mapping(sidecar),
        );

        // Add top-level networks definition
        let mut net_def = serde_yaml::Mapping::new();
        net_def.insert(
            serde_yaml::Value::String("driver".to_string()),
            serde_yaml::Value::String("bridge".to_string()),
        );

        let mut networks = doc
            .get("networks")
            .and_then(|n| n.as_mapping())
            .cloned()
            .unwrap_or_default();
        networks.insert(
            serde_yaml::Value::String(network_name),
            serde_yaml::Value::Mapping(net_def),
        );
        doc.as_mapping_mut().unwrap().insert(
            serde_yaml::Value::String("networks".to_string()),
            serde_yaml::Value::Mapping(networks),
        );
    }

    // Add named volume for sidecar state
    let mut volumes = doc
        .get("volumes")
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();
    volumes.insert(
        serde_yaml::Value::String(volume_name),
        serde_yaml::Value::Null,
    );
    doc.as_mapping_mut().unwrap().insert(
        serde_yaml::Value::String("volumes".to_string()),
        serde_yaml::Value::Mapping(volumes),
    );

    serde_yaml::to_string(&doc).map_err(|e| ServiceError::Io(format!("Serialize compose YAML: {e}")))
}

/// Remove the Tailscale sidecar from a compose YAML.
pub fn remove_tailscale_sidecar(compose_yaml: &str) -> Result<String, ServiceError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| ServiceError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| ServiceError::Io("No 'services' key in compose YAML".to_string()))?;

    // Check if sidecar exists
    let sidecar_key = serde_yaml::Value::String("ts-sidecar".to_string());
    let had_sidecar = services.contains_key(&sidecar_key);

    if !had_sidecar {
        return serde_yaml::to_string(&doc)
            .map_err(|e| ServiceError::Io(format!("Serialize compose YAML: {e}")));
    }

    // Get sidecar's ports (to move back to main service if it was in sidecar mode)
    let sidecar_ports = services
        .get(&sidecar_key)
        .and_then(|s| s.get("ports"))
        .cloned();

    // Remove sidecar service
    services.remove(&sidecar_key);

    // Check if main service has network_mode: service:ts-sidecar
    let first_key = services.keys().next().cloned();
    if let Some(key) = first_key {
        if let Some(main_svc) = services.get_mut(&key).and_then(|s| s.as_mapping_mut()) {
            let nm_key = serde_yaml::Value::String("network_mode".to_string());
            let is_sidecar_mode = main_svc
                .get(&nm_key)
                .and_then(|v| v.as_str())
                .map(|s| s == "service:ts-sidecar")
                .unwrap_or(false);

            if is_sidecar_mode {
                // Remove network_mode
                main_svc.remove(&nm_key);

                // Remove depends_on for ts-sidecar
                let deps_key = serde_yaml::Value::String("depends_on".to_string());
                if let Some(deps) = main_svc.get_mut(&deps_key) {
                    if let Some(seq) = deps.as_sequence_mut() {
                        seq.retain(|v| v.as_str() != Some("ts-sidecar"));
                        if seq.is_empty() {
                            main_svc.remove(&deps_key);
                        }
                    }
                }

                // Move ports back
                if let Some(ports) = sidecar_ports {
                    main_svc.insert(
                        serde_yaml::Value::String("ports".to_string()),
                        ports,
                    );
                }
            } else {
                // Network mode: just remove the networks entry for the ts-net
                let networks_key = serde_yaml::Value::String("networks".to_string());
                if let Some(nets) = main_svc.get_mut(&networks_key) {
                    if let Some(seq) = nets.as_sequence_mut() {
                        seq.retain(|v| {
                            v.as_str()
                                .map(|s| !s.starts_with("ts-net-"))
                                .unwrap_or(true)
                        });
                        if seq.is_empty() {
                            main_svc.remove(&networks_key);
                        }
                    }
                }
            }
        }
    }

    // Clean up top-level volumes (remove ts-*-state)
    if let Some(volumes) = doc.get_mut("volumes").and_then(|v| v.as_mapping_mut()) {
        let ts_keys: Vec<_> = volumes
            .keys()
            .filter(|k| {
                k.as_str()
                    .map(|s| s.starts_with("ts-") && s.ends_with("-state"))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        for k in ts_keys {
            volumes.remove(&k);
        }
        if volumes.is_empty() {
            doc.as_mapping_mut()
                .unwrap()
                .remove(&serde_yaml::Value::String("volumes".to_string()));
        }
    }

    // Clean up top-level networks (remove ts-net-*)
    if let Some(networks) = doc.get_mut("networks").and_then(|v| v.as_mapping_mut()) {
        let ts_keys: Vec<_> = networks
            .keys()
            .filter(|k| {
                k.as_str()
                    .map(|s| s.starts_with("ts-net-"))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        for k in ts_keys {
            networks.remove(&k);
        }
        if networks.is_empty() {
            doc.as_mapping_mut()
                .unwrap()
                .remove(&serde_yaml::Value::String("networks".to_string()));
        }
    }

    serde_yaml::to_string(&doc).map_err(|e| ServiceError::Io(format!("Serialize compose YAML: {e}")))
}

/// Check if a sidecar container is running for a specific service.
pub async fn is_sidecar_running(instance_id: &str) -> bool {
    let container = sidecar_container_name(instance_id);
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", &container])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

// ── Migration from TSDProxy ─────────────────────────────────────────────────

/// Remove old TSDProxy labels from a compose YAML string.
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
                    if labels_map.is_empty() {
                        svc_map.remove(&labels_key);
                    }
                }
            }
        }
    }

    serde_yaml::to_string(&doc).map_err(|e| ServiceError::Io(format!("Serialize compose YAML: {e}")))
}

/// Migrate from old TSDProxy to new sidecar architecture.
/// Called on startup if tsdproxy/ directory exists.
pub async fn migrate_from_tsdproxy(base: &Path) {
    let tsdproxy_dir = base.join("tsdproxy");
    if !tsdproxy_dir.exists() {
        return;
    }

    tracing::info!("Migrating from TSDProxy to sidecar architecture...");

    // Read old auth key from config (still available since we read but don't write it)
    let old_auth_key = config::try_load_tailscale(base).auth_key;

    // Stop TSDProxy
    if tsdproxy_dir.join("docker-compose.yml").exists() {
        if let Ok(compose_cmd) = crate::compose::detect_command().await {
            let _ = crate::compose::run(
                &compose_cmd,
                &tsdproxy_dir,
                &["down", "--remove-orphans", "--volumes"],
            )
            .await;
        }
    }

    // Force-remove TSDProxy container
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", "myground-tsdproxy"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    // Remove TSDProxy labels from all installed service compose files
    let installed = config::list_installed_services(base);
    for id in &installed {
        let compose_path = config::service_dir(base, id).join("docker-compose.yml");
        if let Ok(yaml) = std::fs::read_to_string(&compose_path) {
            if let Ok(cleaned) = remove_tsdproxy_labels(&yaml) {
                let _ = std::fs::write(&compose_path, cleaned);
            }
        }
    }

    // Remove tsdproxy directory
    let _ = std::fs::remove_dir_all(&tsdproxy_dir);

    // Start exit node with old auth key
    if let Err(e) = ensure_exit_node(base, old_auth_key.as_deref()).await {
        tracing::warn!("Failed to start exit node during migration: {e}");
    }

    tracing::info!("TSDProxy migration complete.");
}

// ── Legacy cleanup (for nuke) ───────────────────────────────────────────────

/// Clean up old TSDProxy directory if it exists (for nuke).
pub async fn cleanup_tsdproxy(base: &Path) -> Vec<String> {
    let mut actions = Vec::new();

    // Force-remove TSDProxy container
    let _ = tokio::process::Command::new("docker")
        .args(["rm", "-f", "myground-tsdproxy"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await;

    let tsdproxy_dir = base.join("tsdproxy");
    if tsdproxy_dir.exists() {
        if std::fs::remove_dir_all(&tsdproxy_dir).is_ok() {
            actions.push("Removed old TSDProxy data".to_string());
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn inject_sidecar_mode_adds_sidecar_service() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    container_name: myground-whoami
    ports:
      - "9000:80"
"#;
        let result = inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None).unwrap();
        assert!(result.contains("ts-sidecar"));
        assert!(result.contains("myground-whoami-ts"));
        assert!(result.contains("network_mode"));
        assert!(result.contains("service:ts-sidecar"));
        assert!(result.contains("ts-whoami-state"));
    }

    #[test]
    fn inject_sidecar_mode_moves_ports() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    container_name: myground-whoami
    ports:
      - "9000:80"
"#;
        let result = inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None).unwrap();
        // Main service should have network_mode instead of ports
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = doc.get("services").unwrap().get("whoami").unwrap();
        assert!(main.get("ports").is_none());
        assert!(main.get("network_mode").is_some());

        // Sidecar should have the ports
        let sidecar = doc.get("services").unwrap().get("ts-sidecar").unwrap();
        assert!(sidecar.get("ports").is_some());
    }

    #[test]
    fn inject_network_mode_keeps_main_ports() {
        let yaml = r#"services:
  pihole:
    image: pihole/pihole
    container_name: myground-pihole
    ports:
      - "53:53/tcp"
      - "8086:80"
"#;
        let result = inject_tailscale_sidecar(yaml, "pihole", 80, "network", None).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = doc.get("services").unwrap().get("pihole").unwrap();
        // Main service keeps its ports
        assert!(main.get("ports").is_some());
        assert!(main.get("network_mode").is_none());
        // Has networks
        assert!(main.get("networks").is_some());
    }

    #[test]
    fn inject_network_mode_host_service() {
        // Beszel uses network_mode: host — should not add networks to main
        let yaml = r#"services:
  beszel:
    image: henrygd/beszel
    container_name: myground-beszel
    network_mode: host
"#;
        let result = inject_tailscale_sidecar(yaml, "beszel", 8085, "network", None).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = doc.get("services").unwrap().get("beszel").unwrap();
        // Main service should NOT have networks (it uses network_mode: host)
        assert!(main.get("networks").is_none());
        assert_eq!(main.get("network_mode").unwrap().as_str(), Some("host"));

        // Sidecar should have networks
        let sidecar = doc.get("services").unwrap().get("ts-sidecar").unwrap();
        assert!(sidecar.get("networks").is_some());
    }

    #[test]
    fn inject_with_auth_key() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    ports:
      - "9000:80"
"#;
        let result =
            inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", Some("tskey-auth-xxx"))
                .unwrap();
        assert!(result.contains("tskey-auth-xxx"));
    }

    #[test]
    fn remove_sidecar_restores_sidecar_mode() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    container_name: myground-whoami
    ports:
      - "9000:80"
"#;
        let injected = inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None).unwrap();
        let restored = remove_tailscale_sidecar(&injected).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();
        let main = doc.get("services").unwrap().get("whoami").unwrap();
        // Should have ports back
        assert!(main.get("ports").is_some());
        // Should NOT have network_mode
        assert!(main.get("network_mode").is_none());
        // Should NOT have depends_on
        assert!(main.get("depends_on").is_none());
        // No sidecar service
        assert!(doc.get("services").unwrap().get("ts-sidecar").is_none());
    }

    #[test]
    fn remove_sidecar_restores_network_mode() {
        let yaml = r#"services:
  pihole:
    image: pihole/pihole
    container_name: myground-pihole
    ports:
      - "53:53/tcp"
"#;
        let injected = inject_tailscale_sidecar(yaml, "pihole", 80, "network", None).unwrap();
        let restored = remove_tailscale_sidecar(&injected).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();
        assert!(doc.get("services").unwrap().get("ts-sidecar").is_none());
        let main = doc.get("services").unwrap().get("pihole").unwrap();
        assert!(main.get("ports").is_some());
    }

    #[test]
    fn remove_sidecar_noop_when_none() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    ports:
      - "9000:80"
"#;
        let result = remove_tailscale_sidecar(yaml).unwrap();
        assert!(!result.contains("ts-sidecar"));
    }

    #[test]
    fn generate_serve_config_valid_json() {
        let config = generate_serve_config("http://127.0.0.1:80", 80);
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert!(parsed.get("TCP").is_some());
        assert!(parsed.get("Web").is_some());
    }

    #[test]
    fn generate_exit_node_compose_basic() {
        let compose = generate_exit_node_compose(Some("tskey-auth-xxx"), None);
        assert!(compose.contains("tskey-auth-xxx"));
        assert!(compose.contains(EXIT_NODE_CONTAINER));
        assert!(compose.contains("advertise-exit-node"));
        assert!(!compose.contains("dns:"));
    }

    #[test]
    fn generate_exit_node_compose_with_pihole_dns() {
        let compose = generate_exit_node_compose(None, Some("172.17.0.5"));
        assert!(compose.contains("dns:"));
        assert!(compose.contains("172.17.0.5"));
    }

    #[test]
    fn remove_tsdproxy_labels_cleans_old_labels() {
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
        assert!(!result.contains("tsdproxy.container_port"));
    }
}
