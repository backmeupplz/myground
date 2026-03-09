//! End-to-end backup & restore tests that exercise the real Docker + restic pipeline.
//!
//! These tests require a running Docker daemon and are skipped by default.
//! Run with: `cargo test --test backup_e2e -- --ignored`

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use myground::backup;
use myground::config::{BackupConfig, BackupJob, GlobalConfig, InstalledAppState};
use myground::state::BackupJobProgress;

/// Build a BackupConfig pointing at a local temp repo.
fn test_backup_config(repo_dir: &std::path::Path) -> BackupConfig {
    BackupConfig {
        repository: Some(repo_dir.to_string_lossy().to_string()),
        password: Some("test-password-123".to_string()),
        ..Default::default()
    }
}

/// Create a temp dir under `target/` (not `/tmp`) so it passes `validate_storage_path`.
fn safe_tempdir() -> tempfile::TempDir {
    let target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("e2e-tmp");
    std::fs::create_dir_all(&target).unwrap();
    tempfile::tempdir_in(&target).unwrap()
}

/// Create a temp directory with some test files.
fn create_test_files(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("hello.txt"), "Hello, world!").unwrap();
    std::fs::write(dir.join("data.bin"), vec![0u8, 1, 2, 3, 4, 5, 6, 7]).unwrap();
    std::fs::create_dir_all(dir.join("subdir")).unwrap();
    std::fs::write(dir.join("subdir/nested.txt"), "nested content").unwrap();
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn docker_available() {
    let output = tokio::process::Command::new("docker")
        .args(["info"])
        .output()
        .await
        .expect("docker binary not found");
    assert!(output.status.success(), "docker info failed — is the Docker daemon running?");
}

#[tokio::test]
#[ignore]
async fn pull_restic_image() {
    backup::ensure_restic_image().await.expect("Failed to pull restic image");
}

#[tokio::test]
#[ignore]
async fn init_local_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let config = test_backup_config(&repo_dir);

    let result = backup::init_repo(&config).await.expect("init_repo failed");
    assert!(!result.is_empty(), "init_repo should return non-empty output");

    // Repo directory should exist and contain restic metadata
    assert!(repo_dir.exists(), "repo directory should exist after init");
}

#[tokio::test]
#[ignore]
async fn init_repo_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let config = test_backup_config(&repo_dir);

    backup::init_repo(&config).await.expect("first init_repo failed");
    let result = backup::init_repo(&config).await.expect("second init_repo failed");
    assert!(
        result.contains("already initialized"),
        "second init should say already initialized, got: {result}"
    );
}

#[tokio::test]
#[ignore]
async fn backup_and_list_snapshots() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let data_dir = tmp.path().join("data");
    let config = test_backup_config(&repo_dir);

    create_test_files(&data_dir);
    backup::init_repo(&config).await.unwrap();

    // Back up via run_restic
    let mounts = vec![(data_dir.to_string_lossy().to_string(), "/data:ro".to_string())];
    let output = backup::run_restic(
        &["backup", "/data", "--json"],
        &config,
        &mounts,
    )
    .await
    .expect("backup failed");
    assert!(output.contains("summary"), "backup output should contain summary JSON: {output}");

    // List snapshots
    let snapshots = backup::list_snapshots(&config).await.expect("list_snapshots failed");
    assert_eq!(snapshots.len(), 1, "should have exactly 1 snapshot");
}

#[tokio::test]
#[ignore]
async fn backup_and_restore_files() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let data_dir = tmp.path().join("data");
    let restore_dir = tmp.path().join("restored");
    let config = test_backup_config(&repo_dir);

    create_test_files(&data_dir);
    backup::init_repo(&config).await.unwrap();

    // Backup
    let mounts = vec![(data_dir.to_string_lossy().to_string(), "/data:ro".to_string())];
    backup::run_restic(&["backup", "/data", "--json"], &config, &mounts)
        .await
        .unwrap();

    // Get snapshot ID
    let snapshots = backup::list_snapshots(&config).await.unwrap();
    assert_eq!(snapshots.len(), 1);
    let snapshot_id = &snapshots[0].id;

    // Restore
    let restore_str = restore_dir.to_string_lossy().to_string();
    backup::restore_snapshot(&restore_str, snapshot_id, &config)
        .await
        .expect("restore_snapshot failed");

    // Verify restored contents (restic restores under the original mount path /data)
    let restored_hello = restore_dir.join("data/hello.txt");
    assert!(restored_hello.exists(), "restored hello.txt should exist at {}", restored_hello.display());
    assert_eq!(
        std::fs::read_to_string(&restored_hello).unwrap(),
        "Hello, world!"
    );

    let restored_nested = restore_dir.join("data/subdir/nested.txt");
    assert!(restored_nested.exists(), "restored nested.txt should exist");
    assert_eq!(
        std::fs::read_to_string(&restored_nested).unwrap(),
        "nested content"
    );

    let restored_bin = restore_dir.join("data/data.bin");
    assert_eq!(
        std::fs::read(&restored_bin).unwrap(),
        vec![0u8, 1, 2, 3, 4, 5, 6, 7]
    );
}

