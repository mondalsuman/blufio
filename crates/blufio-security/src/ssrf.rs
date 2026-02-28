// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SSRF-safe DNS resolver that blocks connections to private IP ranges.
//!
//! Implements `reqwest::dns::Resolve` to filter resolved IP addresses before
//! any connection is made, preventing Server-Side Request Forgery attacks.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use blufio_core::BlufioError;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tracing::{error, info};

/// Custom DNS resolver that blocks private/reserved IP addresses.
///
/// When a hostname resolves to a private IP, the connection is blocked
/// unless that IP is in the configured allowlist. This prevents SSRF
/// attacks where an attacker tricks the agent into connecting to internal
/// services.
pub struct SsrfSafeResolver {
    allowed_private_ips: Vec<IpAddr>,
}

impl SsrfSafeResolver {
    /// Create a new resolver with the given private IP allowlist.
    pub fn new(allowed: Vec<String>) -> Self {
        let allowed_ips = allowed
            .iter()
            .filter_map(|s| s.parse::<IpAddr>().ok())
            .collect();
        Self {
            allowed_private_ips: allowed_ips,
        }
    }

    /// Check if an IP is in a private or reserved range.
    ///
    /// Blocks: RFC 1918, loopback, link-local, broadcast, unspecified,
    /// AWS metadata endpoint, IPv6 loopback, unique-local, link-local.
    pub fn is_private(ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_unspecified()
                    || *v4 == Ipv4Addr::new(169, 254, 169, 254) // AWS metadata
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
                    || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
            }
        }
    }
}

impl Resolve for SsrfSafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let allowed = self.allowed_private_ips.clone();
        let hostname = name.as_str().to_string();

        Box::pin(async move {
            // Resolve DNS normally.
            let host = format!("{hostname}:0");
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&host)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(e)
                })?
                .collect();

            // Filter private IPs.
            let filtered: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|addr| {
                    let ip = addr.ip();
                    if SsrfSafeResolver::is_private(&ip) {
                        if allowed.contains(&ip) {
                            info!(ip = %ip, host = %hostname, "allowing configured private IP");
                            true
                        } else {
                            error!(ip = %ip, host = %hostname, "SSRF blocked: resolved to private IP");
                            false
                        }
                    } else {
                        true
                    }
                })
                .collect();

            if filtered.is_empty() {
                let err: Box<dyn std::error::Error + Send + Sync> =
                    format!("SSRF blocked: {hostname} resolves only to private IPs").into();
                return Err(err);
            }

            let addrs: Addrs = Box::new(filtered.into_iter());
            Ok(addrs)
        })
    }
}

/// Convenience function to check if an IP is private/reserved.
///
/// This is the same logic as `SsrfSafeResolver::is_private` but available
/// without constructing a resolver instance.
pub fn is_private_ip(ip: &IpAddr) -> bool {
    SsrfSafeResolver::is_private(ip)
}

/// Validate that a URL does not target a private IP.
///
/// This is a static check on the URL host -- it catches literal IP addresses
/// but not hostnames (which require DNS resolution).
pub fn validate_url_host(url: &str) -> Result<(), BlufioError> {
    if let Ok(parsed) = url::Url::parse(url)
        && let Some(host) = parsed.host_str()
        && let Ok(ip) = host.parse::<IpAddr>()
        && SsrfSafeResolver::is_private(&ip)
    {
        error!(ip = %ip, url = %url, "SSRF blocked: URL targets private IP");
        return Err(BlufioError::Security(format!(
            "SSRF blocked: URL targets private IP {ip}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // --- IPv4 private range tests ---

    #[test]
    fn blocks_rfc1918_class_a() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_rfc1918_class_b() {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));
        assert!(SsrfSafeResolver::is_private(&ip));

        let ip = IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_rfc1918_class_c() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_loopback_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert!(SsrfSafeResolver::is_private(&ip));

        let ip = IpAddr::V4(Ipv4Addr::new(127, 255, 255, 255));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_link_local_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_unspecified_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_broadcast() {
        let ip = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_aws_metadata() {
        let ip = IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    // --- IPv6 private range tests ---

    #[test]
    fn blocks_loopback_v6() {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_unspecified_v6() {
        let ip = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_unique_local_v6() {
        // fc00::/7
        let ip = IpAddr::V6(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1));
        assert!(SsrfSafeResolver::is_private(&ip));

        let ip = IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn blocks_link_local_v6() {
        // fe80::/10
        let ip = IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
        assert!(SsrfSafeResolver::is_private(&ip));
    }

    // --- Public IP tests ---

    #[test]
    fn allows_public_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert!(!SsrfSafeResolver::is_private(&ip));

        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        assert!(!SsrfSafeResolver::is_private(&ip));

        let ip = IpAddr::V4(Ipv4Addr::new(104, 18, 0, 1));
        assert!(!SsrfSafeResolver::is_private(&ip));
    }

    #[test]
    fn allows_public_v6() {
        // Google DNS IPv6
        let ip = IpAddr::V6(Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888));
        assert!(!SsrfSafeResolver::is_private(&ip));
    }

    // --- Allowlist tests ---

    #[test]
    fn allowlist_parses_ip_strings() {
        let resolver = SsrfSafeResolver::new(vec![
            "10.0.0.1".to_string(),
            "192.168.1.100".to_string(),
            "invalid".to_string(), // should be silently ignored
        ]);
        assert_eq!(resolver.allowed_private_ips.len(), 2);
    }

    // --- URL validation tests ---

    #[test]
    fn validate_url_host_blocks_private_ip() {
        assert!(validate_url_host("http://10.0.0.1:8080/api").is_err());
        assert!(validate_url_host("http://192.168.1.1/admin").is_err());
        assert!(validate_url_host("http://127.0.0.1/internal").is_err());
    }

    #[test]
    fn validate_url_host_allows_public_ip() {
        assert!(validate_url_host("https://8.8.8.8/dns").is_ok());
        assert!(validate_url_host("https://1.1.1.1/").is_ok());
    }

    #[test]
    fn validate_url_host_allows_hostnames() {
        // Hostnames can't be checked statically -- they need DNS resolution.
        assert!(validate_url_host("https://api.anthropic.com/v1").is_ok());
    }
}
