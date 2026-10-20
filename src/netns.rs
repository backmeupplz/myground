//! Rejoin app containers to a restarted network-namespace owner.
//!
//! An app that runs with `network_mode: service:ts-sidecar` (or `service:gluetun`)
//! is started by Docker with `container:<owner id>`. If the owner restarts in
//! place (same ID, e.g. via its `unless-stopped` policy or `compose restart
//! ts-sidecar`), it gets a fresh network namespace while the app keeps running
//! in the old one. The app still looks healthy, but the sidecar's proxy to
//! `127.0.0.1:<port>` gets "connection refused" and the app goes dark.
//!
//! The watchdog finds running `myground-*` containers whose namespace owner
//! started after they did, and restarts them so they join the owner's new
//! namespace.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use bollard::models::ContainerSummaryStateEnum;
use bollard::query_parameters::{
    InspectContainerOptions, ListContainersOptionsBuilder, RestartContainerOptions,
};
use bollard::Docker;
use chrono::{DateTime, Utc};

use crate::docker::CONTAINER_PREFIX;
use crate::state::AppState;

/// How often the watchdog looks for stranded containers.
const CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// How long an owner must have been running before its dependents are
/// restarted. Leaves room for compose to restart dependents on its own.
const OWNER_GRACE: Duration = Duration::from_secs(20);

/// A running container that joins another container's network namespace.
#[derive(Debug, Clone)]
pub struct Dependent {
    pub name: String,
    pub owner_id: String,
    pub started_at: Option<DateTime<Utc>>,
}

/// Spawn the network-namespace watchdog as a background task.
pub fn spawn(state: AppState) {
    let Some(docker) = state.docker.clone() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(CHECK_INTERVAL).await;
            rejoin_stranded(&docker, OWNER_GRACE).await;
        }
    });
}

/// Restart every running `myground-*` container whose namespace owner was
/// restarted after it, once the owner has been up for `grace`.
/// Returns the names of the containers that were restarted.
pub async fn rejoin_stranded(docker: &Docker, grace: Duration) -> Vec<String> {
    let opts = ListContainersOptionsBuilder::default().all(false).build();
    let containers = match docker.list_containers(Some(opts)).await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("netns watchdog: list containers failed: {e}");
            return Vec::new();
        }
    };

    let mut running: HashSet<String> = HashSet::new();
    let mut candidates: Vec<(String, String)> = Vec::new();
    for c in &containers {
        let Some(id) = c.id.clone() else { continue };
        if c.state != Some(ContainerSummaryStateEnum::RUNNING) {
            continue;
        }
        running.insert(id);

        let Some(name) = c
            .names
            .as_ref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
        else {
            continue;
        };
        if !name.starts_with(CONTAINER_PREFIX) {
            continue;
        }
        let network_mode = c.host_config.as_ref().and_then(|h| h.network_mode.as_deref());
        if let Some(owner_id) = owner_id(network_mode) {
            candidates.push((name, owner_id.to_string()));
        }
    }

    let mut dependents = Vec::new();
    let mut owner_started: HashMap<String, Option<DateTime<Utc>>> = HashMap::new();
    for (name, owner_id) in candidates {
        // An owner that is gone or stopped can't be rejoined by a restart.
        if !running.contains(&owner_id) {
            continue;
        }
        if !owner_started.contains_key(&owner_id) {
            owner_started.insert(owner_id.clone(), started_at(docker, &owner_id).await);
        }
        dependents.push(Dependent {
            started_at: started_at(docker, &name).await,
            name,
            owner_id,
        });
    }

    let stranded = find_stranded(&dependents, &owner_started, Utc::now(), grace);
    let mut restarted = Vec::new();
    for name in stranded {
        tracing::warn!(
            "{name} is stuck in a stale network namespace (its network owner restarted); restarting it"
        );
        match docker.restart_container(&name, None::<RestartContainerOptions>).await {
            Ok(()) => restarted.push(name),
            Err(e) => tracing::warn!("Failed to restart {name}: {e}"),
        }
    }
    restarted
}

/// Extract the owner container ID from a `container:<id>` network mode.
pub fn owner_id(network_mode: Option<&str>) -> Option<&str> {
    network_mode
        .and_then(|m| m.strip_prefix("container:"))
        .filter(|id| !id.is_empty())
}

/// Names of dependents whose owner started after them and has been running
/// for at least `grace`. Dependents or owners with unknown start times are skipped.
pub fn find_stranded(
    dependents: &[Dependent],
    owner_started: &HashMap<String, Option<DateTime<Utc>>>,
    now: DateTime<Utc>,
    grace: Duration,
) -> Vec<String> {
    let grace = chrono::Duration::from_std(grace).unwrap_or(chrono::Duration::zero());
    dependents
        .iter()
        .filter(|d| {
            let Some(dep_started) = d.started_at else {
                return false;
            };
            let Some(Some(owner_started)) = owner_started.get(&d.owner_id) else {
                return false;
            };
            *owner_started > dep_started && now - *owner_started >= grace
        })
        .map(|d| d.name.clone())
        .collect()
}

