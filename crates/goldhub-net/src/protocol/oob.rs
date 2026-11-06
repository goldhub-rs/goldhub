//! Out-of-band (connectionless) GoldSrc network messages.

use crate::error::NetError;

pub const OOB_PREFIX: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

pub const S2C_CHALLENGE: u8 = b'A';
pub const S2C_CONNECTION: u8 = b'B';
pub const A2S_INFO: u8 = b'T';
pub const A2S_PLAYER: u8 = b'U';
pub const A2S_RULES: u8 = b'V';
pub const A2A_GETCHALLENGE: u8 = b'W';
pub const A2A_PING: u8 = b'i';
pub const A2A_ACK: u8 = b'j';

/// Parsed out-of-band message payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OobMessage<'a> {
    ChallengeRequest,
    ChallengeResponse { challenge: u32 },
    ConnectRequest { challenge: u32, user_info: &'a str },
    ConnectResponse { client_slot: u32 },
    Ping,
    Ack,
    ServerInfoRequest,
    GenericQuery { opcode: u8, payload: &'a [u8] },
}

impl<'a> OobMessage<'a> {
    /// Decodes an out-of-band datagram from raw bytes.
    pub fn decode(bytes: &'a [u8]) -> Result<Option<Self>, NetError> {
        if bytes.len() < 4 || bytes[0..4] != OOB_PREFIX {
            return Ok(None);
        }

        let body = &bytes[4..];
        if body.is_empty() {
            return Ok(None);
        }

        if body.starts_with(b"getchallenge") {
            return Ok(Some(Self::ChallengeRequest));
        }

        let opcode = body[0];
        let payload = &body[1..];

        let msg = match opcode {
            A2A_GETCHALLENGE => Self::ChallengeRequest,
            S2C_CHALLENGE => {
                if payload.len() < 4 {
                    return Err(NetError::MalformedPayload {
                        opcode,
                        reason: "expected 4-byte challenge value".into(),
                    });
                }
                let challenge =
                    u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Self::ChallengeResponse { challenge }
            }
            A2A_PING => Self::Ping,
            A2A_ACK => Self::Ack,
            A2S_INFO => Self::ServerInfoRequest,
            _ => Self::GenericQuery { opcode, payload },
        };

        Ok(Some(msg))
    }

    /// Encodes an out-of-band message into the provided output buffer.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, NetError> {
        if out.len() < 5 {
            return Err(NetError::PacketTooSmall {
                expected: 5,
                received: out.len(),
            });
        }

        out[0..4].copy_from_slice(&OOB_PREFIX);

        match self {
            Self::ChallengeRequest => {
                const REQ: &[u8] = b"getchallenge\0";
                if out.len() < 4 + REQ.len() {
                    return Err(NetError::PacketTooSmall {
                        expected: 4 + REQ.len(),
                        received: out.len(),
                    });
                }
                out[4..4 + REQ.len()].copy_from_slice(REQ);
                Ok(4 + REQ.len())
            }
            Self::ChallengeResponse { challenge } => {
                if out.len() < 9 {
                    return Err(NetError::PacketTooSmall {
                        expected: 9,
                        received: out.len(),
                    });
                }
                out[4] = S2C_CHALLENGE;
                out[5..9].copy_from_slice(&challenge.to_le_bytes());
                Ok(9)
            }
            Self::Ping => {
                out[4] = A2A_PING;
                Ok(5)
            }
            Self::Ack => {
                out[4] = A2A_ACK;
                Ok(5)
            }
            Self::ServerInfoRequest => {
                const PAYLOAD: &[u8] = b"TSource Engine Query\0";
                if out.len() < 4 + PAYLOAD.len() {
                    return Err(NetError::PacketTooSmall {
                        expected: 4 + PAYLOAD.len(),
                        received: out.len(),
                    });
                }
                out[4..4 + PAYLOAD.len()].copy_from_slice(PAYLOAD);
                Ok(4 + PAYLOAD.len())
            }
            Self::ConnectRequest {
                challenge,
                user_info,
            } => {
                let prefix = format!("connect 48 {challenge} \"{user_info}\"\0");
                let bytes = prefix.as_bytes();
                if out.len() < 4 + bytes.len() {
                    return Err(NetError::PacketTooSmall {
                        expected: 4 + bytes.len(),
                        received: out.len(),
                    });
                }
                out[4..4 + bytes.len()].copy_from_slice(bytes);
                Ok(4 + bytes.len())
            }
            Self::ConnectResponse { client_slot } => {
                if out.len() < 9 {
                    return Err(NetError::PacketTooSmall {
                        expected: 9,
                        received: out.len(),
                    });
                }
                out[4] = S2C_CONNECTION;
                out[5..9].copy_from_slice(&client_slot.to_le_bytes());
                Ok(9)
            }
            Self::GenericQuery { opcode, payload } => {
                let total = 5 + payload.len();
                if out.len() < total {
                    return Err(NetError::PacketTooSmall {
                        expected: total,
                        received: out.len(),
                    });
                }
                out[4] = *opcode;
                out[5..total].copy_from_slice(payload);
                Ok(total)
            }
        }
    }
}
