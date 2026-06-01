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

/// Whether an IPv6 address is global unicast — i.e. a real internet address,
/// not loopback (`::1`), unspecified (`::`), link-local (`fe80::/10`), or
/// unique-local (`fc00::/7`, e.g. a WireGuard/VPN ULA).
fn is_global_unicast_v6(v6: std::net::Ipv6Addr) -> bool {
    let first = v6.segments()[0];
    !v6.is_loopback()
        && !v6.is_unspecified()
        && (first & 0xffc0) != 0xfe80 // link-local fe80::/10
        && (first & 0xfe00) != 0xfc00 // unique-local fc00::/7
}

/// Check whether the host has a usable route to the IPv6 internet.
///
/// Uses the same zero-packet UDP "connect" trick as [`get_server_ip`]: the
/// kernel performs a route lookup when connecting a datagram socket but sends
/// nothing. With no global IPv6 route the connect fails fast (`ENETUNREACH`),
/// and we additionally require the kernel-chosen source address to be global
/// unicast — so a host that only has a link-local or WireGuard ULA address
/// (no real IPv6 egress) is correctly reported as `false`.
///
/// Used to decide whether Pi-hole should filter AAAA records: an exit node on a
/// host without IPv6 egress would otherwise advertise a `::/0` route it can't
/// serve, black-holing clients' IPv6 connections (Happy-Eyeballs timeouts).
pub fn host_has_ipv6_egress() -> bool {
    let socket = match std::net::UdpSocket::bind("[::]:0") {
        Ok(s) => s,
        Err(_) => return false,
    };
    // Google public DNS over IPv6 — no packets are actually sent.
    if socket.connect("[2001:4860:4860::8888]:80").is_err() {
        return false;
    }
    match socket.local_addr().map(|a| a.ip()) {
        Ok(std::net::IpAddr::V6(v6)) => is_global_unicast_v6(v6),
        _ => false,
    }
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

/// Detect which GPU types are available on this system.
/// Returns a list like `["intel", "nvidia"]`.
pub fn detect_available_gpus() -> Vec<String> {
    let mut gpus = Vec::new();

    // Intel/AMD iGPU: check for /dev/dri
    if std::path::Path::new("/dev/dri").exists() {
        gpus.push("intel".to_string());
    }

    // NVIDIA: check for nvidia-smi binary
    if std::process::Command::new("nvidia-smi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
    {
        gpus.push("nvidia".to_string());
    }

    gpus
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
    fn global_unicast_v6_classification() {
        use std::net::Ipv6Addr;
        let g = |s: &str| is_global_unicast_v6(s.parse::<Ipv6Addr>().unwrap());
        // Global unicast internet addresses
        assert!(g("2607:f8b0:400a:809::200e")); // google AAAA
        assert!(g("2001:4860:4860::8888")); // google DNS
        // Not egress: loopback, unspecified, link-local, ULA (e.g. WireGuard fd08::)
        assert!(!g("::1"));
        assert!(!g("::"));
        assert!(!g("fe80::1"));
        assert!(!g("fd08:4711::1"));
        assert!(!g("fc00::1"));
    }

    #[test]
    fn host_has_ipv6_egress_does_not_panic() {
        // Result depends on the test host's network; just ensure it's callable.
        let _ = host_has_ipv6_egress();
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