#[tokio::test]
#[ignore]
async fn backup_multiple_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let data_dir = tmp.path().join("data");
    let config = test_backup_config(&repo_dir);

    create_test_files(&data_dir);
    backup::init_repo(&config).await.unwrap();

    let mounts = vec![(data_dir.to_string_lossy().to_string(), "/data:ro".to_string())];

    // First backup with tag "app/data"
    backup::run_restic(
        &["backup", "/data", "--tag", "app/data", "--json"],
        &config,
        &mounts,
    )
    .await
    .unwrap();

    // Second backup with tag "app/config"
    backup::run_restic(
        &["backup", "/data", "--tag", "app/config", "--json"],
        &config,
        &mounts,
    )
    .await
    .unwrap();

    let snapshots = backup::list_snapshots(&config).await.unwrap();
    assert_eq!(snapshots.len(), 2, "should have 2 snapshots");

    let tags: Vec<&str> = snapshots.iter().flat_map(|s| s.tags.iter().map(|t| t.as_str())).collect();
    assert!(tags.contains(&"app/data"), "should have tag app/data");
    assert!(tags.contains(&"app/config"), "should have tag app/config");
}

#[tokio::test]
#[ignore]
async fn list_snapshot_files() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let data_dir = tmp.path().join("data");
    let config = test_backup_config(&repo_dir);

    create_test_files(&data_dir);
    backup::init_repo(&config).await.unwrap();

    let mounts = vec![(data_dir.to_string_lossy().to_string(), "/data:ro".to_string())];
    backup::run_restic(&["backup", "/data", "--json"], &config, &mounts)
        .await
        .unwrap();

    let snapshots = backup::list_snapshots(&config).await.unwrap();
    let snapshot_id = &snapshots[0].id;

    let files = backup::list_snapshot_files(snapshot_id, None, &config)
        .await
        .expect("list_snapshot_files failed");

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.contains("hello.txt")),
        "should list hello.txt, got: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.contains("nested.txt")),
        "should list nested.txt, got: {paths:?}"
    );
}

#[tokio::test]
#[ignore]
async fn forget_snapshot_removes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("repo");
    let data_dir = tmp.path().join("data");
    let config = test_backup_config(&repo_dir);

    create_test_files(&data_dir);
    backup::init_repo(&config).await.unwrap();

    let mounts = vec![(data_dir.to_string_lossy().to_string(), "/data:ro".to_string())];
    backup::run_restic(&["backup", "/data", "--json"], &config, &mounts)
        .await
        .unwrap();

    let snapshots = backup::list_snapshots(&config).await.unwrap();
    assert_eq!(snapshots.len(), 1);
    let snapshot_id = snapshots[0].id.clone();

    // Forget the snapshot
    backup::forget_snapshot(&snapshot_id, &config)
        .await
        .expect("forget_snapshot failed");

    let remaining = backup::list_snapshots(&config).await.unwrap();
    assert!(remaining.is_empty(), "snapshot list should be empty after forget");
}

#[tokio::test]
#[ignore]
async fn verify_repo_reports_ok() {
    let tmp = safe_tempdir();
    let repo_dir = tmp.path().join("repo");
    let config = test_backup_config(&repo_dir);

    backup::init_repo(&config).await.unwrap();

    let result = backup::verify_repo(&config).await;
    assert!(result.ok, "verify_repo should report ok for a valid repo");
    assert_eq!(result.snapshot_count, Some(0));
    assert!(result.error.is_none());
}

#[tokio::test]
#[ignore]
async fn verify_repo_wrong_password() {
    let tmp = safe_tempdir();
    let repo_dir = tmp.path().join("repo");
    let config = test_backup_config(&repo_dir);

    backup::init_repo(&config).await.unwrap();

    let bad_config = BackupConfig {
        password: Some("wrong-password".to_string()),
        ..config
    };

    let result = backup::verify_repo(&bad_config).await;
    assert!(!result.ok, "verify_repo should fail with wrong password");
    assert!(result.error.is_some());
}

