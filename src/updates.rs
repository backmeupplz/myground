use std::path::Path;

use crate::compose;
use crate::config::{self, UpdateConfig};
use crate::error::ServiceError;

// ── Image digest tracking ──────────────────────────────────────────────────

/// Get the pinned digest for a Docker image reference.
/// Runs `docker image inspect` to extract the repo digest.
pub async fn get_image_digest(image_ref: &str) -> Result<String, ServiceError> {
    let output = tokio::process::Command::new("docker")
        .args([
            "image",
            "inspect",
            "--format",
            "{{index .RepoDigests 0}}",
            image_ref,
        ])
        .output()
        .await
        .map_err(|e| ServiceError::Compose(format!("Failed to inspect image: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ServiceError::Compose(format!(
            "docker image inspect failed: {stderr}"
        )));
    }

    let digest = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if digest.is_empty() {
        return Err(ServiceError::Compose(
            "No repo digest found for image".to_string(),
        ));
    }
    Ok(digest)
}

/// Extract the primary image reference from a compose template.
/// Finds the first `image:` field in the YAML.
pub fn extract_primary_image(compose_content: &str) -> Option<String> {
    for line in compose_content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("image:") {
            let image = rest.trim().trim_matches('"').trim_matches('\'');
            // Skip lines with unresolved ${} variables
            if !image.contains("${") && !image.is_empty() {
                return Some(image.to_string());
            }
        }
    }
    None
}

// ── Checking for service updates ───────────────────────────────────────────

/// Check if a service has a newer Docker image available.
/// Pulls the image quietly, then compares digests.
pub async fn check_service_update(
    data_dir: &Path,
    service_id: &str,
    registry: &std::collections::HashMap<String, crate::registry::ServiceDefinition>,
) -> Result<bool, ServiceError> {
    let svc_state = config::load_service_state(data_dir, service_id)?;
    if !svc_state.installed {
        return Ok(false);
    }

    // Find the compose template to get the image reference
    let def = crate::services::lookup_definition(service_id, registry, data_dir)?;
    let global_config = config::load_global_config(data_dir).unwrap_or_default();
    let storage_env = config::resolve_storage_paths(
        data_dir,
        service_id,
        def,
        &global_config,
        &svc_state,
    );
    let mut env = compose::merge_env(&def.defaults, &svc_state.env_overrides);
    for (k, v) in &storage_env {
        env.insert(k.clone(), v.clone());
    }
    if let Some(port) = svc_state.port {
        env.insert("PORT".to_string(), port.to_string());
    }
    // Inject BIND_IP so ${BIND_IP} doesn't remain unresolved
    let bind_ip = if svc_state.lan_accessible { "0.0.0.0" } else { "127.0.0.1" };
    env.insert("BIND_IP".to_string(), bind_ip.to_string());

    let compose_content = compose::generate_compose(def, &env);
    let image_ref = match extract_primary_image(&compose_content) {
        Some(img) => img,
        None => return Ok(false),
    };

    // Pull the latest version quietly
    let pull = tokio::process::Command::new("docker")
        .args(["pull", "-q", &image_ref])
        .output()
        .await
        .map_err(|e| ServiceError::Compose(format!("docker pull failed: {e}")))?;

    if !pull.status.success() {
        let stderr = String::from_utf8_lossy(&pull.stderr);
        tracing::warn!("Failed to pull {image_ref} for update check: {stderr}");
        return Ok(false);
    }

    // Get the new digest
    let new_digest = get_image_digest(&image_ref).await?;

    // Compare against stored digest
    let has_update = match &svc_state.image_digest {
        Some(old_digest) => old_digest != &new_digest,
        None => false, // No baseline to compare against
    };

    // Update service state
    let mut svc_state = config::load_service_state(data_dir, service_id)?;
    svc_state.update_available = has_update;
    svc_state.last_update_check = Some(chrono::Utc::now().to_rfc3339());
    config::save_service_state(data_dir, service_id, &svc_state)?;

    Ok(has_update)
}

// ── Checking for MyGround updates ──────────────────────────────────────────

