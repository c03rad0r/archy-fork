//! Primary host LAN IPv4 detection.
//!
//! `hostname -I` lists addresses in interface-creation order, so once a VPN
//! or bridge interface exists (NetBird's WireGuard tunnel, br-tollgate, …)
//! its address can sort ahead of the real NIC — a fresh-ISO node handed out
//! `https://10.44.0.1:8087` as NetBird's launch URL instead of the LAN IP.
//! The main routing table's default route names the physical uplink even when
//! a VPN is active (NetBird/Tailscale steer traffic via policy-routing rules
//! in separate tables, not by replacing the main-table default), so that is
//! the authoritative source, with `hostname -I` kept only as the last resort
//! for hosts with no default route at all.

/// The node's primary LAN IPv4, as a string.
///
/// Resolution order:
/// 1. `src`/`dev` of the main-table default route (`ip -4 route show default`)
/// 2. source address of a connected UDP socket (never transmits)
/// 3. first non-loopback IPv4 from `hostname -I` (legacy behaviour)
pub(crate) async fn primary_host_ipv4() -> Option<String> {
    if let Some(ip) = default_route_ip().await {
        return Some(ip);
    }
    if let Some(ip) = udp_route_ip() {
        return Some(ip);
    }
    hostname_i_ip().await
}

async fn default_route_ip() -> Option<String> {
    let out = tokio::process::Command::new("ip")
        .args(["-4", "route", "show", "default"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let route = String::from_utf8_lossy(&out.stdout);
    if let Some(ip) = parse_route_src(&route) {
        return Some(ip);
    }
    // No `src` hint on the route — resolve the device's global address.
    let dev = parse_route_dev(&route)?;
    let out = tokio::process::Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", &dev, "scope", "global"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_addr_inet(&String::from_utf8_lossy(&out.stdout))
}

fn parse_route_src(route: &str) -> Option<String> {
    field_after(route.lines().next()?, "src")
}

fn parse_route_dev(route: &str) -> Option<String> {
    field_after(route.lines().next()?, "dev")
}

fn field_after(line: &str, key: &str) -> Option<String> {
    let mut words = line.split_whitespace();
    while let Some(w) = words.next() {
        if w == key {
            return words.next().map(ToOwned::to_owned);
        }
    }
    None
}

fn parse_addr_inet(out: &str) -> Option<String> {
    let cidr = field_after(out.lines().next()?, "inet")?;
    Some(cidr.split('/').next().unwrap_or(&cidr).to_string())
}

/// A connected UDP socket's local address is the source IP the kernel would
/// use to reach the peer; nothing is sent. Can still land on a tunnel IP when
/// a VPN policy-routes all traffic, hence only a fallback.
fn udp_route_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() => {
            Some(v4.to_string())
        }
        _ => None,
    }
}

async fn hostname_i_ip() -> Option<String> {
    let out = tokio::process::Command::new("hostname")
        .arg("-I")
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .find(|s| !s.starts_with("127.") && s.contains('.'))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_src_wins() {
        let route = "default via 192.168.1.254 dev wlp3s0 proto dhcp src 192.0.2.12 metric 600";
        assert_eq!(parse_route_src(route).as_deref(), Some("192.0.2.12"));
    }

    #[test]
    fn route_dev_without_src() {
        let route = "default via 192.168.1.1 dev enp0s31f6 proto static";
        assert_eq!(parse_route_src(route), None);
        assert_eq!(parse_route_dev(route).as_deref(), Some("enp0s31f6"));
    }

    #[test]
    fn addr_inet_strips_prefix() {
        let out = "3: wlp3s0    inet 192.0.2.17/24 brd 192.0.2.255 scope global dynamic noprefixroute wlp3s0\\       valid_lft 85328sec preferred_lft 85328sec";
        assert_eq!(parse_addr_inet(out).as_deref(), Some("192.0.2.17"));
    }

    #[test]
    fn empty_route_table() {
        assert_eq!(parse_route_src(""), None);
        assert_eq!(parse_route_dev(""), None);
    }
}
