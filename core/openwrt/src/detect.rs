use anyhow::Result;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;
use tracing::{debug, info};

use crate::Router;

const SSH_PORT: u16 = 22;
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Scan a CIDR subnet and return IP addresses of OpenWrt routers.
///
/// Probes TCP/22, then verifies /etc/openwrt_release over SSH.
/// `ssh_user` and `ssh_password` are used for the verification probe only.
pub async fn scan_subnet(
    subnet_base: [u8; 4],
    prefix_len: u8,
    ssh_user: &str,
    ssh_password: &str,
) -> Vec<IpAddr> {
    let host_count = host_count_for_prefix(prefix_len);
    let base_u32 = u32::from_be_bytes(subnet_base);
    let mask = !((1u32 << (32 - prefix_len)) - 1);
    let network = base_u32 & mask;

    let mut candidates = Vec::new();
    for i in 1..host_count {
        let ip_u32 = network + i;
        let ip = IpAddr::V4(std::net::Ipv4Addr::from(ip_u32));
        if tcp_reachable(ip, SSH_PORT) {
            candidates.push(ip);
        }
    }

    info!(
        "{} hosts with TCP/22 open in /{}",
        candidates.len(),
        prefix_len
    );

    let mut routers = Vec::new();
    for ip in candidates {
        match verify_openwrt(ip, ssh_user, ssh_password) {
            Ok(true) => {
                info!("OpenWrt detected at {}", ip);
                routers.push(ip);
            }
            Ok(false) => debug!("{} is not OpenWrt", ip),
            Err(e) => debug!("{} probe failed: {}", ip, e),
        }
    }

    routers
}

/// Check whether a known IP is an OpenWrt router.
pub fn probe(ip: IpAddr, ssh_user: &str, ssh_password: &str) -> Result<bool> {
    verify_openwrt(ip, ssh_user, ssh_password)
}

fn tcp_reachable(ip: IpAddr, port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::new(ip, port), PROBE_TIMEOUT).is_ok()
}

fn verify_openwrt(ip: IpAddr, user: &str, password: &str) -> Result<bool> {
    let router = Router::connect_password(&ip.to_string(), SSH_PORT, user, password)?;
    let (out, code) = router.run("cat /etc/openwrt_release")?;
    Ok(code == 0 && out.contains("OpenWrt"))
}

fn host_count_for_prefix(prefix_len: u8) -> u32 {
    if prefix_len >= 32 {
        return 1;
    }
    1u32 << (32 - prefix_len)
}
