use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SystemStats {
    pub cpu_usage_percent: f32,
    pub cpu_count: usize,
    pub cpu_brand: String,
    pub ram_total_bytes: u64,
    pub ram_used_bytes: u64,
}

/// Discover the server's LAN IP by creating a UDP socket aimed at 8.8.8.8.
/// No packets are actually sent; the OS just resolves which local interface would route.
pub fn get_server_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_server_ip_returns_valid_ip() {
        let ip = get_server_ip();
        assert!(ip.is_some(), "should detect a local IP");
        let ip = ip.unwrap();
        assert!(!ip.is_empty());
        assert!(ip.parse::<std::net::IpAddr>().is_ok(), "not a valid IP: {ip}");
    }

    #[test]
    fn get_stats_returns_cpu_and_ram() {
        let stats = get_stats();
        assert!(stats.cpu_count > 0);
        assert!(!stats.cpu_brand.is_empty());
        assert!(stats.ram_total_bytes > 0);
        assert!(stats.ram_used_bytes > 0);
    }
}
