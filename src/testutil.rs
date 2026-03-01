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
        backup_supported: true,
        post_install_notes: None,
        web_path: None,
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
