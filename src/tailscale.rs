use std::path::Path;
use std::process::Stdio;

use tokio::io::AsyncBufReadExt;

use crate::config;
use crate::error::AppError;
use crate::state::AppState;

const EXIT_NODE_CONTAINER: &str = "myground-tailscale-exit";

// ── Exit Node ───────────────────────────────────────────────────────────────

/// Public wrapper for WebSocket handler.
pub async fn get_pihole_ip_public() -> Option<String> {
    get_pihole_ip().await
}

/// Public wrapper for WebSocket handler.
pub fn generate_exit_node_compose_public(pihole_ip: Option<&str>, hostname: &str) -> String {
    generate_exit_node_compose(pihole_ip, hostname)
}

/// Generate docker-compose.yml content for the exit node.
fn generate_exit_node_compose(pihole_ip: Option<&str>, hostname: &str) -> String {
    let (dns_line, networks_svc, networks_top) = match pihole_ip {
        Some(ip) => (
            format!("\n    dns:\n      - \"{ip}\""),
            "\n    networks:\n      - default\n      - pihole_net".to_string(),
            "\nnetworks:\n  pihole_net:\n    external: true\n    name: pihole_default\n".to_string(),
        ),
        None => (String::new(), String::new(), String::new()),
    };

    format!(
        r#"services:
  tailscale-exit:
    image: tailscale/tailscale:latest
    container_name: {EXIT_NODE_CONTAINER}
    hostname: {hostname}
    env_file: .env
    environment:
      TS_STATE_DIR: /var/lib/tailscale
      TS_SERVE_CONFIG: /config/ts-serve.json
      TS_EXTRA_ARGS: "--advertise-exit-node --accept-dns=false"
    volumes:
      - ./state:/var/lib/tailscale
      - ./ts-serve.json:/config/ts-serve.json:ro
    cap_add:
      - net_admin
      - sys_module
    extra_hosts:
      - "host.docker.internal:host-gateway"
    restart: unless-stopped{dns_line}{networks_svc}
{networks_top}"#
    )
}

/// Ensure the exit node is running. Creates compose file and starts the container.
pub async fn ensure_exit_node(base: &Path, auth_key: Option<&str>, pihole_dns: bool) -> Result<(), AppError> {
    let exit_dir = base.join("tailscale-exit");
    std::fs::create_dir_all(&exit_dir)
        .map_err(|e| AppError::Io(format!("Create tailscale-exit dir: {e}")))?;

    let state_dir = exit_dir.join("state");
    std::fs::create_dir_all(&state_dir)
        .map_err(|e| AppError::Io(format!("Create tailscale state dir: {e}")))?;

    let ts_cfg = config::try_load_tailscale(base);
    let hostname = ts_cfg.exit_hostname.as_deref().unwrap_or("myground");
    let pihole_ip = if pihole_dns { get_pihole_ip().await } else { None };
    let compose = generate_exit_node_compose(pihole_ip.as_deref(), hostname);

    let compose_path = exit_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose)
        .map_err(|e| AppError::Io(format!("Write exit node compose: {e}")))?;
    crate::compose::restrict_file_permissions(&compose_path);

    // Write serve config so the exit node proxies HTTPS → MyGround UI
    let serve_config = generate_serve_config("http://host.docker.internal:8080");
    std::fs::write(exit_dir.join("ts-serve.json"), &serve_config)
        .map_err(|e| AppError::Io(format!("Write exit node ts-serve.json: {e}")))?;

    // Write auth key to .env (only when provided — first start)
    if let Some(key) = auth_key {
        let env_path = exit_dir.join(".env");
        std::fs::write(&env_path, format!("TS_AUTHKEY={key}\n"))
            .map_err(|e| AppError::Io(format!("Write exit node .env: {e}")))?;
        crate::compose::restrict_file_permissions(&env_path);
    }

    let compose_cmd = crate::compose::detect_command().await?;
    crate::compose::run(&compose_cmd, &exit_dir, &["up", "-d"]).await?;

    Ok(())
}