/// Check GitHub releases for a newer version of MyGround.
pub async fn check_myground_update(data_dir: &Path) -> Result<bool, ServiceError> {
    let output = tokio::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "15",
            "-H",
            "Accept: application/vnd.github+json",
            "https://api.github.com/repos/borodutch/myground/releases/latest",
        ])
        .output()
        .await
        .map_err(|e| ServiceError::Io(format!("curl failed: {e}")))?;

    if !output.status.success() {
        return Ok(false);
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| ServiceError::Io(format!("Failed to parse GitHub response: {e}")))?;

    let tag = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v');
    if tag.is_empty() {
        return Ok(false);
    }

    let current = env!("CARGO_PKG_VERSION");
    let is_newer = semver_is_newer(tag, current);

    // Find the download URL for current architecture
    let download_url = find_download_url(&json);

    // Save to global config
    let mut global = config::load_global_config(data_dir).unwrap_or_default();
    let updates = global.updates.get_or_insert_with(UpdateConfig::default);
    updates.last_check = Some(chrono::Utc::now().to_rfc3339());
    if is_newer {
        updates.latest_myground_version = Some(tag.to_string());
        updates.latest_myground_url = download_url;
    } else {
        updates.latest_myground_version = None;
        updates.latest_myground_url = None;
    }
    config::save_global_config(data_dir, &global)?;

    Ok(is_newer)
}

/// Simple semver comparison: returns true if `candidate` > `current`.
fn semver_is_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let parts: Vec<u64> = s
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };
    parse(candidate) > parse(current)
}

/// Find the download URL for the current platform from GitHub release assets.
fn find_download_url(release: &serde_json::Value) -> Option<String> {
    let arch = std::env::consts::ARCH;
    let target_arch = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        _ => return None,
    };

    let assets = release["assets"].as_array()?;
    for asset in assets {
        let name = asset["name"].as_str().unwrap_or("");
        if name.contains(target_arch) && name.contains("linux") && !name.ends_with(".sha256") {
            return asset["browser_download_url"].as_str().map(String::from);
        }
    }
    None
}

// ── Performing service update ──────────────────────────────────────────────

/// Update a service by pulling new images and restarting.
/// Streams progress through the provided channel.
pub async fn update_service_streaming(
    data_dir: &Path,
    service_id: &str,
    tx: tokio::sync::mpsc::Sender<String>,
) -> Result<(), ServiceError> {
    let _ = tx.send("Pulling latest images...".to_string()).await;

    // Re-deploy (pull + up -d)
    compose::deploy_streaming(data_dir, service_id, tx.clone()).await?;

    // Record the new digest
    let svc_state = config::load_service_state(data_dir, service_id)?;
    let svc_dir = config::service_dir(data_dir, service_id);
    let compose_path = svc_dir.join("docker-compose.yml");
    if compose_path.exists() {
        let content = std::fs::read_to_string(&compose_path)
            .map_err(|e| ServiceError::Io(format!("Read compose: {e}")))?;
        if let Some(image_ref) = extract_primary_image(&content) {
            if let Ok(digest) = get_image_digest(&image_ref).await {
                let mut svc_state = svc_state;
                svc_state.image_digest = Some(digest);
                svc_state.update_available = false;
                svc_state.last_update_check = Some(chrono::Utc::now().to_rfc3339());
                config::save_service_state(data_dir, service_id, &svc_state)?;
            }
        }
    }

    let _ = tx.send("Update complete.".to_string()).await;
    Ok(())
}

/// Update a service without streaming (for auto-update).
pub async fn update_service(data_dir: &Path, service_id: &str) -> Result<(), ServiceError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);

    let data_dir = data_dir.to_path_buf();
    let sid = service_id.to_string();
    let task = tokio::spawn(async move {
        update_service_streaming(&data_dir, &sid, tx).await
    });

    // Drain the channel
    while rx.recv().await.is_some() {}

    task.await
        .map_err(|e| ServiceError::Compose(format!("Update task failed: {e}")))?
}

// ── Self-update ────────────────────────────────────────────────────────────

/// Compute SHA-256 hash of a file.
fn sha256_file(path: &Path) -> Result<String, ServiceError> {
    use sha2::{Digest, Sha256};
    let data = std::fs::read(path)
        .map_err(|e| ServiceError::Io(format!("Read file for hash: {e}")))?;
    let hash = Sha256::digest(&data);
    Ok(format!("{hash:x}"))
}

