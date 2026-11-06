//! Protocol 48 NetChannel framing, stream fragmentation, and sequence tracking.

use crate::error::NetError;

pub const MAX_STREAMS: usize = 2;
pub const STREAM_NORMAL: usize = 0;
pub const STREAM_FILE: usize = 1;

pub const RELIABLE_FLAG: u32 = 1 << 31;
pub const FRAGMENTED_FLAG: u32 = 1 << 30;
pub const SEQUENCE_MASK: u32 = !(RELIABLE_FLAG | FRAGMENTED_FLAG);

/// Single multiplexed stream fragment descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamFragment {
    pub is_fragmented: bool,
    pub fragment_id: u32,
    pub fragment_offset: u16,
    pub fragment_length: u16,
}

/// Connected packet header for GoldSrc Protocol 48.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetChannelHeader {
    pub sequence: u32,
    pub sequence_ack: u32,
    pub reliable_message: bool,
    pub reliable_ack: bool,
    pub contains_fragments: bool,
    pub streams: [StreamFragment; MAX_STREAMS],
}

impl NetChannelHeader {
    /// Decodes a NetChannel header from raw packet bytes.
    ///
    /// Returns the parsed header along with the remaining packet slice (the payload).
    pub fn decode(bytes: &[u8]) -> Result<(Self, &[u8]), NetError> {
        if bytes.len() < 8 {
            return Err(NetError::PacketTooSmall {
                expected: 8,
                received: bytes.len(),
            });
        }

        let raw_seq = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let raw_ack = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        let reliable_message = (raw_seq & RELIABLE_FLAG) != 0;
        let reliable_ack = (raw_ack & RELIABLE_FLAG) != 0;
        let contains_fragments = (raw_seq & FRAGMENTED_FLAG) != 0;

        let sequence = raw_seq & SEQUENCE_MASK;
        let sequence_ack = raw_ack & SEQUENCE_MASK;

        let mut offset = 8;
        let mut streams = [StreamFragment::default(); MAX_STREAMS];

        if contains_fragments {
            for stream in &mut streams {
                if offset >= bytes.len() {
                    return Err(NetError::MalformedHeader("truncated fragment stream flag"));
                }
                let has_frag = bytes[offset] != 0;
                offset += 1;

                if has_frag {
                    if offset + 8 > bytes.len() {
                        return Err(NetError::MalformedHeader(
                            "truncated fragment descriptor parameters",
                        ));
                    }
                    let fragment_id = u32::from_le_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ]);
                    let fragment_offset =
                        u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]);
                    let fragment_length =
                        u16::from_le_bytes([bytes[offset + 6], bytes[offset + 7]]);
                    offset += 8;

                    *stream = StreamFragment {
                        is_fragmented: true,
                        fragment_id,
                        fragment_offset,
                        fragment_length,
                    };
                }
            }
        }

        let header = Self {
            sequence,
            sequence_ack,
            reliable_message,
            reliable_ack,
            contains_fragments,
            streams,
        };

        Ok((header, &bytes[offset..]))
    }

    /// Encodes the NetChannel header into the target buffer.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, NetError> {
        if out.len() < 8 {
            return Err(NetError::PacketTooSmall {
                expected: 8,
                received: out.len(),
            });
        }

        let mut raw_seq = self.sequence & SEQUENCE_MASK;
        if self.reliable_message {
            raw_seq |= RELIABLE_FLAG;
        }
        if self.contains_fragments {
            raw_seq |= FRAGMENTED_FLAG;
        }

        let mut raw_ack = self.sequence_ack & SEQUENCE_MASK;
        if self.reliable_ack {
            raw_ack |= RELIABLE_FLAG;
        }

        out[0..4].copy_from_slice(&raw_seq.to_le_bytes());
        out[4..8].copy_from_slice(&raw_ack.to_le_bytes());

        let mut offset = 8;

        if self.contains_fragments {
            for stream in &self.streams {
                if offset >= out.len() {
                    return Err(NetError::PacketTooSmall {
                        expected: offset + 1,
                        received: out.len(),
                    });
                }
                if stream.is_fragmented {
                    if offset + 9 > out.len() {
                        return Err(NetError::PacketTooSmall {
                            expected: offset + 9,
                            received: out.len(),
                        });
                    }
                    out[offset] = 1;
                    out[offset + 1..offset + 5].copy_from_slice(&stream.fragment_id.to_le_bytes());
                    out[offset + 5..offset + 7]
                        .copy_from_slice(&stream.fragment_offset.to_le_bytes());
                    out[offset + 7..offset + 9]
                        .copy_from_slice(&stream.fragment_length.to_le_bytes());
                    offset += 9;
                } else {
                    out[offset] = 0;
                    offset += 1;
                }
            }
        }

        Ok(offset)
    }
}
