//! Zero-allocation adaptive jitter buffer for smoothing UDP packet arrival bursts.

use super::ring_channel::MAX_PACKET_SIZE;

const JITTER_SLOTS: usize = 64;
const SLOT_MASK: usize = JITTER_SLOTS - 1;

/// A buffered packet ready for playback.
#[derive(Clone, Copy)]
pub struct JitterPacket {
    pub sequence: u32,
    pub timestamp_micros: u64,
    pub payload_len: u16,
    pub payload: [u8; MAX_PACKET_SIZE],
}

impl Default for JitterPacket {
    fn default() -> Self {
        Self {
            sequence: 0,
            timestamp_micros: 0,
            payload_len: 0,
            payload: [0u8; MAX_PACKET_SIZE],
        }
    }
}

/// Adaptive packet jitter buffer to eliminate frame micro-stuttering.
pub struct AdaptiveJitterBuffer {
    slots: Box<[Option<JitterPacket>]>,
    next_playout_seq: u32,
    initialized: bool,
    smoothed_jitter_micros: u64,
    last_arrival_micros: u64,
    last_transit_micros: u64,
}

impl Default for AdaptiveJitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveJitterBuffer {
    /// Creates a new empty jitter buffer, pre-allocating slots on the heap.
    pub fn new() -> Self {
        let boxed_slots = vec![None; JITTER_SLOTS].into_boxed_slice();
        Self {
            slots: boxed_slots,
            next_playout_seq: 0,
            initialized: false,
            smoothed_jitter_micros: 1000, // Initial 1ms estimate
            last_arrival_micros: 0,
            last_transit_micros: 0,
        }
    }

    /// Inserts a newly arrived packet into the jitter buffer.
    ///
    /// Updates the network jitter statistics and slots the packet by sequence number.
    pub fn push_packet(
        &mut self,
        sequence: u32,
        send_timestamp_micros: u64,
        now_micros: u64,
        payload: &[u8],
    ) {
        if self.initialized {
            // Update jitter estimation via RFC 3550 EMA
            let current_transit = now_micros.saturating_sub(send_timestamp_micros);
            let transit_diff = current_transit.abs_diff(self.last_transit_micros);
            self.smoothed_jitter_micros = (self.smoothed_jitter_micros * 15 + transit_diff) / 16;
            self.last_transit_micros = current_transit;
            self.last_arrival_micros = now_micros;
        } else {
            self.next_playout_seq = sequence;
            self.initialized = true;
            self.last_arrival_micros = now_micros;
            self.last_transit_micros = now_micros.saturating_sub(send_timestamp_micros);
        }

        // If packet is older than current playout sequence, drop it as late
        if sequence < self.next_playout_seq {
            return;
        }

        // Drop if beyond slot window capacity
        if sequence >= self.next_playout_seq + (JITTER_SLOTS as u32) {
            return;
        }

        let slot_idx = (sequence as usize) & SLOT_MASK;
        let mut pkt = JitterPacket {
            sequence,
            timestamp_micros: send_timestamp_micros,
            payload_len: payload.len().min(MAX_PACKET_SIZE) as u16,
            payload: [0u8; MAX_PACKET_SIZE],
        };
        let copy_len = payload.len().min(MAX_PACKET_SIZE);
        pkt.payload[..copy_len].copy_from_slice(&payload[..copy_len]);

        self.slots[slot_idx] = Some(pkt);
    }

    /// Pops the next continuous packet in the playout sequence.
    ///
    /// Returns `Some(packet)` if ready, or `None` if waiting for packet in-flight.
    pub fn pop_next(&mut self) -> Option<JitterPacket> {
        if !self.initialized {
            return None;
        }

        let slot_idx = (self.next_playout_seq as usize) & SLOT_MASK;
        if let Some(pkt) = self.slots[slot_idx].take() {
            if pkt.sequence == self.next_playout_seq {
                self.next_playout_seq = self.next_playout_seq.wrapping_add(1);
                return Some(pkt);
            }
            // Put it back if out of sequence order
            self.slots[slot_idx] = Some(pkt);
        }

        None
    }

    /// Returns estimated smoothed network jitter in microseconds.
    #[inline(always)]
    pub const fn smoothed_jitter_micros(&self) -> u64 {
        self.smoothed_jitter_micros
    }

    /// Resets the jitter buffer state.
    pub fn reset(&mut self) {
        self.slots.fill(None);
        self.initialized = false;
        self.next_playout_seq = 0;
        self.smoothed_jitter_micros = 1000;
    }
}
