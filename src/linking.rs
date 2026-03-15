use std::path::Path;
use std::process::Stdio;

use crate::error::AppError;

/// The shared Docker network name used to connect linked media/download apps.
pub const SHARED_NETWORK_NAME: &str = "myground-media";

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Append a network name to a service's `networks` list.
///
/// If the service already has a `networks` sequence, the name is appended
/// (if not already present). If no `networks` key exists, a new sequence
/// `[default, <network_name>]` is inserted.
fn append_network_to_service(svc: &mut serde_yaml::Mapping, network_name: &str) {
    let networks_key = serde_yaml::Value::String("networks".to_string());
    let network_val = serde_yaml::Value::String(network_name.to_string());

    if let Some(networks) = svc.get_mut(&networks_key) {
        if let Some(seq) = networks.as_sequence_mut() {
            if !seq.iter().any(|v| v.as_str() == Some(network_name)) {
                seq.push(network_val);
            }
            return;
        }
    }

    // No existing networks list — create one with "default" + our network
    svc.insert(
        networks_key,
        serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("default".to_string()),
            network_val,
        ]),
    );
}

// ── Core YAML injection ──────────────────────────────────────────────────────

/// Inject the `myground-media` shared network into a compose YAML string.
///
/// # VPN active (`has_vpn = true`)
/// The gluetun service (service key "gluetun") acts as the network gateway for
/// the main app (which has `network_mode: service:gluetun`). We add
/// `myground-media` to gluetun's `networks` list. Because the main app shares
/// gluetun's network stack it automatically gains access.
///
/// Gluetun may already have networks attached (e.g. from a Tailscale "network"
/// mode injection), so we always **append** rather than replace.
///
/// # No VPN (`has_vpn = false`)
/// We find the first (main) service and check whether it has `network_mode`:
///
/// - `network_mode: service:X` (e.g. Tailscale "sidecar" mode) — add
///   `myground-media` to service X so the main app (which shares X's network
///   stack) can reach the shared network.
/// - No `network_mode` — add `myground-media` directly to the main service.
///   If the main service already has a `networks` list (e.g. from Tailscale
///   "network" mode) the new name is appended; otherwise a fresh list
///   `[default, myground-media]` is created.
///
/// In all cases the top-level `networks:` section is updated with:
/// ```yaml
/// myground-media:
///   external: true
///   name: myground-media
/// ```
pub fn inject_shared_network(
    compose_yaml: &str,
    _instance_id: &str,
    has_vpn: bool,
) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    {
        let services = doc
            .get_mut("services")
            .and_then(|s| s.as_mapping_mut())
            .ok_or_else(|| AppError::Io("No 'services' key in compose YAML".to_string()))?;

        let first_key = services
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| AppError::Io("No entries in compose YAML".to_string()))?;

        if has_vpn {
            // Add shared network to gluetun (which is the gateway for the main app).
            let gluetun_key = serde_yaml::Value::String("gluetun".to_string());
            if let Some(gluetun_svc) = services
                .get_mut(&gluetun_key)
                .and_then(|s| s.as_mapping_mut())
            {
                append_network_to_service(gluetun_svc, SHARED_NETWORK_NAME);
            } else {
                // Fallback: no gluetun service found — try the first service
                if let Some(main_svc) = services
                    .get_mut(&first_key)
                    .and_then(|s| s.as_mapping_mut())
                {
                    append_network_to_service(main_svc, SHARED_NETWORK_NAME);
                }
            }
        } else {
            // Determine if the main service delegates its network stack to another service.
            let network_mode = services
                .get(&first_key)
                .and_then(|s| s.as_mapping())
                .and_then(|m| m.get(&serde_yaml::Value::String("network_mode".to_string())))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            match network_mode {
                Some(nm) if nm.starts_with("service:") => {
                    // e.g. "service:ts-sidecar" — add network to the referenced service
                    let svc_name = nm.trim_start_matches("service:");
                    let svc_key = serde_yaml::Value::String(svc_name.to_string());
                    if let Some(ref_svc) =
                        services.get_mut(&svc_key).and_then(|s| s.as_mapping_mut())
                    {
                        append_network_to_service(ref_svc, SHARED_NETWORK_NAME);
                    }
                }
                _ => {
                    // No network_mode delegation — add to main service directly.
                    // If it already has a networks list (e.g. from Tailscale
                    // "network" mode injection) we append; otherwise we create one.
                    let main_svc = services
                        .get_mut(&first_key)
                        .and_then(|s| s.as_mapping_mut())
                        .ok_or_else(|| AppError::Io("Main service is not a mapping".to_string()))?;
                    append_network_to_service(main_svc, SHARED_NETWORK_NAME);
                }
            }
        }
    }

    // Add top-level networks entry for myground-media (external).
    let mut networks = doc
        .get("networks")
        .and_then(|n| n.as_mapping())
        .cloned()
        .unwrap_or_default();

    // Only insert if not already present (idempotent).
    let media_key = serde_yaml::Value::String(SHARED_NETWORK_NAME.to_string());
    if !networks.contains_key(&media_key) {
        let mut media_net = serde_yaml::Mapping::new();
        media_net.insert(
            serde_yaml::Value::String("external".to_string()),
            serde_yaml::Value::Bool(true),
        );
        media_net.insert(
            serde_yaml::Value::String("name".to_string()),
            serde_yaml::Value::String(SHARED_NETWORK_NAME.to_string()),
        );
        networks.insert(media_key, serde_yaml::Value::Mapping(media_net));
    }

    doc.as_mapping_mut().unwrap().insert(
        serde_yaml::Value::String("networks".to_string()),
        serde_yaml::Value::Mapping(networks),
    );

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
}

