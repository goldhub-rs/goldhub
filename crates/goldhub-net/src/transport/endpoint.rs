//! Network address representations and endpoints.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Compact, zero-allocation network endpoint representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct NetAddress {
    pub ip: [u8; 4],
    pub port: u16,
}

impl NetAddress {
    /// Creates a new IPv4 endpoint from 4 bytes and port.
    #[inline(always)]
    pub const fn new(ip: [u8; 4], port: u16) -> Self {
        Self { ip, port }
    }

    /// Localhost loopback endpoint (127.0.0.1:port).
    #[inline(always)]
    pub const fn loopback(port: u16) -> Self {
        Self {
            ip: [127, 0, 0, 1],
            port,
        }
    }

    /// Broadcast endpoint (255.255.255.255:port).
    #[inline(always)]
    pub const fn broadcast(port: u16) -> Self {
        Self {
            ip: [255, 255, 255, 255],
            port,
        }
    }

    /// Converts to standard library `SocketAddr`.
    #[inline(always)]
    pub fn to_socket_addr(self) -> SocketAddr {
        SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(
                self.ip[0], self.ip[1], self.ip[2], self.ip[3],
            )),
            self.port,
        )
    }
}

impl From<SocketAddr> for NetAddress {
    #[inline(always)]
    fn from(addr: SocketAddr) -> Self {
        match addr {
            SocketAddr::V4(v4) => Self {
                ip: v4.ip().octets(),
                port: v4.port(),
            },
            SocketAddr::V6(v6) => {
                // Fallback / mapped IPv4 for compatibility
                let octets = v6.ip().octets();
                Self {
                    ip: [octets[12], octets[13], octets[14], octets[15]],
                    port: v6.port(),
                }
            }
        }
    }
}

impl From<NetAddress> for SocketAddr {
    #[inline(always)]
    fn from(addr: NetAddress) -> Self {
        addr.to_socket_addr()
    }
}

impl fmt::Display for NetAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}:{}",
            self.ip[0], self.ip[1], self.ip[2], self.ip[3], self.port
        )
    }
}
