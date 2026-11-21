//! Canonical Data & Command Intermediate Representation (SSOT).
//!
//! All subsystems (modern Bevy ECS systems, game-rule plugins, physics kernels,
//! and the future C-ABI Metamod/AMXX FFI facade) translate to and from these
//! vendor-agnostic IR types without coupling to internal pointer layouts.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Canonical 5-phase deterministic frame execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum EnginePhase {
    /// Phase 1: Packet ingestion, `UserCmd` capture, legacy dirty outbound sync.
    Ingestion = 0,
    /// Phase 2: Deterministic FixedUpdate kinematics & collision contact gathering.
    Physics = 1,
    /// Phase 3: Pluggable GameRules, triggers, ballistics, and hook dispatch.
    Logic = 2,
    /// Phase 4: Structural mutations, entity spawn/despawn, snapshot commit.
    Mutation = 3,
    /// Phase 5: Interpolation, audio DSP mixing, particles, and HUD/Radar draw.
    Presentation = 4,
}

/// Entity interaction activation mode (`USE_TYPE` in GoldSrc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum UseType {
    /// Deactivate / close / turn off.
    Off = 0,
    /// Activate / open / turn on.
    On = 1,
    /// Set explicit numeric state (`value`).
    Set = 2,
    /// Flip current state (default for player `+use`).
    Toggle = 3,
}

/// Hook verdict mirroring Metamod-R execution control without C globals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HookVerdict<T> {
    /// Hook did not inspect or alter execution.
    Ignored,
    /// Hook reacted side-effectfully; continue normal execution.
    Handled,
    /// Execute original engine behavior, then replace return value with `T`.
    Override(T),
    /// Halt engine execution immediately and return `T`.
    Supercede(T),
}

/// Cache-line aligned (64-byte) linear & angular kinematics snapshot.
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KinematicsIr {
    /// World origin in GoldSrc coordinate space `(X=Right, Y=Forward, Z=Up)`.
    pub origin: [f32; 3],
    /// Linear velocity in units/sec.
    pub velocity: [f32; 3],
    /// Inherited conveyor/platform base velocity in units/sec.
    pub base_velocity: [f32; 3],
    /// View angles `[pitch, yaw, roll]` in degrees.
    pub angles: [f32; 3],
    /// Additive camera punch angles `[pitch, yaw, roll]` in degrees.
    pub punch_angle: [f32; 3],
    /// Explicit padding to complete 64-byte L1 cache line alignment.
    pub _pad: u32,
}

const _: () = assert!(core::mem::size_of::<KinematicsIr>() == 64);

/// Player/Entity hull bounding box specification.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BoundsIr {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub solid: u8,
    pub hull_index: u8,
    pub _pad: [u8; 2],
}

impl Default for BoundsIr {
    fn default() -> Self {
        Self {
            mins: [-16.0, -16.0, -36.0],
            maxs: [16.0, 16.0, 36.0],
            solid: 3, // SOLID_SLIDEBOX / BBOX parity default
            hull_index: 0,
            _pad: [0; 2],
        }
    }
}

/// Canonical command/event bus messages bridging ECS systems and legacy hooks.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum EngineCommandIr {
    /// Schedules or triggers an entity think pulse.
    ScheduleThink {
        entity_id: u32,
        next_think_secs: f64,
    },
    /// Notifies logic systems of a collision impact pair (never mutates during Phase 2).
    ContactImpact {
        entity_a: u32,
        entity_b: u32,
        impact_point: [f32; 3],
        normal: [f32; 3],
    },
    /// Requests state activation on a target entity.
    Interact {
        target_id: u32,
        activator_id: u32,
        caller_id: u32,
        use_type: UseType,
        value: f32,
    },
    /// Applies impulse or kinetic damage from logic phase to physics/health systems.
    Damage {
        target_id: u32,
        inflictor_id: u32,
        attacker_id: u32,
        amount: f32,
        damage_bits: u32,
    },
    /// Triggers a 3D spatial sound with GoldSrc attenuation coefficient.
    EmitSound {
        entity_id: u32,
        channel: u8,
        volume: f32,
        attenuation: f32,
        pitch: u8,
        origin: [f32; 3],
    },
}

/// Radar marker shape conforming to CS 1.6 Z-axis elevation convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum RadarGlyphKind {
    /// Target is on approximately the same elevation (`|ΔZ| < 128`).
    #[default]
    Dot = 0,
    /// Target is elevated above observer (`ΔZ <= -128` in GoldSrc player-minus-target).
    AboveT = 1,
    /// Target is below observer (`ΔZ >= 128`).
    BelowFlippedT = 2,
    /// Planted objective / urgent cross indicator (`X`).
    Cross = 3,
}

/// Normalized RGBA8 color for HUD/Radar primitives.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RadarColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RadarColor {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// Game-agnostic radar blip descriptor emitted by GameRules plugins.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RadarMarkerIr {
    /// World coordinates in GoldSrc axes `(X=Right, Y=Forward, Z=Up)`.
    pub world_pos: [f32; 3],
    /// Visual color on the radar disc.
    pub color: RadarColor,
    /// Forced glyph override (if `Some(Cross)` e.g. planted C4), otherwise derived from `ΔZ`.
    pub force_glyph: Option<RadarGlyphKind>,
    /// Whether this marker is currently visible (handles blink phase).
    pub visible: bool,
}

/// Zero-allocation fixed ring buffer storing historical ticks for client prediction rollback.
#[derive(Debug, Clone)]
pub struct SnapshotBuffer<T: Copy + Default, const N: usize> {
    slots: [T; N],
    ticks: [u64; N],
    valid: [bool; N],
}

impl<T: Copy + Default, const N: usize> Default for SnapshotBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Default, const N: usize> SnapshotBuffer<T, N> {
    /// Creates an empty ring buffer with zero heap allocations.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: [T::default(); N],
            ticks: [0; N],
            valid: [false; N],
        }
    }

    /// Commits a snapshot state at `tick`, overwriting the oldest slot modulo `N`.
    pub fn push(&mut self, tick: u64, state: T) {
        if N == 0 {
            return;
        }
        let idx = (tick as usize) % N;
        self.slots[idx] = state;
        self.ticks[idx] = tick;
        self.valid[idx] = true;
    }

    /// Retrieves the committed snapshot for an exact `tick`, or `None` if evicted or unrecorded.
    #[must_use]
    pub fn get(&self, tick: u64) -> Option<&T> {
        if N == 0 {
            return None;
        }
        let idx = (tick as usize) % N;
        if self.valid[idx] && self.ticks[idx] == tick {
            Some(&self.slots[idx])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_buffer_ring_rollback() {
        let mut buf: SnapshotBuffer<KinematicsIr, 64> = SnapshotBuffer::new();
        for t in 0..70u64 {
            let state = KinematicsIr {
                origin: [t as f32, 0.0, 0.0],
                ..Default::default()
            };
            buf.push(t, state);
        }

        // Ticks 0..5 were overwritten by 64..69
        assert!(buf.get(0).is_none());
        assert!(buf.get(5).is_none());
        // Tick 64..69 and 6..63 remain valid
        assert_eq!(buf.get(64).unwrap().origin[0], 64.0);
        assert_eq!(buf.get(69).unwrap().origin[0], 69.0);
        assert_eq!(buf.get(10).unwrap().origin[0], 10.0);
    }
}
