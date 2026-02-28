use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SystemStats {
    pub cpu_usage_percent: f32,
    pub cpu_count: usize,
    pub cpu_brand: String,
    pub ram_total_bytes: u64,
    pub ram_used_bytes: u64,
    pub gpus: Vec<GpuInfo>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GpuInfo {
    pub name: String,
    pub driver: String,
    pub memory_total_mb: Option<u64>,
    pub memory_used_mb: Option<u64>,
    pub utilization_percent: Option<u32>,
    pub temperature_celsius: Option<u32>,
}

/// Collect CPU, RAM, and GPU stats.
pub fn get_stats() -> SystemStats {
    let mut sys = sysinfo::System::new();

    // Two refreshes with a short sleep for meaningful CPU usage
    sys.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpus = sys.cpus();
    let cpu_usage = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
    };
    let cpu_brand = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();

    SystemStats {
        cpu_usage_percent: cpu_usage,
        cpu_count: cpus.len(),
        cpu_brand,
        ram_total_bytes: sys.total_memory(),
        ram_used_bytes: sys.used_memory(),
        gpus: query_gpus(),
    }
}

/// Query NVIDIA GPUs via `nvidia-smi`. Returns empty vec if unavailable.
fn query_gpus() -> Vec<GpuInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,memory.total,memory.used,utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(", ").collect();
            if parts.len() < 6 {
                return None;
            }
            Some(GpuInfo {
                name: parts[0].trim().to_string(),
                driver: parts[1].trim().to_string(),
                memory_total_mb: parts[2].trim().parse().ok(),
                memory_used_mb: parts[3].trim().parse().ok(),
                utilization_percent: parts[4].trim().parse().ok(),
                temperature_celsius: parts[5].trim().parse().ok(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_stats_returns_cpu_and_ram() {
        let stats = get_stats();
        assert!(stats.cpu_count > 0);
        assert!(!stats.cpu_brand.is_empty());
        assert!(stats.ram_total_bytes > 0);
        assert!(stats.ram_used_bytes > 0);
    }

    #[test]
    fn gpu_query_graceful_when_unavailable() {
        // nvidia-smi may or may not be available; just verify no panic
        let gpus = query_gpus();
        // gpus may be empty or populated depending on the system
        let _ = gpus;
    }
}
