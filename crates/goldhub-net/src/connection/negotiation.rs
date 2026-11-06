//! Client capability negotiation, server modes, and backwards compatibility fallbacks.

use crate::error::NetError;

pub const PROTOCOL_GOLDSRC_48: u32 = 48;
pub const PROTOCOL_GOLDHUB_NEXTGEN_1: u32 = 1001;

pub const CAP_SUBTICK: u32 = 1 << 0;
pub const CAP_WIRE_RING_DMA: u32 = 1 << 1;
pub const CAP_ECS_DELTAS: u32 = 1 << 2;
pub const CAP_ODD_INT_COMPRESSION: u32 = 1 << 3;

/// Specific feature capabilities negotiated during the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientCapabilities {
    pub protocol_version: u32,
    pub feature_flags: u32,
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self::legacy_protocol_48()
    }
}

impl ClientCapabilities {
    /// Baseline capabilities for standard CS 1.6 Protocol 48 clients.
    pub const fn legacy_protocol_48() -> Self {
        Self {
            protocol_version: PROTOCOL_GOLDSRC_48,
            feature_flags: 0,
        }
    }

    /// Full NextGen capabilities for GoldHub clients.
    pub const fn nextgen_goldhub() -> Self {
        Self {
            protocol_version: PROTOCOL_GOLDHUB_NEXTGEN_1,
            feature_flags: CAP_SUBTICK
                | CAP_WIRE_RING_DMA
                | CAP_ECS_DELTAS
                | CAP_ODD_INT_COMPRESSION,
        }
    }

    /// Checks if sub-tick input capturing is supported.
    #[inline(always)]
    pub const fn supports_subtick(&self) -> bool {
        (self.feature_flags & CAP_SUBTICK) != 0
    }

    /// Checks if wire-ring DMA buffer mapping is supported.
    #[inline(always)]
    pub const fn supports_wire_ring_dma(&self) -> bool {
        (self.feature_flags & CAP_WIRE_RING_DMA) != 0
    }

    /// Checks if ECS delta bitmasks are supported.
    #[inline(always)]
    pub const fn supports_ecs_deltas(&self) -> bool {
        (self.feature_flags & CAP_ECS_DELTAS) != 0
    }

    /// Checks if odd-int coordinate quantization is supported.
    #[inline(always)]
    pub const fn supports_odd_int_compression(&self) -> bool {
        (self.feature_flags & CAP_ODD_INT_COMPRESSION) != 0
    }
}

/// Server operation mode defining compatibility boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServerMode {
    /// 100% retro GoldSrc compatibility (Protocol 48 only, 100 FPS physics emulation).
    LegacyStrict,
    /// Dual-stack hybrid: serves both classic Protocol 48 clients and NextGen GoldHub clients.
    #[default]
    HybridDualStack,
    /// NextGen esports standard: zero legacy baggage, requires modern client.
    NextGenPure,
}

/// Negotiated channel profile for a connected client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveChannelProfile {
    /// Classic Protocol 48 channel with legacy lumps and delta compression.
    LegacyProtocol48,
    /// Full NextGen channel with sub-tick inputs, ECS component bitmasks, and DMA frames.
    NextGenFastPath,
}

/// Decision engine resolving compatibility between client and server.
#[derive(Debug, Clone, Copy)]
pub struct CapabilityNegotiator {
    server_mode: ServerMode,
}

impl CapabilityNegotiator {
    /// Initializes a capability negotiator with the active server mode.
    pub const fn new(server_mode: ServerMode) -> Self {
        Self { server_mode }
    }

    /// Returns the active server mode.
    pub const fn server_mode(&self) -> ServerMode {
        self.server_mode
    }

    /// Evaluates incoming client connection request and determines connection validity and channel profile.
    pub fn negotiate(&self, client: ClientCapabilities) -> Result<ActiveChannelProfile, NetError> {
        match self.server_mode {
            ServerMode::LegacyStrict => {
                // In LegacyStrict, all clients (even NextGen) operate in Protocol 48 fallback mode
                Ok(ActiveChannelProfile::LegacyProtocol48)
            }
            ServerMode::HybridDualStack => {
                if client.protocol_version >= PROTOCOL_GOLDHUB_NEXTGEN_1
                    && client.supports_subtick()
                {
                    Ok(ActiveChannelProfile::NextGenFastPath)
                } else {
                    Ok(ActiveChannelProfile::LegacyProtocol48)
                }
            }
            ServerMode::NextGenPure => {
                if client.protocol_version >= PROTOCOL_GOLDHUB_NEXTGEN_1 {
                    Ok(ActiveChannelProfile::NextGenFastPath)
                } else {
                    Err(NetError::ConnectionClosed(
                        "This server requires the GoldHub client (NextGen Protocol v1). Outdated clients not supported in NextGenPure mode.",
                    ))
                }
            }
        }
    }
}
