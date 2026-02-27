use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bollard::Docker;

use crate::registry::ServiceDefinition;

#[derive(Clone)]
pub struct AppState {
    pub docker: Option<Docker>,
    pub registry: Arc<HashMap<String, ServiceDefinition>>,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let docker = crate::docker::connect();
        let registry = Arc::new(crate::registry::load_registry());

        Self {
            docker,
            registry,
            data_dir,
        }
    }

    /// Create AppState with an explicit Docker connection (useful for testing).
    pub fn with_docker(docker: Option<Docker>, data_dir: PathBuf) -> Self {
        let registry = Arc::new(crate::registry::load_registry());
        Self {
            docker,
            registry,
            data_dir,
        }
    }
}
