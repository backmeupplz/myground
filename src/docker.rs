use std::collections::HashMap;
use std::process::Stdio;

use bollard::query_parameters::ListContainersOptionsBuilder;
use bollard::Docker;
use serde::Serialize;
use utoipa::ToSchema;

/// Prefix used for all MyGround container names.
pub const CONTAINER_PREFIX: &str = "myground-";

/// Try to connect to the Docker daemon. Returns None if unavailable.
pub fn connect() -> Option<Docker> {
    let docker = Docker::connect_with_socket_defaults().ok()?;
    Some(docker)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DockerStatus {
    pub connected: bool,
    pub version: Option<String>,
    pub api_version: Option<String>,
}

/// Ping Docker and return status info.
pub async fn get_status(docker: &Option<Docker>) -> DockerStatus {
    let Some(docker) = docker else {
        return DockerStatus {
            connected: false,
            version: None,
            api_version: None,
        };
    };

    match docker.version().await {
        Ok(version) => DockerStatus {
            connected: true,
            version: version.version,
            api_version: version.api_version,
        },
        Err(_) => DockerStatus {
            connected: false,
            version: None,
            api_version: None,
        },
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContainerStatus {
    pub name: String,
    pub state: String,
    pub status: String,
}

/// Extract the service ID from a Docker container name using known installed IDs.
/// Uses longest-match to correctly map e.g. "myground-filebrowser-2-fb" → "filebrowser-2".
/// Falls back to first segment after "myground-" if no installed IDs match.
pub fn parse_service_id<'a>(container_name: &str, installed_ids: &'a [String]) -> Option<String> {
    let name = container_name.trim_start_matches('/');
    let after_prefix = name.strip_prefix(CONTAINER_PREFIX)?;

    // Try longest match first against known installed IDs
    let mut best: Option<&str> = None;
    for id in installed_ids {
        if after_prefix == id.as_str()
            || after_prefix.starts_with(&format!("{}-", id))
        {
            if best.is_none() || id.len() > best.unwrap().len() {
                best = Some(id);
            }
        }
    }

    if let Some(matched) = best {
        return Some(matched.to_string());
    }

    // Fallback: first segment
    Some(after_prefix.split('-').next().unwrap_or(after_prefix).to_string())
}

/// Check if a Docker container is running by name.
pub async fn is_container_running(name: &str) -> bool {
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", name])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

/// Get statuses for all containers with the `myground-` prefix.
pub async fn get_container_statuses(
    docker: &Option<Docker>,
    installed_ids: &[String],
) -> HashMap<String, Vec<ContainerStatus>> {
    let mut result: HashMap<String, Vec<ContainerStatus>> = HashMap::new();
    let Some(docker) = docker else {
        return result;
    };

    let opts = ListContainersOptionsBuilder::default().all(true).build();

    let containers = match docker.list_containers(Some(opts)).await {
        Ok(c) => c,
        Err(_) => return result,
    };

    for container in containers {
        let names = container.names.unwrap_or_default();
        for name in &names {
            if let Some(service_id) = parse_service_id(name, installed_ids) {
                let clean_name = name.trim_start_matches('/');
                let status = ContainerStatus {
                    name: clean_name.to_string(),
                    state: container
                        .state
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    status: container.status.clone().unwrap_or_default(),
                };
                result
                    .entry(service_id)
                    .or_default()
                    .push(status);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn connect_returns_some_or_none() {
        let _ = connect();
    }

    #[tokio::test]
    async fn get_status_with_none_returns_disconnected() {
        let status = get_status(&None).await;
        assert!(!status.connected);
        assert!(status.version.is_none());
        assert!(status.api_version.is_none());
    }

    #[tokio::test]
    async fn get_container_statuses_with_none_returns_empty() {
        let statuses = get_container_statuses(&None, &[]).await;
        assert!(statuses.is_empty());
    }

    #[test]
    fn parse_simple_service_name() {
        let installed = ids(&["whoami"]);
        assert_eq!(parse_service_id("/myground-whoami", &installed), Some("whoami".to_string()));
    }

    #[test]
    fn parse_compound_service_name() {
        let installed = ids(&["immich"]);
        assert_eq!(parse_service_id("/myground-immich-server", &installed), Some("immich".to_string()));
        assert_eq!(parse_service_id("/myground-immich-machine-learning", &installed), Some("immich".to_string()));
    }

    #[test]
    fn parse_ignores_non_myground_containers() {
        let installed = ids(&["whoami"]);
        assert_eq!(parse_service_id("/postgres", &installed), None);
        assert_eq!(parse_service_id("/some-random-container", &installed), None);
        assert_eq!(parse_service_id("/myapp-whoami", &installed), None);
    }

    #[test]
    fn parse_handles_no_leading_slash() {
        let installed = ids(&["whoami"]);
        assert_eq!(parse_service_id("myground-whoami", &installed), Some("whoami".to_string()));
    }

    #[test]
    fn parse_handles_empty_string() {
        assert_eq!(parse_service_id("", &[]), None);
        assert_eq!(parse_service_id("/", &[]), None);
    }

    #[test]
    fn parse_handles_prefix_only() {
        assert_eq!(parse_service_id("/myground-", &[]), Some("".to_string()));
    }

    #[test]
    fn parse_multi_instance_prefers_longest_match() {
        let installed = ids(&["filebrowser", "filebrowser-2"]);
        assert_eq!(
            parse_service_id("/myground-filebrowser-2-fb", &installed),
            Some("filebrowser-2".to_string())
        );
        assert_eq!(
            parse_service_id("/myground-filebrowser-fb", &installed),
            Some("filebrowser".to_string())
        );
    }

    #[test]
    fn parse_multi_instance_exact_match() {
        let installed = ids(&["filebrowser", "filebrowser-2"]);
        assert_eq!(
            parse_service_id("/myground-filebrowser-2", &installed),
            Some("filebrowser-2".to_string())
        );
    }
}
