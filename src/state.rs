use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bollard::Docker;

use crate::registry::ServiceDefinition;

/// Tracks failed login attempts for rate limiting.
#[derive(Clone, Default)]
pub struct LoginAttempts {
    /// Map from username -> (fail_count, window_start)
    entries: HashMap<String, (u32, Instant)>,
}

impl LoginAttempts {
    const MAX_ATTEMPTS: u32 = 5;
    const WINDOW_SECS: u64 = 300; // 5 minutes

    /// Returns true if the username is currently rate-limited.
    pub fn is_blocked(&self, username: &str) -> bool {
        if let Some((count, start)) = self.entries.get(username) {
            if start.elapsed().as_secs() < Self::WINDOW_SECS {
                return *count >= Self::MAX_ATTEMPTS;
            }
        }
        false
    }

    /// Record a failed login attempt. Returns true if now blocked.
    pub fn record_failure(&mut self, username: &str) -> bool {
        let entry = self.entries.entry(username.to_string()).or_insert((0, Instant::now()));
        if entry.1.elapsed().as_secs() >= Self::WINDOW_SECS {
            // Reset window
            *entry = (1, Instant::now());
        } else {
            entry.0 += 1;
        }
        entry.0 >= Self::MAX_ATTEMPTS
    }

    /// Clear attempts for a username (on successful login).
    pub fn clear(&mut self, username: &str) {
        self.entries.remove(username);
    }
}

#[derive(Clone)]
pub struct AppState {
    pub docker: Option<Docker>,
    pub registry: Arc<HashMap<String, ServiceDefinition>>,
    pub data_dir: PathBuf,
    pub sessions: Arc<RwLock<HashSet<String>>>,
    pub login_attempts: Arc<RwLock<LoginAttempts>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let docker = crate::docker::connect();
        let registry = Arc::new(crate::registry::load_registry());

        Self {
            docker,
            registry,
            data_dir,
            sessions: Arc::new(RwLock::new(HashSet::new())),
            login_attempts: Arc::new(RwLock::new(LoginAttempts::default())),
        }
    }

    /// Create AppState with an explicit Docker connection (useful for testing).
    pub fn with_docker(docker: Option<Docker>, data_dir: PathBuf) -> Self {
        let registry = Arc::new(crate::registry::load_registry());
        Self {
            docker,
            registry,
            data_dir,
            sessions: Arc::new(RwLock::new(HashSet::new())),
            login_attempts: Arc::new(RwLock::new(LoginAttempts::default())),
        }
    }
}