/// Download and install a new MyGround binary.
pub async fn self_update(download_url: &str) -> Result<(), ServiceError> {
    let current_exe = std::env::current_exe()
        .map_err(|e| ServiceError::Io(format!("Cannot determine current exe: {e}")))?;

    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("old");

    // Download the new binary
    let status = tokio::process::Command::new("curl")
        .args(["-sL", "--max-time", "120", "-o"])
        .arg(&tmp_path)
        .arg(download_url)
        .status()
        .await
        .map_err(|e| ServiceError::Io(format!("Download failed: {e}")))?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(ServiceError::Io("Download returned non-zero exit".to_string()));
    }

    // Verify SHA-256 checksum if available
    let sha_url = format!("{download_url}.sha256");
    let sha_output = tokio::process::Command::new("curl")
        .args(["-sL", "--max-time", "15", "-f"])
        .arg(&sha_url)
        .output()
        .await;

    if let Ok(output) = sha_output {
        if output.status.success() {
            let expected = String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();
            if !expected.is_empty() {
                let actual = sha256_file(&tmp_path)?;
                if actual != expected {
                    let _ = std::fs::remove_file(&tmp_path);
                    return Err(ServiceError::Io(format!(
                        "Checksum mismatch: expected {expected}, got {actual}"
                    )));
                }
                tracing::info!("Self-update checksum verified: {actual}");
            }
        } else {
            tracing::warn!("No .sha256 file available for self-update; skipping verification");
        }
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| ServiceError::Io(format!("chmod failed: {e}")))?;
    }

    // Backup current binary
    if backup_path.exists() {
        let _ = std::fs::remove_file(&backup_path);
    }
    std::fs::rename(&current_exe, &backup_path)
        .map_err(|e| ServiceError::Io(format!("Backup current binary: {e}")))?;

    // Move new binary into place
    std::fs::rename(&tmp_path, &current_exe)
        .map_err(|e| ServiceError::Io(format!("Install new binary: {e}")))?;

    // Try systemd restart, fall back to self-termination
    let restart = tokio::process::Command::new("systemctl")
        .args(["restart", "myground"])
        .status()
        .await;

    match restart {
        Ok(s) if s.success() => Ok(()),
        _ => {
            // Exit for non-systemd environments (process manager will restart)
            std::process::exit(0);
        }
    }
}

// ── Aggregate check ────────────────────────────────────────────────────────

