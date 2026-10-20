//! End-to-end test for the network-namespace watchdog against a real Docker daemon.
//!
//! Requires a running Docker daemon and is skipped by default.
//! Run with: `cargo test --test netns_e2e -- --ignored`

use std::process::Command;
use std::time::Duration;

const OWNER: &str = "myground-netns-e2e-ts";
const APP: &str = "myground-netns-e2e";

fn docker(args: &[&str]) -> (bool, String) {
    let out = Command::new("docker").args(args).output().expect("run docker");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// Fetch the app's HTTP port from inside the owner's network namespace.
fn app_reachable_from_owner() -> bool {
    docker(&["exec", OWNER, "wget", "-q", "-O-", "-T", "2", "http://127.0.0.1:8080/ok"]).0
}

fn cleanup() {
    let _ = docker(&["rm", "-f", APP, OWNER]);
}

#[tokio::test]
#[ignore]
async fn restarted_owner_gets_its_app_back() {
    cleanup();
    let docker_client = myground::docker::connect().expect("docker");

    assert!(docker(&["run", "-d", "--name", OWNER, "busybox", "sh", "-c",
        "trap 'exit 0' TERM; while true; do sleep 1; done"]).0);
    assert!(docker(&["run", "-d", "--name", APP, "--network", &format!("container:{OWNER}"),
        "busybox", "sh", "-c", "mkdir -p /www && echo ok > /www/ok && httpd -f -p 8080 -h /www"]).0);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(app_reachable_from_owner(), "app should be reachable before the owner restarts");

    // Nothing is stranded yet.
    let restarted = myground::netns::rejoin_stranded(&docker_client, Duration::ZERO).await;
    assert!(!restarted.contains(&APP.to_string()), "restarted {restarted:?}");

    // Restart the owner in place: the app is left in the old namespace.
    assert!(docker(&["restart", OWNER]).0);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!app_reachable_from_owner(), "app should be stranded after the owner restarts");

    // Within the grace period the watchdog leaves it alone.
    let restarted = myground::netns::rejoin_stranded(&docker_client, Duration::from_secs(3600)).await;
    assert!(!restarted.contains(&APP.to_string()), "restarted {restarted:?}");

    let restarted = myground::netns::rejoin_stranded(&docker_client, Duration::ZERO).await;
    assert!(restarted.contains(&APP.to_string()), "restarted {restarted:?}");
    tokio::time::sleep(Duration::from_secs(1)).await;
    let ok = app_reachable_from_owner();

    // A second pass finds nothing to do.
    let again = myground::netns::rejoin_stranded(&docker_client, Duration::ZERO).await;
    cleanup();
    assert!(ok, "app should be reachable again after the watchdog restarts it");
    assert!(!again.contains(&APP.to_string()), "restarted twice: {again:?}");
}
