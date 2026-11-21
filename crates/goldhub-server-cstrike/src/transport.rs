//! Transport abstractions for orthogonal Counter-Strike 1.6 server.
//!
//! Allows the authoritative server to run identically whether in-process (embedded zero-copy)
//! or over real network sockets (`goldhub-net` UDP).

use std::collections::VecDeque;

/// Abstract bi-directional packet transport boundary for the server.
pub trait ServerTransport: Send + Sync {
    /// Polls the next incoming packet from a client. Returns `(client_id, payload)`.
    fn poll_incoming(&mut self) -> Option<(u32, Vec<u8>)>;

    /// Sends an outgoing packet to a specific client.
    fn send_to(&mut self, client_id: u32, packet: &[u8]);

    /// Broadcasts a packet to all connected clients.
    fn broadcast(&mut self, packet: &[u8]);
}

/// In-Process / Embedded transport using memory queues for zero-copy singleplayer or listen server.
#[derive(Default, Debug)]
pub struct InProcessTransport {
    pub client_to_server: VecDeque<(u32, Vec<u8>)>,
    pub server_to_client: VecDeque<(u32, Vec<u8>)>,
}

impl InProcessTransport {
    /// Creates a new in-process pair of queues.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a client packet directly into the server's incoming queue.
    pub fn push_from_client(&mut self, client_id: u32, packet: Vec<u8>) {
        self.client_to_server.push_back((client_id, packet));
    }

    /// Pops the next packet sent from the server to the client.
    pub fn pop_for_client(&mut self) -> Option<(u32, Vec<u8>)> {
        self.server_to_client.pop_front()
    }
}

impl ServerTransport for InProcessTransport {
    fn poll_incoming(&mut self) -> Option<(u32, Vec<u8>)> {
        self.client_to_server.pop_front()
    }

    fn send_to(&mut self, client_id: u32, packet: &[u8]) {
        self.server_to_client
            .push_back((client_id, packet.to_vec()));
    }

    fn broadcast(&mut self, packet: &[u8]) {
        self.server_to_client.push_back((0, packet.to_vec()));
    }
}
