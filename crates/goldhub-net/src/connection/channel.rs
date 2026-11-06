//! NetConnection: End-to-end client connection management, sequencing, and packet staging.

use super::flow::{FlowStats, RateLimiter};
use super::state::ConnectionState;
use crate::buffer::{AdaptiveJitterBuffer, MAX_PACKET_SIZE, NetworkRingChannel};
use crate::error::NetError;
use crate::protocol::netchan::NetChannelHeader;
use crate::protocol::oob::{OOB_PREFIX, OobMessage};
use crate::transport::{NetAddress, UdpTransport};

/// A decoded incoming packet payload from the netchannel.
#[derive(Debug, Clone)]
pub struct IncomingPacket {
    pub sequence: u32,
    pub is_reliable: bool,
    pub payload: Vec<u8>,
}

/// Fully managed client network connection.
pub struct NetConnection {
    transport: UdpTransport,
    remote_addr: Option<NetAddress>,
    state: ConnectionState,
    outgoing_sequence: u32,
    incoming_sequence: u32,
    reliable_sequence: u32,
    last_reliable_received: bool,
    flow: FlowStats,
    rate_limiter: RateLimiter,
    jitter_buffer: AdaptiveJitterBuffer,
    ring: NetworkRingChannel,
}

impl NetConnection {
    /// Initializes a new connection on an ephemeral UDP port.
    pub fn new(capacity: usize) -> Result<Self, NetError> {
        let transport = UdpTransport::bind_ephemeral()?;
        let ring = NetworkRingChannel::new(capacity)?;
        let rate_limiter = RateLimiter::new(60_000); // 60 KB/s rate default

        Ok(Self {
            transport,
            remote_addr: None,
            state: ConnectionState::Disconnected,
            outgoing_sequence: 1,
            incoming_sequence: 0,
            reliable_sequence: 0,
            last_reliable_received: false,
            flow: FlowStats::default(),
            rate_limiter,
            jitter_buffer: AdaptiveJitterBuffer::new(),
            ring,
        })
    }

    /// Sets the target server destination.
    pub fn set_remote(&mut self, remote: NetAddress) {
        self.remote_addr = Some(remote);
        self.state = ConnectionState::Challenging;
    }

    /// Returns the remote target address if connected.
    #[inline(always)]
    pub const fn remote_addr(&self) -> Option<NetAddress> {
        self.remote_addr
    }

    /// Returns the current connection state.
    #[inline(always)]
    pub const fn state(&self) -> ConnectionState {
        self.state
    }

    /// Sets the connection state.
    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    /// Sends an out-of-band message to the remote endpoint.
    pub fn send_oob(&mut self, msg: &OobMessage<'_>) -> Result<usize, NetError> {
        let remote = self
            .remote_addr
            .ok_or(NetError::ConnectionClosed("no remote endpoint set"))?;
        let mut buf = [0u8; 1024];
        let len = msg.encode(&mut buf)?;
        self.transport.send(&buf[..len], remote)
    }

    /// Transmits a connected netchannel packet with automatic sequencing and reliability flags.
    pub fn send_packet(&mut self, payload: &[u8], reliable: bool) -> Result<usize, NetError> {
        let remote = self
            .remote_addr
            .ok_or(NetError::ConnectionClosed("not connected"))?;

        let mut buf = [0u8; MAX_PACKET_SIZE];
        let header = NetChannelHeader {
            sequence: self.outgoing_sequence,
            sequence_ack: self.incoming_sequence,
            reliable_message: reliable,
            reliable_ack: self.last_reliable_received,
            contains_fragments: false,
            streams: Default::default(),
        };

        let header_len = header.encode(&mut buf)?;
        let total_len = header_len + payload.len();

        if total_len > MAX_PACKET_SIZE {
            return Err(NetError::PacketTooLarge {
                max: MAX_PACKET_SIZE,
                received: total_len,
            });
        }

        // Apply rate limiter flow control
        if !self.rate_limiter.try_consume(total_len) {
            return Err(NetError::RingCapacityExhausted {
                requested: total_len,
            });
        }

        buf[header_len..total_len].copy_from_slice(payload);

        let sent = self.transport.send(&buf[..total_len], remote)?;
        if sent > 0 {
            self.outgoing_sequence = self.outgoing_sequence.wrapping_add(1);
            if reliable {
                self.reliable_sequence = self.reliable_sequence.wrapping_add(1);
            }
            self.flow.packets_sent += 1;
            self.flow.bytes_sent += sent as u64;
        }

        Ok(sent)
    }

