use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub fs_type: String,
    pub is_removable: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SmartHealth {
    pub device: String,
    pub healthy: bool,
    pub temperature_celsius: Option<i64>,
    pub power_on_hours: Option<u64>,
    pub raw_output: String,
}

/// List all mounted disks with space info.
pub fn list_disks() -> Vec<DiskInfo> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|d| {
            let total = d.total_space();
            let available = d.available_space();
            DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                total_bytes: total,
                available_bytes: available,
                used_bytes: total.saturating_sub(available),
                fs_type: d.file_system().to_string_lossy().to_string(),
                is_removable: d.is_removable(),
            }
        })
        .collect()
}

/// Find which disk a given path resides on (longest mount point prefix match).
pub fn disk_usage_for_path(path: &str) -> Option<DiskInfo> {
    let disks = list_disks();
    disks
        .into_iter()
        .filter(|d| path.starts_with(&d.mount_point))
        .max_by_key(|d| d.mount_point.len())
}

/// Query SMART health for a device via `smartctl -j -H`.
/// Returns None if smartctl is unavailable or fails.
pub fn smart_health(device: &str) -> Option<SmartHealth> {
    let output = std::process::Command::new("smartctl")
        .args(["-j", "-H", "-A", device])
        .output()
        .ok()?;

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;

    let healthy = json["smart_status"]["passed"].as_bool().unwrap_or(false);

    let temperature = json["temperature"]["current"].as_i64();

    let power_on_hours = json["power_on_time"]["hours"].as_u64();

    Some(SmartHealth {
        device: device.to_string(),
        healthy,
        temperature_celsius: temperature,
        power_on_hours,
        raw_output: raw,
    })
}

/// Get SMART health for all disks. Returns empty vec if smartctl is unavailable.
pub fn smart_health_all() -> Vec<SmartHealth> {
    // Try to discover devices via smartctl --scan
    let output = std::process::Command::new("smartctl")
        .args(["--scan", "-j"])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    let raw = String::from_utf8_lossy(&output.stdout);
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };

    let Some(devices) = json["devices"].as_array() else {
        return Vec::new();
    };

    devices
        .iter()
        .filter_map(|d| {
            let name = d["name"].as_str()?;
            smart_health(name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_disks_returns_non_empty() {
        let disks = list_disks();
        assert!(!disks.is_empty(), "Expected at least one disk");
    }

    #[test]
    fn disk_usage_for_root_finds_disk() {
        let disk = disk_usage_for_path("/");
        assert!(disk.is_some(), "Expected to find disk for /");
        let disk = disk.unwrap();
        assert_eq!(disk.mount_point, "/");
        assert!(disk.total_bytes > 0);
    }

    #[test]
    fn disk_usage_for_nonexistent_returns_root() {
        // A path under / should still match the root disk
        let disk = disk_usage_for_path("/some/nonexistent/path");
        assert!(disk.is_some());
    }

    #[test]
    fn smart_health_graceful_when_unavailable() {
        // smartctl may or may not be available; just verify no panic
        let _ = smart_health_all();
    }
}
