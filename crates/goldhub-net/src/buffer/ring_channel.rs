//! High-performance ring buffer channels powered by `wire-ring`.

use wire_ring::presets::Ring;
use wire_ring::{Consumer, MirroredStorage, Producer};

use crate::error::NetError;
use crate::transport::NetAddress;

/// Maximum payload size of a single UDP packet supported by the ring buffer.
pub const MAX_PACKET_SIZE: usize = 4096;

/// Mirrored zero-allocation ring channel for network packet staging.
pub struct NetworkRingChannel {
    tx: Producer<MirroredStorage>,
    rx: Consumer<MirroredStorage>,
}

impl NetworkRingChannel {
    /// Initializes a new mirrored network ring channel with the specified byte capacity.
    ///
    /// The capacity will be page-aligned by `wire-ring`.
    pub fn new(capacity: usize) -> Result<Self, NetError> {
        let (tx, rx) = Ring::network_default(capacity)?;
        Ok(Self { tx, rx })
    }

    /// Enqueues a packet into the ring buffer.
    ///
    /// Encodes a 16-bit payload length prefix and the source address followed by the raw bytes.
    pub fn enqueue_packet(&mut self, addr: NetAddress, payload: &[u8]) -> Result<(), NetError> {
        let payload_len = payload.len();
        if payload_len > MAX_PACKET_SIZE {
            return Err(NetError::PacketTooLarge {
                max: MAX_PACKET_SIZE,
                received: payload_len,
            });
        }

        // Frame format: [u16 len][4B ip][u16 port][payload...]
        let total_frame_len = 2 + 4 + 2 + payload_len;
        if self.tx.available_write() < total_frame_len {
            return Err(NetError::RingCapacityExhausted {
                requested: total_frame_len,
            });
        }

        let mut grant = self
            .tx
            .grant(total_frame_len)
            .ok_or(NetError::RingCapacityExhausted {
                requested: total_frame_len,
            })?;

        let slice = &mut grant[..total_frame_len];
        slice[0..2].copy_from_slice(&(payload_len as u16).to_le_bytes());
        slice[2..6].copy_from_slice(&addr.ip);
        slice[6..8].copy_from_slice(&addr.port.to_le_bytes());
        slice[8..total_frame_len].copy_from_slice(payload);

        grant.commit(total_frame_len);
        Ok(())
    }

    /// Dequeues the next packet from the ring buffer into the provided slice.
    ///
    /// Returns `Ok(Some((bytes_read, remote_addr)))` or `Ok(None)` if empty.
    pub fn dequeue_packet(
        &mut self,
        out_buf: &mut [u8],
    ) -> Result<Option<(usize, NetAddress)>, NetError> {
        let Some(grant) = self.rx.grant() else {
            return Ok(None);
        };

        if grant.len() < 8 {
            return Ok(None);
        }

        let payload_len = u16::from_le_bytes([grant[0], grant[1]]) as usize;
        let total_frame_len = 8 + payload_len;

        if grant.len() < total_frame_len {
            return Ok(None);
        }

        let ip = [grant[2], grant[3], grant[4], grant[5]];
        let port = u16::from_le_bytes([grant[6], grant[7]]);
        let remote_addr = NetAddress::new(ip, port);

        if out_buf.len() < payload_len {
            return Err(NetError::PacketTooSmall {
                expected: payload_len,
                received: out_buf.len(),
            });
        }

        out_buf[..payload_len].copy_from_slice(&grant[8..total_frame_len]);
        grant.commit(total_frame_len);

        Ok(Some((payload_len, remote_addr)))
    }

    /// Returns the number of bytes available for reading in the ring.
    #[inline(always)]
    pub fn available_read(&mut self) -> usize {
        self.rx.available_read()
    }

    /// Returns the free write capacity in the ring.
    #[inline(always)]
    pub fn available_write(&mut self) -> usize {
        self.tx.available_write()
    }
}