/// Read the Tailscale auth key from the exit node's `.env` file.
/// Returns `None` if the file doesn't exist or has no key.
pub fn read_exit_node_auth_key(base: &Path) -> Option<String> {
    let env_path = base.join("tailscale-exit").join(".env");
    let contents = std::fs::read_to_string(env_path).ok()?;
    for line in contents.lines() {
        if let Some(val) = line.strip_prefix("TS_AUTHKEY=") {
            let val = val.trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Stop the exit node.
pub async fn stop_exit_node(base: &Path) -> Result<(), AppError> {
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
    crate::docker::is_container_running(EXIT_NODE_CONTAINER).await
}

/// Check if the exit node has been approved in the Tailscale admin panel.
/// Returns `None` if the container isn't running or status can't be determined.
/// Approved means AllowedIPs includes 0.0.0.0/0 (exit routes enabled).
pub async fn is_exit_node_approved() -> Option<bool> {
    let output = tokio::process::Command::new("docker")
        .args([
            "exec", EXIT_NODE_CONTAINER,
            "tailscale", "status", "--json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).ok()?;

    let self_node = json.get("Self")?;

    // If AllowedIPs includes 0.0.0.0/0, the exit node has been approved
    let allowed_ips = self_node.get("AllowedIPs")?.as_array()?;
    let approved = allowed_ips
        .iter()
        .any(|ip| ip.as_str().map_or(false, |s| s == "0.0.0.0/0" || s == "::/0"));

    Some(approved)
}

/// Update exit node DNS based on Pi-hole availability.
pub async fn update_exit_node_dns(base: &Path, pihole_dns: bool) -> Result<(), AppError> {
    let exit_dir = base.join("tailscale-exit");
    if !exit_dir.join("docker-compose.yml").exists() {
        return Ok(());
    }

    let ts_cfg = config::try_load_tailscale(base);
    let hostname = ts_cfg.exit_hostname.as_deref().unwrap_or("myground");
    let pihole_ip = if pihole_dns { get_pihole_ip().await } else { None };
    let compose = generate_exit_node_compose(pihole_ip.as_deref(), hostname);

    let compose_path = exit_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose)
        .map_err(|e| AppError::Io(format!("Write exit node compose: {e}")))?;
    crate::compose::restrict_file_permissions(&compose_path);

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
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}\n{{end}}",
            "myground-pihole",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    // Container may be on multiple networks; take the first non-empty IP.
    let out = String::from_utf8_lossy(&output.stdout);
    let ip = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(ip.to_string())
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

/// Check whether HTTPS certificates are available on the exit node.
///
/// Returns `Some(true)` if CertDomains is non-empty, `Some(false)` if empty,
/// or `None` if we can't determine (exit node not running).
pub async fn check_https_enabled() -> Option<bool> {
    let output = tokio::process::Command::new("docker")
        .args(["exec", EXIT_NODE_CONTAINER, "tailscale", "status", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let cert_domains = json
        .get("CertDomains")
        .and_then(|v| v.as_array());

    Some(cert_domains.map_or(false, |a| !a.is_empty()))
}

// ── Service name / port extraction ──────────────────────────────────────────

/// Extract the first service key from a compose YAML (used as Docker DNS name).
pub fn extract_main_service_name(compose_yaml: &str) -> Option<String> {
    let doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml).ok()?;
    let services = doc.get("services")?.as_mapping()?;
    let (key, _svc) = services.iter().next()?;
    key.as_str().map(|s| s.to_string())
}

/// Extract the container port from the main service's port mapping in a deployed
/// compose file.  Parses formats like `"127.0.0.1:9000:9000"` or `"8080:80"` and
/// returns the last (container) port.
pub fn extract_main_service_container_port(compose_yaml: &str) -> Option<u16> {
    let doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml).ok()?;
    let services = doc.get("services")?.as_mapping()?;
    let (_, svc) = services.iter().next()?;
    let ports = svc.get("ports")?.as_sequence()?;
    for entry in ports {
        let s = entry.as_str()?;
        // "[ip:]host_port:container_port"
        let container_port = s.rsplit(':').next()?;
        return container_port.parse().ok();
    }
    None
}

// ── Sidecar injection ───────────────────────────────────────────────────────

/// Container name for an app's Tailscale sidecar.
pub fn sidecar_container_name(instance_id: &str) -> String {
    format!("myground-{instance_id}-ts")
}

/// Log out the Tailscale sidecar so it is removed from the tailnet.
/// Best-effort — logs failures but does not propagate errors.
pub async fn logout_sidecar(instance_id: &str) {
    let container = sidecar_container_name(instance_id);
    match tokio::process::Command::new("docker")
        .args(["exec", &container, "tailscale", "logout"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            tracing::info!("Tailscale logout succeeded for {instance_id}");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(
                "Tailscale logout failed for {instance_id} (exit {}): {stderr}",
                output.status.code().unwrap_or(-1)
            );
        }
        Err(e) => {
            tracing::warn!("Tailscale logout failed for {instance_id}: {e}");
        }
    }
}

/// Generate TS_SERVE_CONFIG JSON for a sidecar.
pub fn generate_serve_config(proxy_target: &str) -> String {
    serde_json::json!({
        "TCP": {
            "443": {
                "HTTPS": true
            }
        },
        "Web": {
            "${TS_CERT_DOMAIN}:443": {
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
    proxy_target: &str,
) -> Result<(), AppError> {
    let config = generate_serve_config(proxy_target);
    std::fs::write(svc_dir.join("ts-serve.json"), config)
        .map_err(|e| AppError::Io(format!("Write ts-serve.json: {e}")))?;
    Ok(())
}

/// Build the common sidecar mapping shared by both sidecar and network modes.
fn build_sidecar_mapping(
    sidecar_name: &str,
    ts_hostname: &str,
    volume_name: &str,
) -> serde_yaml::Mapping {
    let mut sidecar = serde_yaml::Mapping::new();
    sidecar.insert(
        serde_yaml::Value::String("image".to_string()),
        serde_yaml::Value::String("tailscale/tailscale:latest".to_string()),
    );
    sidecar.insert(
        serde_yaml::Value::String("container_name".to_string()),
        serde_yaml::Value::String(sidecar_name.to_string()),
    );
    // NOTE: Do NOT set `hostname` here. Docker adds hostname to /etc/hosts,
    // which shadows the app container's name and breaks the proxy target.
    // TS_HOSTNAME env var handles the Tailscale machine name instead.
    sidecar.insert(
        serde_yaml::Value::String("restart".to_string()),
        serde_yaml::Value::String("unless-stopped".to_string()),
    );

    // Always reference the sidecar .env file — it's written during install
    // and must persist across compose regenerations (e.g. after storage updates).
    sidecar.insert(
        serde_yaml::Value::String("env_file".to_string()),
        serde_yaml::Value::String("./ts-sidecar.env".to_string()),
    );

    let mut env = serde_yaml::Mapping::new();
    env.insert(
        serde_yaml::Value::String("TS_STATE_DIR".to_string()),
        serde_yaml::Value::String("/var/lib/tailscale".to_string()),
    );
    env.insert(
        serde_yaml::Value::String("TS_SERVE_CONFIG".to_string()),
        serde_yaml::Value::String("/config/ts-serve.json".to_string()),
    );
    env.insert(
        serde_yaml::Value::String("TS_HOSTNAME".to_string()),
        serde_yaml::Value::String(ts_hostname.to_string()),
    );
    sidecar.insert(
        serde_yaml::Value::String("environment".to_string()),
        serde_yaml::Value::Mapping(env),
    );

    let volumes = serde_yaml::Value::Sequence(vec![
        serde_yaml::Value::String(format!("{volume_name}:/var/lib/tailscale")),
        serde_yaml::Value::String("./ts-serve.json:/config/ts-serve.json:ro".to_string()),
    ]);
    sidecar.insert(
        serde_yaml::Value::String("volumes".to_string()),
        volumes,
    );

    // Allow sidecar to reach apps on the host network via host.docker.internal
    sidecar.insert(
        serde_yaml::Value::String("extra_hosts".to_string()),
        serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
            "host.docker.internal:host-gateway".to_string(),
        )]),
    );

    sidecar
}

/// Inject a Tailscale sidecar into a compose YAML.
///
/// - `mode = "sidecar"`: main app uses `network_mode: service:ts-sidecar`, ports move to sidecar
/// - `mode = "network"`: main app keeps its own network, sidecar joins a shared Docker network
pub fn inject_tailscale_sidecar(
    compose_yaml: &str,
    instance_id: &str,
    _container_port: u16,
    mode: &str,
    _auth_key: Option<&str>,
    custom_hostname: Option<&str>,
) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("No 'services' key in compose YAML".to_string()))?;

    let sidecar_name = sidecar_container_name(instance_id);
    let default_hostname = format!("myground-{instance_id}");
    let ts_hostname = custom_hostname.unwrap_or(&default_hostname);
    let volume_name = format!("ts-{instance_id}-state");

    let first_key = services
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| AppError::Io("No entries in compose YAML".to_string()))?;

    let main_svc = services
        .get_mut(&first_key)
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("Main app entry is not a mapping".to_string()))?;

    let mut sidecar = build_sidecar_mapping(&sidecar_name, ts_hostname, &volume_name);

    if mode == "sidecar" {
        // Extract ports from main app and move to sidecar
        let ports_key = serde_yaml::Value::String("ports".to_string());
        let ports_value = main_svc.remove(&ports_key);

        main_svc.insert(
            serde_yaml::Value::String("network_mode".to_string()),
            serde_yaml::Value::String("service:ts-sidecar".to_string()),
        );

        main_svc.insert(
            serde_yaml::Value::String("depends_on".to_string()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
                "ts-sidecar".to_string(),
            )]),
        );

        if let Some(ports) = ports_value {
            sidecar.insert(serde_yaml::Value::String("ports".to_string()), ports);
        }
    } else if mode == "network" {
        let network_name = format!("ts-net-{instance_id}");

        let network_mode_val = main_svc
            .get(&serde_yaml::Value::String("network_mode".to_string()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let has_network_mode = network_mode_val.is_some();

        // If main app uses network_mode: service:gluetun (VPN active), add gluetun
        // to the tailscale network so the sidecar can reach it.
        let vpn_service = network_mode_val
            .as_deref()
            .and_then(|nm| nm.strip_prefix("service:"))
            .map(|s| s.to_string());

        if !has_network_mode {
            main_svc.insert(
                serde_yaml::Value::String("networks".to_string()),
                serde_yaml::Value::Sequence(vec![
                    serde_yaml::Value::String("default".to_string()),
                    serde_yaml::Value::String(network_name.clone()),
                ]),
            );
        }

        sidecar.insert(
            serde_yaml::Value::String("networks".to_string()),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::String(
                network_name.clone(),
            )]),
        );

        // If VPN is active, add the VPN service (gluetun) to the ts network
        if let Some(ref vpn_svc_name) = vpn_service {
            let services_for_vpn = doc
                .get_mut("services")
                .and_then(|s| s.as_mapping_mut())
                .unwrap();
            if let Some(vpn_svc) = services_for_vpn
                .get_mut(&serde_yaml::Value::String(vpn_svc_name.clone()))
                .and_then(|s| s.as_mapping_mut())
            {
                vpn_svc.insert(
                    serde_yaml::Value::String("networks".to_string()),
                    serde_yaml::Value::Sequence(vec![
                        serde_yaml::Value::String("default".to_string()),
                        serde_yaml::Value::String(network_name.clone()),
                    ]),
                );
            }
        }

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
            serde_yaml::Value::String("default".to_string()),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
        networks.insert(
            serde_yaml::Value::String(network_name),
            serde_yaml::Value::Mapping(net_def),
        );
        doc.as_mapping_mut().unwrap().insert(
            serde_yaml::Value::String("networks".to_string()),
            serde_yaml::Value::Mapping(networks),
        );
    }

    // Re-borrow services after potential doc mutation
    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .unwrap();
    services.insert(
        serde_yaml::Value::String("ts-sidecar".to_string()),
        serde_yaml::Value::Mapping(sidecar),
    );

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

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
}