async fn started_at(docker: &Docker, container: &str) -> Option<DateTime<Utc>> {
    let info = docker
        .inspect_container(container, None::<InspectContainerOptions>)
        .await
        .ok()?;
    parse_started_at(info.state?.started_at.as_deref()?)
}

/// Parse Docker's RFC 3339 `StartedAt`. The zero time Docker reports for
/// never-started containers is treated as unknown.
pub fn parse_started_at(value: &str) -> Option<DateTime<Utc>> {
    let t = DateTime::parse_from_rfc3339(value).ok()?.with_timezone(&Utc);
    (t.timestamp() > 0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        parse_started_at(s).unwrap()
    }

    fn dep(name: &str, owner: &str, started: &str) -> Dependent {
        Dependent {
            name: name.to_string(),
            owner_id: owner.to_string(),
            started_at: Some(t(started)),
        }
    }

    fn owners(list: &[(&str, Option<&str>)]) -> HashMap<String, Option<DateTime<Utc>>> {
        list.iter()
            .map(|(id, s)| (id.to_string(), s.map(t)))
            .collect()
    }

    const NOW: &str = "2026-10-20T23:00:00Z";

    #[test]
    fn owner_id_parses_container_mode() {
        assert_eq!(owner_id(Some("container:abc123")), Some("abc123"));
        assert_eq!(owner_id(Some("container:")), None);
        assert_eq!(owner_id(Some("bridge")), None);
        assert_eq!(owner_id(Some("host")), None);
        assert_eq!(owner_id(None), None);
    }

    #[test]
    fn parse_started_at_handles_docker_formats() {
        assert_eq!(
            parse_started_at("2026-10-20T22:54:23.123456789Z"),
            Some(t("2026-10-20T22:54:23.123456789Z"))
        );
        assert_eq!(parse_started_at("0001-01-01T00:00:00Z"), None);
        assert_eq!(parse_started_at(""), None);
        assert_eq!(parse_started_at("garbage"), None);
    }

    #[test]
    fn owner_restarted_after_dependent_is_stranded() {
        // The 2026-10-20 incident: vaultwarden up 8 days, sidecar restarted in place.
        let deps = vec![dep("myground-vaultwarden", "side", "2026-10-12T10:00:00Z")];
        let o = owners(&[("side", Some("2026-10-20T22:54:23Z"))]);
        assert_eq!(
            find_stranded(&deps, &o, t(NOW), OWNER_GRACE),
            vec!["myground-vaultwarden".to_string()]
        );
    }

    #[test]
    fn dependent_started_after_owner_is_fine() {
        let deps = vec![dep("myground-whoami", "side", "2026-10-20T22:00:05Z")];
        let o = owners(&[("side", Some("2026-10-20T22:00:00Z"))]);
        assert!(find_stranded(&deps, &o, t(NOW), OWNER_GRACE).is_empty());
    }

    #[test]
    fn owner_within_grace_is_left_alone() {
        let deps = vec![dep("myground-whoami", "side", "2026-10-12T10:00:00Z")];
        let o = owners(&[("side", Some("2026-10-20T22:59:50Z"))]);
        assert!(find_stranded(&deps, &o, t(NOW), OWNER_GRACE).is_empty());
        // Once the grace period has passed, the dependent is restarted.
        assert_eq!(
            find_stranded(&deps, &o, t("2026-10-20T23:00:10Z"), OWNER_GRACE).len(),
            1
        );
    }

    #[test]
    fn unknown_start_times_are_skipped() {
        let mut d = dep("myground-a", "side", "2026-10-12T10:00:00Z");
        d.started_at = None;
        let deps = vec![d, dep("myground-b", "missing", "2026-10-12T10:00:00Z"), dep("myground-c", "nostart", "2026-10-12T10:00:00Z")];
        let o = owners(&[("side", Some("2026-10-20T22:00:00Z")), ("nostart", None)]);
        assert!(find_stranded(&deps, &o, t(NOW), OWNER_GRACE).is_empty());
    }

    #[test]
    fn only_stranded_dependents_are_returned() {
        let deps = vec![
            dep("myground-immich-server", "ts", "2026-10-12T10:00:00Z"),
            dep("myground-qbit", "gluetun", "2026-10-20T22:30:00Z"),
        ];
        let o = owners(&[
            ("ts", Some("2026-10-20T22:00:00Z")),
            ("gluetun", Some("2026-10-20T22:10:00Z")),
        ]);
        assert_eq!(
            find_stranded(&deps, &o, t(NOW), OWNER_GRACE),
            vec!["myground-immich-server".to_string()]
        );
    }
}
