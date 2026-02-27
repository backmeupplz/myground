use std::collections::HashMap;

use crate::registry::{ServiceDefinition, ServiceMetadata, StorageVolume};

pub fn dummy_metadata(id: &str) -> ServiceMetadata {
    ServiceMetadata {
        id: id.to_string(),
        name: id.to_string(),
        description: "test".to_string(),
        icon: "test".to_string(),
        website: "https://test.com".to_string(),
        category: "test".to_string(),
        multi_instance: false,
    }
}

pub fn dummy_service_def(
    id: &str,
    compose_template: &str,
    defaults: HashMap<String, String>,
    storage: Vec<StorageVolume>,
) -> ServiceDefinition {
    ServiceDefinition {
        metadata: dummy_metadata(id),
        compose_template: compose_template.to_string(),
        defaults,
        health: None,
        storage,
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

#[allow(dead_code)]
pub fn dummy_storage_volume_with_db_dump() -> StorageVolume {
    use crate::registry::DbDumpConfig;
    StorageVolume {
        name: "db_data".to_string(),
        container_path: "/var/lib/postgresql/data".to_string(),
        description: "Database".to_string(),
        db_dump: Some(DbDumpConfig {
            container: "myground-test-db".to_string(),
            command: "pg_dumpall -U postgres".to_string(),
            dump_file: "test_dump.sql".to_string(),
        }),
    }
}