#[tokio::test]
#[ignore]
async fn full_job_lifecycle() {
    use myground::registry::{AppDefinition, AppMetadata, StorageVolume};

    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();
    let app_id = "e2e-test-app";
    let job_id = "e2e-job";

    // Create repo dir inside temp
    let repo_dir = tmp.path().join("backups").join(app_id);

    // Set up storage volume directory with test files
    let storage_dir = base.join("apps").join(app_id).join("volumes").join("data");
    create_test_files(&storage_dir);

    // Create app definition with a single storage volume
    let def = AppDefinition {
        metadata: AppMetadata {
            id: app_id.to_string(),
            name: "E2E Test App".to_string(),
            description: "test".to_string(),
            icon: "test".to_string(),
            website: "https://test.com".to_string(),
            category: "test".to_string(),
            backup_supported: true,
            post_install_notes: None,
            web_path: None,
            tailscale_mode: "sidecar".to_string(),
            gpu_apps: Vec::new(),
            vpn_port_forward_command: None,
            on_tailscale_change: Vec::new(),
            extra_folders_base: None,
        },
        compose_template: String::new(),
        defaults: HashMap::new(),
        health: None,
        storage: vec![StorageVolume {
            name: "data".to_string(),
            container_path: "/data".to_string(),
            description: "Data".to_string(),
            db_dump: None,
        }],
        install_variables: Vec::new(),
    };

    let mut registry = HashMap::new();
    registry.insert(app_id.to_string(), def);

    // Create a backup job with an explicit repo path
    let job = BackupJob {
        id: job_id.to_string(),
        destination_type: "local".to_string(),
        repository: Some(repo_dir.to_string_lossy().to_string()),
        password: Some("e2e-test-pass".to_string()),
        ..Default::default()
    };

    // Create and save app state
    let app_state = InstalledAppState {
        installed: true,
        backup_jobs: vec![job],
        ..Default::default()
    };

    // Ensure data dir structure exists
    let apps_dir = base.join("apps").join(app_id);
    std::fs::create_dir_all(&apps_dir).unwrap();
    myground::config::save_app_state(base, app_id, &app_state).unwrap();

    // Global config — no defaults needed since job has explicit config
    let global_config = GlobalConfig::default();

    // Progress map + cancel set
    let progress_map: Arc<RwLock<HashMap<String, BackupJobProgress>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let cancel_set: Arc<RwLock<std::collections::HashSet<String>>> =
        Arc::new(RwLock::new(std::collections::HashSet::new()));

    // Run the backup job
    let results = backup::backup_job_run(
        base,
        app_id,
        job_id,
        &registry,
        &global_config,
        &progress_map,
        &cancel_set,
    )
    .await
    .expect("backup_job_run failed");

    // Verify results
    assert!(!results.is_empty(), "should have at least one BackupResult");
    assert!(
        !results[0].snapshot_id.is_empty(),
        "snapshot_id should not be empty"
    );

    // Verify persisted state
    let loaded_state = myground::config::load_app_state(base, app_id).unwrap();
    let persisted_job = loaded_state.backup_jobs.iter().find(|j| j.id == job_id).unwrap();
    assert_eq!(
        persisted_job.last_status.as_deref(),
        Some("succeeded"),
        "persisted last_status should be 'succeeded'"
    );
    assert!(
        persisted_job.last_run_at.is_some(),
        "last_run_at should be set"
    );

    // Verify snapshot exists in the repo
    let backup_config = BackupConfig {
        repository: Some(repo_dir.to_string_lossy().to_string()),
        password: Some("e2e-test-pass".to_string()),
        ..Default::default()
    };
    let snapshots = backup::list_snapshots(&backup_config).await.unwrap();
    assert_eq!(snapshots.len(), 1, "should have 1 snapshot in repo");

    // Restore and verify file contents
    let restore_dir = tmp.path().join("restored");
    let restore_str = restore_dir.to_string_lossy().to_string();
    backup::restore_snapshot(&restore_str, &snapshots[0].id, &backup_config)
        .await
        .expect("restore failed");

    // Find hello.txt in restored tree (restic restores under original paths)
    let find_hello = find_file_recursive(&restore_dir, "hello.txt");
    assert!(find_hello.is_some(), "hello.txt should exist in restored tree");
    assert_eq!(
        std::fs::read_to_string(find_hello.unwrap()).unwrap(),
        "Hello, world!"
    );
}

/// Recursively find a file by name.
fn find_file_recursive(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
                return Some(path);
            }
            if path.is_dir() {
                if let Some(found) = find_file_recursive(&path, name) {
                    return Some(found);
                }
            }
        }
    }
    None
}
