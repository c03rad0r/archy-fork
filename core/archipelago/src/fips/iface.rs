//! Detect the `fips0` TUN interface's ULA (fd00::/8) IPv6 address.
//!
//! The `fips` daemon configures the TUN device with an address derived
//! from the node's identity key. We need that address to bind a
//! peer-facing listener that is only reachable from the FIPS overlay —
//! WAN IPv6 addresses never carry ULA prefixes, so binding specifically
//! to the fips0 address keeps the peer surface off the public internet.
//!
//! We read `/proc/net/if_inet6` rather than shelling out to `ip` so
//! this can run under the `archipelago` service user without extra
//! capabilities.
#![allow(dead_code)]

use std::net::Ipv6Addr;

/// Interface name the FIPS daemon creates (matches upstream default in
/// `/etc/fips/fips.yaml: tun.name`).
pub const FIPS_IFACE: &str = "fips0";

/// Return the first ULA (fd00::/8) address assigned to `fips0`, if any.
///
/// - `None` if the interface is missing, has no address, or only has
///   link-local addresses.
/// - Link-local (`fe80::/10`) and non-ULA addresses are ignored — we
///   only want the mesh-routable ULA that `<npub>.fips` DNS resolves to.
pub fn fips0_ula() -> Option<Ipv6Addr> {
    addresses_on(FIPS_IFACE).into_iter().find(|a| is_ula(a))
}

/// List every IPv6 address bound to a given interface from
/// `/proc/net/if_inet6`. Returns empty on any parse failure.
pub fn addresses_on(iface: &str) -> Vec<Ipv6Addr> {
    let contents = match std::fs::read_to_string("/proc/net/if_inet6") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    contents
        .lines()
        .filter_map(|line| parse_line(line, iface))
        .collect()
}

/// `fd00::/8` test — covers the full ULA range.
pub fn is_ula(addr: &Ipv6Addr) -> bool {
    (addr.octets()[0] & 0xFE) == 0xFC
}

fn parse_line(line: &str, iface: &str) -> Option<Ipv6Addr> {
    // /proc/net/if_inet6 format (whitespace-separated):
    //   <32 hex chars addr> <idx> <prefixlen> <scope> <flags> <devname>
    // e.g. "fdd8...cd85 6f 80 00 80 fips0"
    let mut parts = line.split_whitespace();
    let hex = parts.next()?;
    let _idx = parts.next()?;
    let _prefix = parts.next()?;
    let _scope = parts.next()?;
    let _flags = parts.next()?;
    let name = parts.next()?;
    if name != iface {
        return None;
    }
    if hex.len() != 32 {
        return None;
    }
    let mut octets = [0u8; 16];
    for i in 0..16 {
        octets[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(Ipv6Addr::from(octets))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_line_extracts_address() {
        let line = "fdd83d5aabe08c0ee67f75fcf0d4cd85 6f 80 00 80 fips0";
        let addr = parse_line(line, "fips0").unwrap();
        assert_eq!(
            addr,
            "fdd8:3d5a:abe0:8c0e:e67f:75fc:f0d4:cd85"
                .parse::<Ipv6Addr>()
                .unwrap()
        );
    }

    #[test]
    fn parse_line_rejects_other_iface() {
        let line = "fdd83d5aabe08c0ee67f75fcf0d4cd85 6f 80 00 80 eth0";
        assert!(parse_line(line, "fips0").is_none());
    }

    #[test]
    fn parse_line_ignores_malformed() {
        assert!(parse_line("garbage", "fips0").is_none());
        assert!(parse_line("shorthex 6f 80 00 80 fips0", "fips0").is_none());
    }

    #[test]
    fn ula_classifier_matches_fd_range() {
        assert!(is_ula(&"fd00::1".parse().unwrap()));
        assert!(is_ula(&"fdff::".parse().unwrap()));
        assert!(is_ula(&"fc00::1".parse().unwrap()));
        assert!(!is_ula(&"fe80::1".parse().unwrap())); // link-local
        assert!(!is_ula(&"2001:db8::1".parse().unwrap())); // global
        assert!(!is_ula(&"::1".parse().unwrap())); // loopback
    }
}