/// Remove the `myground-media` shared network from a compose YAML string.
///
/// Strips `myground-media` from every service's `networks` list (cleaning up
/// the key entirely when the list becomes empty) and removes the top-level
/// `networks.myground-media` entry.
pub fn remove_shared_network(compose_yaml: &str) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Io(format!("Parse compose YAML: {e}")))?;

    // Strip myground-media from every service's networks list.
    if let Some(services) = doc.get_mut("services").and_then(|s| s.as_mapping_mut()) {
        for (_, svc) in services.iter_mut() {
            if let Some(svc_map) = svc.as_mapping_mut() {
                let networks_key = serde_yaml::Value::String("networks".to_string());
                if let Some(networks) = svc_map.get_mut(&networks_key) {
                    if let Some(seq) = networks.as_sequence_mut() {
                        seq.retain(|v| v.as_str() != Some(SHARED_NETWORK_NAME));
                        if seq.is_empty() {
                            svc_map.remove(&networks_key);
                        }
                    }
                }
            }
        }
    }

    // Remove the top-level myground-media network entry.
    if let Some(networks) = doc.get_mut("networks").and_then(|n| n.as_mapping_mut()) {
        let media_key = serde_yaml::Value::String(SHARED_NETWORK_NAME.to_string());
        networks.remove(&media_key);
        // If the networks mapping is now empty, remove it entirely.
        if networks.is_empty() {
            if let Some(doc_map) = doc.as_mapping_mut() {
                doc_map.remove(&serde_yaml::Value::String("networks".to_string()));
            }
        }
    }

    serde_yaml::to_string(&doc).map_err(|e| AppError::Io(format!("Serialize compose YAML: {e}")))
}

// ── Docker network lifecycle ────────────────────────────────────────────────

