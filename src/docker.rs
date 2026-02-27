use std::collections::HashMap;

use bollard::query_parameters::ListContainersOptionsBuilder;
use bollard::Docker;
use serde::Serialize;
use utoipa::ToSchema;

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

/// Extract the service ID from a Docker container name.
/// Returns None if the container doesn't belong to MyGround.
///
/// Examples:
///   "/myground-whoami" -> Some("whoami")
///   "/myground-immich-server" -> Some("immich")
///   "/some-other-container" -> None
fn parse_service_id(container_name: &str) -> Option<&str> {
    let name = container_name.trim_start_matches('/');
    let after_prefix = name.strip_prefix("myground-")?;
    Some(after_prefix.split('-').next().unwrap_or(after_prefix))
}

/// Get statuses for all containers with the `myground-` prefix.
pub async fn get_container_statuses(
    docker: &Option<Docker>,
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
            if let Some(service_id) = parse_service_id(name) {
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
                    .entry(service_id.to_string())
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
        let statuses = get_container_statuses(&None).await;
        assert!(statuses.is_empty());
    }

    #[test]
    fn parse_simple_service_name() {
        assert_eq!(parse_service_id("/myground-whoami"), Some("whoami"));
    }

    #[test]
    fn parse_compound_service_name() {
        // "myground-immich-server" -> service id is "immich"
        assert_eq!(parse_service_id("/myground-immich-server"), Some("immich"));
        assert_eq!(parse_service_id("/myground-immich-machine-learning"), Some("immich"));
    }

    #[test]
    fn parse_ignores_non_myground_containers() {
        assert_eq!(parse_service_id("/postgres"), None);
        assert_eq!(parse_service_id("/some-random-container"), None);
        assert_eq!(parse_service_id("/myapp-whoami"), None);
    }

    #[test]
    fn parse_handles_no_leading_slash() {
        assert_eq!(parse_service_id("myground-whoami"), Some("whoami"));
    }

    #[test]
    fn parse_handles_empty_string() {
        assert_eq!(parse_service_id(""), None);
        assert_eq!(parse_service_id("/"), None);
    }

    #[test]
    fn parse_handles_prefix_only() {
        // "myground-" with nothing after should return empty string
        // split('-').next() on "" returns Some("")
        assert_eq!(parse_service_id("/myground-"), Some(""));
    }
}
