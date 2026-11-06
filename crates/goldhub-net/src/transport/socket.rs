//! Non-blocking UDP transport wrapper.

use std::io::ErrorKind;
use std::net::UdpSocket;

use super::endpoint::NetAddress;
use crate::error::NetError;

/// High-throughput non-blocking UDP socket abstraction.
#[derive(Debug)]
pub struct UdpTransport {
    socket: UdpSocket,
    local_addr: NetAddress,
}

impl UdpTransport {
    /// Binds a UDP socket to the requested address in non-blocking mode.
    pub fn bind(addr: NetAddress) -> Result<Self, NetError> {
        let socket = UdpSocket::bind(addr.to_socket_addr())?;
        socket.set_nonblocking(true)?;

        let local_addr = socket.local_addr()?.into();
        Ok(Self { socket, local_addr })
    }

    /// Binds an ephemeral client socket on loopback (`127.0.0.1:0`).
    pub fn bind_ephemeral() -> Result<Self, NetError> {
        Self::bind(NetAddress::loopback(0))
    }

    /// Returns the local bound endpoint.
    #[inline(always)]
    pub const fn local_addr(&self) -> NetAddress {
        self.local_addr
    }

    /// Enables or disables UDP broadcast transmission.
    pub fn set_broadcast(&self, broadcast: bool) -> Result<(), NetError> {
        self.socket.set_broadcast(broadcast)?;
        Ok(())
    }

    /// Receives a datagram into the provided buffer without blocking.
    ///
    /// Returns `Ok(Some((bytes_read, remote_addr)))` on receipt, or `Ok(None)` if no datagram is pending.
    pub fn recv(&self, buf: &mut [u8]) -> Result<Option<(usize, NetAddress)>, NetError> {
        match self.socket.recv_from(buf) {
            Ok((len, remote)) => Ok(Some((len, remote.into()))),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                Ok(None)
            }
            Err(e) => Err(NetError::Io(e)),
        }
    }

    /// Sends a datagram to the specified destination address without blocking.
    pub fn send(&self, buf: &[u8], target: NetAddress) -> Result<usize, NetError> {
        match self.socket.send_to(buf, target.to_socket_addr()) {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(NetError::Io(e)),
        }
    }
}
