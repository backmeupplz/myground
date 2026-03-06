use std::collections::HashMap;

use crate::config::{BackupConfig, BackupJob, GlobalConfig};
use crate::registry::{AppDefinition, AppMetadata, DbDumpConfig, StorageVolume};

pub fn dummy_metadata(id: &str) -> AppMetadata {
    AppMetadata {
        id: id.to_string(),
        name: id.to_string(),
        description: "test".to_string(),
        icon: "test".to_string(),
        website: "https://test.com".to_string(),
        category: "test".to_string(),
        backup_supported: true,
        post_install_notes: None,
        web_path: None,
        tailscale_mode: "sidecar".to_string(),
        gpu_apps: Vec::new(),
    }
}

pub fn dummy_app_def(
    id: &str,
    compose_template: &str,
    defaults: HashMap<String, String>,
    storage: Vec<StorageVolume>,
) -> AppDefinition {
    AppDefinition {
        metadata: dummy_metadata(id),
        compose_template: compose_template.to_string(),
        defaults,
        health: None,
        storage,
        install_variables: Vec::new(),
    }
}

pub fn dummy_storage_volumes() -> Vec<StorageVolume> {
    vec![
        StorageVolume {
            name: "data".to_string(),
            container_path: "/srv".to_string(),
            description: "Data".to_string(),
            db_dump: None,
        },
        StorageVolume {
            name: "config".to_string(),
            container_path: "/config".to_string(),
            description: "Config".to_string(),
            db_dump: None,
        },
    ]
}

pub fn dummy_storage_volumes_with_db() -> Vec<StorageVolume> {
    vec![
        StorageVolume {
            name: "data".to_string(),
            container_path: "/srv".to_string(),
            description: "Data".to_string(),
            db_dump: None,
        },
        StorageVolume {
            name: "db".to_string(),
            container_path: "/var/lib/postgresql".to_string(),
            description: "Database".to_string(),
            db_dump: Some(DbDumpConfig {
                container: "myground-testapp-db".to_string(),
                command: "pg_dumpall -U postgres".to_string(),
                dump_file: "db.sql".to_string(),
                restore_command: Some("psql -U postgres < /tmp/db.sql".to_string()),
                wipe_command: Some("dropdb -U postgres --if-exists appdb && createdb -U postgres appdb".to_string()),
            }),
        },
    ]
}

pub fn dummy_backup_job(id: &str) -> BackupJob {
    BackupJob {
        id: id.to_string(),
        destination_type: "local".to_string(),
        ..Default::default()
    }
}

pub fn dummy_global_config() -> GlobalConfig {
    GlobalConfig {
        default_local_destination: Some(BackupConfig {
            repository: Some("/var/backups/myground".to_string()),
            password: Some("default-local-pass".to_string()),
            ..Default::default()
        }),
        default_remote_destination: Some(BackupConfig {
            repository: Some("s3:https://s3.amazonaws.com/mybucket".to_string()),
            password: Some("default-remote-pass".to_string()),
            s3_access_key: Some("AKIADEFAULT".to_string()),
            s3_secret_key: Some("defaultsecret".to_string()),
        }),
        ..Default::default()
    }
}