    /// Pumps incoming UDP datagrams into the internal ring buffer and processes them.
    pub fn pump_incoming(&mut self, now_micros: u64) -> Result<Option<IncomingPacket>, NetError> {
        let mut recv_buf = [0u8; MAX_PACKET_SIZE];

        // 1. Drain socket into mirrored ring buffer
        while let Some((len, remote)) = self.transport.recv(&mut recv_buf)? {
            let _ = self.ring.enqueue_packet(remote, &recv_buf[..len]);
        }

        // 2. Process next packet from ring
        let mut dequeue_buf = [0u8; MAX_PACKET_SIZE];
        if let Some((len, remote)) = self.ring.dequeue_packet(&mut dequeue_buf)? {
            let packet_data = &dequeue_buf[..len];

            // Verify packet comes from known remote or accepts new remote during challenge
            if self.remote_addr.is_none() {
                self.remote_addr = Some(remote);
            } else if self.remote_addr != Some(remote) {
                return Ok(None);
            }

            // Check OOB message
            if len >= 4 && packet_data[0..4] == OOB_PREFIX {
                // Out-of-band packet
                return Ok(None);
            }

            // Decode NetChannel packet
            let (header, payload) = NetChannelHeader::decode(packet_data)?;

            // Sequence validation & packet drop tracking
            if header.sequence <= self.incoming_sequence && self.incoming_sequence > 0 {
                // Duplicate or late packet: ignore
                return Ok(None);
            }

            if header.sequence > self.incoming_sequence.wrapping_add(1)
                && self.incoming_sequence > 0
            {
                let dropped = header.sequence - self.incoming_sequence - 1;
                self.flow.record_dropped_packets(u64::from(dropped));
            }

            self.incoming_sequence = header.sequence;
            self.last_reliable_received = header.reliable_message;
            self.flow.packets_received += 1;
            self.flow.bytes_received += len as u64;

            // Slot packet into jitter buffer
            self.jitter_buffer.push_packet(
                header.sequence,
                now_micros, // Arrival timestamp
                now_micros,
                payload,
            );
        }

        // 3. Pop continuous packet from jitter buffer
        if let Some(jitter_pkt) = self.jitter_buffer.pop_next() {
            return Ok(Some(IncomingPacket {
                sequence: jitter_pkt.sequence,
                is_reliable: false,
                payload: jitter_pkt.payload[..jitter_pkt.payload_len as usize].to_vec(),
            }));
        }

        Ok(None)
    }

    /// Accesses the underlying UDP transport.
    #[inline(always)]
    pub const fn transport(&self) -> &UdpTransport {
        &self.transport
    }

    /// Accesses flow statistics (RTT, packet loss, bandwidth).
    #[inline(always)]
    pub const fn flow(&self) -> &FlowStats {
        &self.flow
    }

    /// Accesses the rate limiter mutably to replenish tokens per frame.
    #[inline(always)]
    pub fn rate_limiter_mut(&mut self) -> &mut RateLimiter {
        &mut self.rate_limiter
    }

    /// Returns the current reliable sequence counter.
    #[inline(always)]
    pub const fn reliable_sequence(&self) -> u32 {
        self.reliable_sequence
    }

    /// Returns the local bound endpoint.
    #[inline(always)]
    pub fn local_addr(&self) -> NetAddress {
        self.transport.local_addr()
    }
}