/// Ensure the `myground-media` Docker network exists.
///
/// First inspects the network; if the inspect fails (network absent) it runs
/// `docker network create myground-media`. This is intentionally synchronous
/// and should be called before starting any linked app.
pub fn ensure_shared_network_exists() -> Result<(), AppError> {
    // Check whether the network already exists.
    let inspect_status = std::process::Command::new("docker")
        .args(["network", "inspect", SHARED_NETWORK_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| AppError::Io(format!("docker network inspect: {e}")))?;

    if inspect_status.success() {
        return Ok(());
    }

    // Network absent — create it.
    tracing::info!("Creating shared Docker network '{SHARED_NETWORK_NAME}'");
    let create_status = std::process::Command::new("docker")
        .args(["network", "create", SHARED_NETWORK_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| AppError::Io(format!("docker network create: {e}")))?;

    if !create_status.success() {
        return Err(AppError::Io(format!(
            "Failed to create Docker network '{SHARED_NETWORK_NAME}'"
        )));
    }

    Ok(())
}

/// Remove the `myground-media` network if no installed app still requires it.
///
/// Scans all installed app states for non-MediaServer links. If none are found
/// the network is removed with `docker network rm`. Errors from `docker network
/// rm` are swallowed (e.g. containers still attached) — callers should ensure
/// all apps are stopped first.
pub fn cleanup_shared_network_if_unused(data_dir: &Path) -> Result<(), AppError> {
    use crate::config::{self, LinkType};

    let any_linked = config::list_installed_apps_with_state(data_dir)
        .into_iter()
        .any(|(_, state)| {
            state
                .app_links
                .iter()
                .any(|link| link.link_type != LinkType::MediaServer)
        });

    if any_linked {
        tracing::debug!("Shared network '{SHARED_NETWORK_NAME}' still in use — skipping removal");
        return Ok(());
    }

    tracing::info!("No linked apps remain — removing shared network '{SHARED_NETWORK_NAME}'");
    let _ = std::process::Command::new("docker")
        .args(["network", "rm", SHARED_NETWORK_NAME])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC_COMPOSE: &str = r#"services:
  app:
    image: sonarr/sonarr:latest
    container_name: myground-sonarr
    ports:
      - "9000:8989"
"#;

    const VPN_COMPOSE: &str = r#"services:
  app:
    image: sonarr/sonarr:latest
    container_name: myground-sonarr
    network_mode: service:gluetun
    depends_on:
      - gluetun
  gluetun:
    image: qmcgaw/gluetun:latest
    container_name: myground-sonarr-vpn
    ports:
      - "9000:8989"
"#;

    const TS_SIDECAR_COMPOSE: &str = r#"services:
  app:
    image: sonarr/sonarr:latest
    container_name: myground-sonarr
    network_mode: service:ts-sidecar
    depends_on:
      - ts-sidecar
  ts-sidecar:
    image: ghcr.io/tailscale/tailscale
    container_name: myground-sonarr-ts
    ports:
      - "9000:8989"
"#;

    const TS_NETWORK_COMPOSE: &str = r#"services:
  app:
    image: sonarr/sonarr:latest
    container_name: myground-sonarr
    networks:
      - default
      - ts-net-sonarr
  ts-sidecar:
    image: ghcr.io/tailscale/tailscale
    container_name: myground-sonarr-ts
    networks:
      - ts-net-sonarr
networks:
  default: {}
  ts-net-sonarr:
    driver: bridge
"#;

    // VPN + Tailscale "network" mode combined
    const VPN_TS_NETWORK_COMPOSE: &str = r#"services:
  app:
    image: sonarr/sonarr:latest
    container_name: myground-sonarr
    network_mode: service:gluetun
    depends_on:
      - gluetun
  gluetun:
    image: qmcgaw/gluetun:latest
    container_name: myground-sonarr-vpn
    networks:
      - default
      - ts-net-sonarr
  ts-sidecar:
    image: ghcr.io/tailscale/tailscale
    container_name: myground-sonarr-ts
    networks:
      - ts-net-sonarr
networks:
  default: {}
  ts-net-sonarr:
    driver: bridge
"#;

    // ── inject tests ──────────────────────────────────────────────────────

    #[test]
    fn inject_no_vpn_no_ts_adds_network_to_main() {
        let result = inject_shared_network(BASIC_COMPOSE, "sonarr", false).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

        let networks = doc["services"]["app"]["networks"].as_sequence().unwrap();
        assert!(networks
            .iter()
            .any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));
        assert!(networks.iter().any(|v| v.as_str() == Some("default")));

        // Top-level network entry
        let media = &doc["networks"][SHARED_NETWORK_NAME];
        assert_eq!(media["external"].as_bool(), Some(true));
        assert_eq!(media["name"].as_str(), Some(SHARED_NETWORK_NAME));
    }

    #[test]
    fn inject_with_vpn_adds_network_to_gluetun() {
        let result = inject_shared_network(VPN_COMPOSE, "sonarr", true).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

        // gluetun gets the network
        let gluetun_nets = doc["services"]["gluetun"]["networks"]
            .as_sequence()
            .unwrap();
        assert!(gluetun_nets
            .iter()
            .any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));

        // Main app keeps network_mode: service:gluetun
        assert_eq!(
            doc["services"]["app"]["network_mode"].as_str(),
            Some("service:gluetun")
        );
        // Main app does NOT get networks (it uses gluetun's stack)
        assert!(doc["services"]["app"].get("networks").is_none());
    }

    #[test]
    fn inject_ts_sidecar_mode_adds_network_to_ts_sidecar() {
        let result = inject_shared_network(TS_SIDECAR_COMPOSE, "sonarr", false).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

        // ts-sidecar gets the network
        let ts_nets = doc["services"]["ts-sidecar"]["networks"]
            .as_sequence()
            .unwrap();
        assert!(ts_nets
            .iter()
            .any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));

        // Main app keeps network_mode: service:ts-sidecar
        assert_eq!(
            doc["services"]["app"]["network_mode"].as_str(),
            Some("service:ts-sidecar")
        );
    }

    #[test]
    fn inject_ts_network_mode_appends_to_existing_networks() {
        let result = inject_shared_network(TS_NETWORK_COMPOSE, "sonarr", false).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

        let app_nets = doc["services"]["app"]["networks"].as_sequence().unwrap();
        // Should still have existing networks
        assert!(app_nets.iter().any(|v| v.as_str() == Some("default")));
        assert!(app_nets.iter().any(|v| v.as_str() == Some("ts-net-sonarr")));
        // And the new one appended
        assert!(app_nets
            .iter()
            .any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));
    }

    #[test]
    fn inject_vpn_plus_ts_appends_to_gluetun_existing_networks() {
        let result = inject_shared_network(VPN_TS_NETWORK_COMPOSE, "sonarr", true).unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();

        let gluetun_nets = doc["services"]["gluetun"]["networks"]
            .as_sequence()
            .unwrap();
        // Keep existing TS network
        assert!(gluetun_nets
            .iter()
            .any(|v| v.as_str() == Some("ts-net-sonarr")));
        // Append shared media network
        assert!(gluetun_nets
            .iter()
            .any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));

        // Top-level networks section has both ts-net-sonarr AND myground-media
        assert!(doc["networks"].get("ts-net-sonarr").is_some());
        let media = &doc["networks"][SHARED_NETWORK_NAME];
        assert_eq!(media["external"].as_bool(), Some(true));
    }

    #[test]
    fn inject_is_idempotent() {
        let once = inject_shared_network(BASIC_COMPOSE, "sonarr", false).unwrap();
        let twice = inject_shared_network(&once, "sonarr", false).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&twice).unwrap();
        let networks = doc["services"]["app"]["networks"].as_sequence().unwrap();

        // Should not contain duplicates
        let media_count = networks
            .iter()
            .filter(|v| v.as_str() == Some(SHARED_NETWORK_NAME))
            .count();
        assert_eq!(media_count, 1);
    }

    // ── remove tests ──────────────────────────────────────────────────────

    #[test]
    fn remove_strips_network_from_services_and_top_level() {
        let injected = inject_shared_network(BASIC_COMPOSE, "sonarr", false).unwrap();
        let restored = remove_shared_network(&injected).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();

        // myground-media should be gone from the main service
        if let Some(nets) = doc["services"]["app"].get("networks") {
            if let Some(seq) = nets.as_sequence() {
                assert!(!seq.iter().any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));
            }
        }

        // Top-level networks entry should be removed
        if let Some(networks) = doc.get("networks") {
            assert!(networks.get(SHARED_NETWORK_NAME).is_none());
        }
    }

    #[test]
    fn remove_vpn_compose_strips_from_gluetun() {
        let injected = inject_shared_network(VPN_COMPOSE, "sonarr", true).unwrap();
        let restored = remove_shared_network(&injected).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();
        if let Some(nets) = doc["services"]["gluetun"].get("networks") {
            if let Some(seq) = nets.as_sequence() {
                assert!(!seq.iter().any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));
            }
        }
    }

    #[test]
    fn remove_noop_when_not_present() {
        // Removing from a compose that never had it should not error.
        let result = remove_shared_network(BASIC_COMPOSE).unwrap();
        assert!(!result.contains(SHARED_NETWORK_NAME));
    }

    #[test]
    fn remove_keeps_other_networks() {
        let injected = inject_shared_network(TS_NETWORK_COMPOSE, "sonarr", false).unwrap();
        let restored = remove_shared_network(&injected).unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&restored).unwrap();

        let app_nets = doc["services"]["app"]["networks"].as_sequence().unwrap();
        // ts-net-sonarr should survive
        assert!(app_nets.iter().any(|v| v.as_str() == Some("ts-net-sonarr")));
        // myground-media should be gone
        assert!(!app_nets
            .iter()
            .any(|v| v.as_str() == Some(SHARED_NETWORK_NAME)));

        // ts-net-sonarr top-level entry survives
        assert!(doc["networks"].get("ts-net-sonarr").is_some());
        // myground-media entry is gone
        assert!(doc["networks"].get(SHARED_NETWORK_NAME).is_none());
    }
}