/// Remove the Tailscale sidecar from a compose YAML.
pub fn remove_tailscale_sidecar(compose_yaml: &str) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("No 'services' key in compose YAML".to_string()))?;

    // Check if sidecar exists
    let sidecar_key = serde_yaml::Value::String("ts-sidecar".to_string());
    let had_sidecar = services.contains_key(&sidecar_key);

    if !had_sidecar {
        return serde_yaml::to_string(&doc)
            .map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")));
    }

    // Get sidecar's ports (to move back to main app if it was in sidecar mode)
    let sidecar_ports = services
        .get(&sidecar_key)
        .and_then(|s| s.get("ports"))
        .cloned();

    // Remove sidecar entry
    services.remove(&sidecar_key);

    // Check if main app has network_mode: service:ts-sidecar
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
                // Network mode: remove the ts-net and default entries from networks
                let networks_key = serde_yaml::Value::String("networks".to_string());
                if let Some(nets) = main_svc.get_mut(&networks_key) {
                    if let Some(seq) = nets.as_sequence_mut() {
                        seq.retain(|v| {
                            v.as_str()
                                .map(|s| !s.starts_with("ts-net-") && s != "default")
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

    // Clean up top-level networks (remove ts-net-* and the explicit default we added)
    if let Some(networks) = doc.get_mut("networks").and_then(|v| v.as_mapping_mut()) {
        let ts_keys: Vec<_> = networks
            .keys()
            .filter(|k| {
                k.as_str()
                    .map(|s| s.starts_with("ts-net-") || s == "default")
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

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
}

/// Check if a sidecar container is running for a specific app.
pub async fn is_sidecar_running(instance_id: &str) -> bool {
    crate::docker::is_container_running(&sidecar_container_name(instance_id)).await
}

/// Check if the Tailscale sidecar is actively serving (BackendState == "Running").
/// Returns `false` if the container isn't running or status can't be determined.
pub async fn is_sidecar_serving(instance_id: &str) -> bool {
    let container = sidecar_container_name(instance_id);
    let output = tokio::process::Command::new("docker")
        .args(["exec", &container, "tailscale", "status", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return false,
    };

    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return false,
    };

    json.get("BackendState")
        .and_then(|v| v.as_str())
        .map(|s| s == "Running")
        .unwrap_or(false)
}

/// Wait for the sidecar's Tailscale daemon to be running, then re-apply the
/// serve config with the resolved cert domain. Best-effort — logs warnings on failure.
///
/// When `expected_hostname` is provided (e.g. after a rename), polls longer (up to 30s)
/// and verifies that `CertDomains` contains a domain starting with the expected hostname
/// before proceeding — otherwise the old hostname's cert domain would be used.
pub async fn apply_serve_config(instance_id: &str, svc_dir: &Path, expected_hostname: Option<&str>) {
    let container = sidecar_container_name(instance_id);

    let max_polls: u32 = if expected_hostname.is_some() { 30 } else { 15 };
    let expected_prefix = expected_hostname.map(|h| format!("{h}."));

    // Poll until BackendState == "Running" and (if expected) CertDomains matches
    let mut cert_domain: Option<String> = None;
    for _ in 0..max_polls {
        let output = tokio::process::Command::new("docker")
            .args(["exec", &container, "tailscale", "status", "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await;

        if let Ok(o) = output {
            if o.status.success() {
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                    if json.get("BackendState").and_then(|v| v.as_str()) == Some("Running") {
                        let domain = json
                            .get("CertDomains")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        // If we expect a specific hostname, verify it matches
                        if let Some(ref prefix) = expected_prefix {
                            if let Some(ref d) = domain {
                                if d.starts_with(prefix.as_str()) {
                                    cert_domain = domain;
                                    break;
                                }
                                // Domain exists but doesn't match yet — keep polling
                                tracing::debug!(
                                    "Sidecar {instance_id}: CertDomain {d} doesn't match expected prefix {prefix}, retrying..."
                                );
                            }
                            // No domain yet — keep polling
                        } else {
                            cert_domain = domain;
                            break;
                        }
                    }
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let Some(domain) = cert_domain else {
        tracing::warn!("Sidecar for {instance_id} did not become ready; skipping serve config");
        return;
    };

    // Read ts-serve.json and resolve ${TS_CERT_DOMAIN}
    let serve_path = svc_dir.join("ts-serve.json");
    let Ok(config) = std::fs::read_to_string(&serve_path) else {
        tracing::warn!("Cannot read ts-serve.json for {instance_id}");
        return;
    };
    let resolved = config.replace("${TS_CERT_DOMAIN}", &domain);

    // Pipe resolved config to `tailscale serve set-raw`
    let child = tokio::process::Command::new("docker")
        .args(["exec", "-i", &container, "tailscale", "serve", "set-raw"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(mut c) => {
            if let Some(mut stdin) = c.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(resolved.as_bytes()).await;
                drop(stdin);
            }
            match c.wait().await {
                Ok(status) if !status.success() => {
                    tracing::warn!("tailscale serve set-raw failed for {instance_id}");
                }
                Err(e) => {
                    tracing::warn!("tailscale serve set-raw error for {instance_id}: {e}");
                }
                _ => {}
            }
        }
        Err(e) => {
            tracing::warn!("Failed to exec tailscale serve set-raw for {instance_id}: {e}");
        }
    }
}

// ── Post-deploy hooks ───────────────────────────────────────────────────────

/// Check whether a container is ready: running + healthy (or running with no healthcheck).
async fn is_container_ready(container: &str) -> bool {
    let output = tokio::process::Command::new("docker")
        .args([
            "inspect", "--format",
            "{{.State.Running}} {{if .State.Health}}{{.State.Health.Status}}{{else}}nohc{{end}}",
            container,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;
    if let Ok(o) = output {
        let s = String::from_utf8_lossy(&o.stdout);
        let s = s.trim();
        // "true healthy" = running with passing healthcheck
        // "true nohc"    = running, no healthcheck defined
        s == "true healthy" || s == "true nohc"
    } else {
        false
    }
}

/// Wait for a container to become ready using `docker events` (event-driven, no polling).
/// Returns `true` if the container became ready within the timeout.
async fn wait_for_container_ready(container: &str, timeout: std::time::Duration) -> bool {
    // Fast path: already ready
    if is_container_ready(container).await {
        return true;
    }

    // Subscribe to Docker events for this container
    let mut child = match tokio::process::Command::new("docker")
        .args([
            "events",
            "--filter", &format!("container={container}"),
            "--filter", "type=container",
            "--format", "{{.Status}}",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to spawn docker events for {container}: {e}");
            return false;
        }
    };

    let stdout = child.stdout.take().expect("stdout piped");
    let mut lines = tokio::io::BufReader::new(stdout).lines();

    // Re-check after subscribing to close the TOCTOU gap
    if is_container_ready(container).await {
        let _ = child.kill().await;
        return true;
    }

    let result = tokio::time::timeout(timeout, async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    // health_status: healthy — container passed its healthcheck
                    if line.contains("health_status: healthy") {
                        return true;
                    }
                    // start event — container just started; check if it has no
                    // healthcheck (in which case "running" is sufficient)
                    if line == "start" && is_container_ready(container).await {
                        return true;
                    }
                }
                // Stream ended or error — container removed / docker daemon issue
                _ => return false,
            }
        }
    })
    .await;

    let _ = child.kill().await;
    result.unwrap_or(false)
}

/// Run on_tailscale_change commands from an app's definition inside its main container.
/// Replaces `${TAILSCALE_DOMAIN}` and `${SERVER_IP}` placeholders.
pub async fn run_on_tailscale_change(
    id: &str,
    def: &crate::registry::AppDefinition,
    svc_state: &crate::config::InstalledAppState,
    data_dir: &std::path::Path,
) {
    if def.metadata.on_tailscale_change.is_empty() {
        return;
    }

    let container = format!("{}{id}", crate::docker::CONTAINER_PREFIX);
    let default_hostname = format!("myground-{id}");
    let hostname = svc_state.tailscale_hostname.as_deref().unwrap_or(&default_hostname);
    let ts_cfg = crate::config::load_tailscale_config(data_dir)
        .unwrap_or(None)
        .unwrap_or_default();
    let tailscale_domain = ts_cfg.tailnet.as_ref()
        .map(|tn| format!("{hostname}.{tn}"))
        .unwrap_or_default();
    let server_ip = crate::stats::get_server_ip().unwrap_or_default();

    // Wait for the container to be running + healthy (event-driven via docker events)
    if !wait_for_container_ready(&container, std::time::Duration::from_secs(180)).await {
        tracing::warn!(
            "on_tailscale_change for {id}: container {container} not ready after 180s, skipping hooks"
        );
        return;
    }

    for cmd_template in &def.metadata.on_tailscale_change {
        let cmd = cmd_template
            .replace("${TAILSCALE_DOMAIN}", &tailscale_domain)
            .replace("${SERVER_IP}", &server_ip);

        tracing::info!("on_tailscale_change for {id}: docker exec {container} sh -c '{cmd}'");
        let result = tokio::process::Command::new("docker")
            .args(["exec", &container, "sh", "-c", &cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match result {
            Ok(o) if !o.status.success() => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                tracing::warn!("on_tailscale_change for {id} failed: {stderr}");
            }
            Err(e) => {
                tracing::warn!("on_tailscale_change for {id} exec error: {e}");
            }
            _ => {}
        }
    }
}

// ── Migration from TSDProxy ─────────────────────────────────────────────────

/// Remove old TSDProxy labels from a compose YAML string.
pub fn remove_tsdproxy_labels(compose_yaml: &str) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Io("No 'services' key in compose YAML".to_string()))?;

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

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
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

    // Remove TSDProxy labels from all installed app compose files
    let installed = config::list_installed_apps(base);
    for id in &installed {
        let compose_path = config::app_dir(base, id).join("docker-compose.yml");
        if let Ok(yaml) = std::fs::read_to_string(&compose_path) {
            if let Ok(cleaned) = remove_tsdproxy_labels(&yaml) {
                let _ = std::fs::write(&compose_path, cleaned);
            }
        }
    }

    // Remove tsdproxy directory
    let _ = std::fs::remove_dir_all(&tsdproxy_dir);

    // Start exit node with old auth key (default pihole_dns=true for migration)
    if let Err(e) = ensure_exit_node(base, old_auth_key.as_deref(), true).await {
        tracing::warn!("Failed to start exit node during migration: {e}");
    }

    tracing::info!("TSDProxy migration complete.");
}

// ── Legacy cleanup (for nuke) ───────────────────────────────────────────────

/// Regenerate ts-serve.json for all installed apps on startup.
/// Compares new config with existing and restarts sidecar containers if changed.
pub async fn regenerate_all_serve_configs(state: &AppState) {
    let ts_cfg = config::try_load_tailscale(&state.data_dir);
    if !ts_cfg.enabled {
        return;
    }

    let installed = config::list_installed_apps_with_state(&state.data_dir);
    let compose_cmd = match crate::compose::detect_command().await {
        Ok(cmd) => cmd,
        Err(_) => return,
    };

    for (id, svc_state) in &installed {
        if svc_state.tailscale_disabled {
            continue;
        }

        let def = match crate::apps::lookup_definition(id, &state.registry, &state.data_dir) {
            Ok(def) => def,
            Err(_) => continue,
        };

        let mode = &def.metadata.tailscale_mode;
        let vpn_active = crate::vpn::is_vpn_enabled(svc_state);
        let eff_mode = crate::apps::effective_tailscale_mode(mode, vpn_active);
        if eff_mode == "skip" {
            continue;
        }

        let svc_dir = config::app_dir(&state.data_dir, id);
        let compose_path = svc_dir.join("docker-compose.yml");
        let Ok(yaml) = std::fs::read_to_string(&compose_path) else {
            continue;
        };

        let toml_port = def.health.as_ref().and_then(|h| h.container_port).unwrap_or(80);
        let main_svc = extract_main_service_name(&yaml);
        let port = extract_main_service_container_port(&yaml).unwrap_or(toml_port);
        let host_net = yaml.contains("network_mode: host");
        let proxy_target = crate::apps::tailscale_proxy_target(
            id,
            port,
            eff_mode,
            vpn_active,
            main_svc.as_deref(),
            host_net,
        );

        let new_config = generate_serve_config(&proxy_target);
        let serve_path = svc_dir.join("ts-serve.json");
        let existing = std::fs::read_to_string(&serve_path).unwrap_or_default();

        if existing == new_config {
            continue;
        }

        tracing::info!("Regenerating ts-serve.json for {id}");
        if let Err(e) = write_serve_config(&svc_dir, &proxy_target) {
            tracing::warn!("Failed to write ts-serve.json for {id}: {e}");
            continue;
        }

        // Restart the sidecar so it picks up the new config
        let container = sidecar_container_name(id);
        if crate::docker::is_container_running(&container).await {
            let _ = crate::compose::run(
                &compose_cmd,
                &svc_dir,
                &["restart", "ts-sidecar"],
            )
            .await;

            // Re-apply serve config after restart so ${TS_CERT_DOMAIN} resolves correctly
            let svc_dir_clone = svc_dir.clone();
            let id_clone = id.clone();
            tokio::spawn(async move {
                apply_serve_config(&id_clone, &svc_dir_clone, None).await;
            });
        }
    }
}

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
    fn extract_tailnet_domain_from_string() {
        assert_eq!(
            extract_tailnet_domain("Connected to myhost.tail1234b.ts.net"),
            Some("tail1234b.ts.net".to_string())
        );
        assert_eq!(extract_tailnet_domain("no domain here"), None);
    }

    #[test]
    fn inject_sidecar_mode_adds_sidecar() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    container_name: myground-whoami
    ports:
      - "9000:80"
"#;
        let result = inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None, None).unwrap();
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
        let result = inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None, None).unwrap();
        // Main app should have network_mode instead of ports
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
        let result = inject_tailscale_sidecar(yaml, "pihole", 80, "network", None, None).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = doc.get("services").unwrap().get("pihole").unwrap();
        // Main app keeps its ports
        assert!(main.get("ports").is_some());
        assert!(main.get("network_mode").is_none());
        // Has networks including default (to preserve inter-service DNS)
        let nets = main.get("networks").unwrap().as_sequence().unwrap();
        let net_names: Vec<&str> = nets.iter().filter_map(|v| v.as_str()).collect();
        assert!(net_names.contains(&"default"), "should include default network");
        assert!(net_names.contains(&"ts-net-pihole"), "should include ts-net");

        // Top-level networks should have both default and ts-net
        let top_nets = doc.get("networks").unwrap().as_mapping().unwrap();
        assert!(top_nets.contains_key(&serde_yaml::Value::String("default".to_string())));
        assert!(top_nets.contains_key(&serde_yaml::Value::String("ts-net-pihole".to_string())));
    }

    #[test]
    fn inject_network_mode_multi_service_keeps_default() {
        // Multi-service app (like Beszel): main service gets both default
        // and ts-net so other services (init, agent) can still reach it by name.
        let yaml = r#"services:
  beszel:
    image: henrygd/beszel
    container_name: myground-beszel
    ports:
      - "8085:8085"
  beszel-init:
    image: alpine:latest
    depends_on:
      beszel:
        condition: service_healthy
"#;
        let result =
            inject_tailscale_sidecar(yaml, "beszel", 8085, "network", None, None).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = doc.get("services").unwrap().get("beszel").unwrap();

        // Main service should have both default and ts-net networks
        let nets = main.get("networks").unwrap().as_sequence().unwrap();
        let net_names: Vec<&str> = nets.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            net_names.contains(&"default"),
            "main service should stay on default network for inter-service DNS"
        );
        assert!(net_names.contains(&"ts-net-beszel"));

        // beszel-init should NOT have networks added (only first service is modified)
        let init = doc.get("services").unwrap().get("beszel-init").unwrap();
        assert!(
            init.get("networks").is_none(),
            "init service should not have networks"
        );
    }

    #[test]
    fn inject_network_mode_host_app() {
        // Beszel agent uses network_mode: host — should not add networks to main
        let yaml = r#"services:
  beszel:
    image: henrygd/beszel
    container_name: myground-beszel
    network_mode: host
"#;
        let result = inject_tailscale_sidecar(yaml, "beszel", 8085, "network", None, None).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let main = doc.get("services").unwrap().get("beszel").unwrap();
        // Main app should NOT have networks (it uses network_mode: host)
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
            inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", Some("tskey-auth-xxx"), None)
                .unwrap();
        // Auth key is now in a separate .env file, not inline
        assert!(!result.contains("tskey-auth-xxx"));
        assert!(result.contains("ts-sidecar.env"));
    }

    #[test]
    fn inject_with_custom_hostname() {
        let yaml = r#"services:
  whoami:
    image: traefik/whoami
    ports:
      - "9000:80"
"#;
        let result =
            inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None, Some("my-web-app"))
                .unwrap();
        // hostname must NOT be set (it creates /etc/hosts conflicts with the app container)
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let sidecar = doc.get("services").unwrap().get("ts-sidecar").unwrap();
        assert!(sidecar.get("hostname").is_none(), "sidecar must not have hostname field");
        // TS_HOSTNAME env var handles the Tailscale machine name
        let env = sidecar.get("environment").unwrap();
        assert_eq!(env.get("TS_HOSTNAME").unwrap().as_str(), Some("my-web-app"));
        // Container name stays as myground-whoami-ts (Docker identity, not Tailscale hostname)
        assert_eq!(
            sidecar.get("container_name").unwrap().as_str(),
            Some("myground-whoami-ts")
        );
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
        let injected = inject_tailscale_sidecar(yaml, "whoami", 80, "sidecar", None, None).unwrap();
        let restored = remove_tailscale_sidecar(&injected).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();
        let main = doc.get("services").unwrap().get("whoami").unwrap();
        // Should have ports back
        assert!(main.get("ports").is_some());
        // Should NOT have network_mode
        assert!(main.get("network_mode").is_none());
        // Should NOT have depends_on
        assert!(main.get("depends_on").is_none());
        // No sidecar entry
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
        let injected = inject_tailscale_sidecar(yaml, "pihole", 80, "network", None, None).unwrap();
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
        let config = generate_serve_config("http://127.0.0.1:80");
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert!(parsed.get("TCP").is_some());
        assert!(parsed.get("Web").is_some());
    }

    #[test]
    fn generate_exit_node_compose_basic() {
        let compose = generate_exit_node_compose(None, "myground");
        assert!(compose.contains(EXIT_NODE_CONTAINER));
        assert!(compose.contains("advertise-exit-node"));
        assert!(compose.contains("env_file: .env"));
        assert!(!compose.contains("dns:"));
        assert!(compose.contains("hostname: myground"));
        assert!(compose.contains("TS_SERVE_CONFIG: /config/ts-serve.json"));
        assert!(compose.contains("./ts-serve.json:/config/ts-serve.json:ro"));
    }

    #[test]
    fn generate_exit_node_compose_with_pihole_dns() {
        let compose = generate_exit_node_compose(Some("172.17.0.5"), "myground");
        assert!(compose.contains("dns:"));
        assert!(compose.contains("172.17.0.5"));
    }

    #[test]
    fn generate_exit_node_compose_custom_hostname() {
        let compose = generate_exit_node_compose(None, "my-custom-exit");
        assert!(compose.contains("hostname: my-custom-exit"));
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