/// Check for updates on all services and MyGround itself.
/// Returns (services_with_updates, myground_has_update).
pub async fn check_all_updates(
    data_dir: &Path,
    registry: &std::collections::HashMap<String, crate::registry::ServiceDefinition>,
) -> (usize, bool) {
    let installed = config::list_installed_services(data_dir);
    let mut updates_found = 0;

    for id in &installed {
        match check_service_update(data_dir, id, registry).await {
            Ok(true) => updates_found += 1,
            Ok(false) => {}
            Err(e) => tracing::warn!("Update check for {id} failed: {e}"),
        }
    }

    let myground_update = check_myground_update(data_dir).await.unwrap_or(false);

    (updates_found, myground_update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_newer() {
        assert!(semver_is_newer("1.1.0", "1.0.0"));
        assert!(semver_is_newer("2.0.0", "1.9.9"));
        assert!(semver_is_newer("1.0.1", "1.0.0"));
        assert!(!semver_is_newer("1.0.0", "1.0.0"));
        assert!(!semver_is_newer("0.9.0", "1.0.0"));
    }

    #[test]
    fn extract_image_basic() {
        let yaml = r#"
services:
  app:
    image: nginx:latest
    ports:
      - "80:80"
"#;
        assert_eq!(
            extract_primary_image(yaml),
            Some("nginx:latest".to_string())
        );
    }

    #[test]
    fn extract_image_with_vars_skipped() {
        let yaml = r#"
services:
  app:
    image: "${CUSTOM_IMAGE}"
  db:
    image: postgres:16
"#;
        assert_eq!(
            extract_primary_image(yaml),
            Some("postgres:16".to_string())
        );
    }

    #[test]
    fn extract_image_none() {
        let yaml = "services:\n  app:\n    build: .\n";
        assert_eq!(extract_primary_image(yaml), None);
    }

    #[test]
    fn semver_major_bump() {
        assert!(semver_is_newer("2.0.0", "1.99.99"));
    }

    #[test]
    fn semver_minor_bump() {
        assert!(semver_is_newer("1.2.0", "1.1.99"));
    }

    #[test]
    fn semver_patch_bump() {
        assert!(semver_is_newer("1.0.2", "1.0.1"));
    }

    #[test]
    fn semver_equal_is_not_newer() {
        assert!(!semver_is_newer("1.0.0", "1.0.0"));
        assert!(!semver_is_newer("0.0.0", "0.0.0"));
    }

    #[test]
    fn semver_partial_versions() {
        // Missing parts default to 0
        assert!(semver_is_newer("1.1", "1.0.0"));
        assert!(semver_is_newer("2", "1.9.9"));
        assert!(!semver_is_newer("1", "1.0.0"));
    }

    #[test]
    fn semver_empty_string() {
        assert!(!semver_is_newer("", "1.0.0"));
        assert!(semver_is_newer("1.0.0", ""));
    }

    #[test]
    fn extract_image_quoted_single() {
        let yaml = "services:\n  app:\n    image: 'redis:7'\n";
        assert_eq!(
            extract_primary_image(yaml),
            Some("redis:7".to_string())
        );
    }

    #[test]
    fn extract_image_quoted_double() {
        let yaml = "services:\n  app:\n    image: \"memcached:latest\"\n";
        assert_eq!(
            extract_primary_image(yaml),
            Some("memcached:latest".to_string())
        );
    }

    #[test]
    fn extract_image_skips_all_vars() {
        let yaml = "services:\n  a:\n    image: ${IMG}\n  b:\n    image: ${OTHER}\n";
        assert_eq!(extract_primary_image(yaml), None);
    }

    #[test]
    fn extract_image_empty_value() {
        let yaml = "services:\n  app:\n    image: \n    build: .\n";
        // Empty image line should be skipped
        assert_eq!(extract_primary_image(yaml), None);
    }

    #[test]
    fn find_download_url_with_matching_asset() {
        let arch = std::env::consts::ARCH;
        let target = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return, // Skip test on unsupported architectures
        };
        let json = serde_json::json!({
            "tag_name": "v1.2.3",
            "assets": [
                {
                    "name": format!("myground-{target}-linux"),
                    "browser_download_url": format!("https://example.com/myground-{target}-linux")
                },
                {
                    "name": format!("myground-{target}-linux.sha256"),
                    "browser_download_url": format!("https://example.com/myground-{target}-linux.sha256")
                }
            ]
        });
        let url = find_download_url(&json);
        assert_eq!(
            url,
            Some(format!("https://example.com/myground-{target}-linux"))
        );
    }

    #[test]
    fn find_download_url_no_matching_asset() {
        let json = serde_json::json!({
            "tag_name": "v1.0.0",
            "assets": [
                {
                    "name": "myground-mips-linux",
                    "browser_download_url": "https://example.com/myground-mips-linux"
                }
            ]
        });
        // Will be None if the current arch isn't mips
        let url = find_download_url(&json);
        // On x86_64/aarch64 this should be None
        if std::env::consts::ARCH != "mips" {
            assert!(url.is_none());
        }
    }

    #[test]
    fn find_download_url_empty_assets() {
        let json = serde_json::json!({ "tag_name": "v1.0.0", "assets": [] });
        assert!(find_download_url(&json).is_none());
    }

    #[test]
    fn find_download_url_no_assets_key() {
        let json = serde_json::json!({ "tag_name": "v1.0.0" });
        assert!(find_download_url(&json).is_none());
    }

    #[test]
    fn find_download_url_skips_sha256() {
        let arch = std::env::consts::ARCH;
        let target = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return,
        };
        let json = serde_json::json!({
            "assets": [
                {
                    "name": format!("myground-{target}-linux.sha256"),
                    "browser_download_url": "https://example.com/sha256"
                }
            ]
        });
        // Only .sha256 files — should return None
        assert!(find_download_url(&json).is_none());
    }
}
