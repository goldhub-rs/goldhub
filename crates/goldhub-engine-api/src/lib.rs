//! # GoldHub Engine API
//!
//! Pure, vendor-agnostic engine abstractions and lifecycle SPI contracts.
//! This crate contains zero engine dependencies and zero game-specific logic,
//! defining the inverted boundary between game rules, engine cores, and frontends.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod ir;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Canonical spatial raycast and hull trace output.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TraceResult {
    /// Fraction of the sweep completed before obstruction [0.0..1.0].
    pub fraction: f32,
    /// Absolute world position where the hull stopped or impacted.
    pub endpos: [f32; 3],
    /// Surface normal vector at point of impact (normalized).
    pub normal: [f32; 3],
    /// True if the trace started inside solid geometry.
    pub start_solid: bool,
    /// True if the hull was completely enclosed in solid geometry.
    pub all_solid: bool,
    /// Entity ID that was struck (0 = worldspawn, >0 = specific brush/studio entity).
    pub hit_entity: u32,
}

impl Default for TraceResult {
    fn default() -> Self {
        Self {
            fraction: 1.0,
            endpos: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            start_solid: false,
            all_solid: false,
            hit_entity: 0,
        }
    }
}

/// Ladder contact point and geometry orientation.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LadderContact {
    /// Outward climbing normal vector perpendicular to the ladder face.
    pub normal: [f32; 3],
    /// Geometric center of the ladder entity bounding box in world space.
    pub center: [f32; 3],
    /// Entity ID of the ladder (`func_ladder`).
    pub entity_id: u32,
}

/// Abstract spatial query provider for world geometry and entity hulls.
pub trait WorldTrace {
    /// Traces an AABB hull from `start` to `end` against the world and all active solid brush entities.
    fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], hull_idx: u32) -> TraceResult;

    /// Tests whether the designated player hull intersects any solid world or brush geometry at `pos`.
    /// Returns `Some(hit_entity)` if inside solid (0 for worldspawn, entity_id for brush entity), or `None` if free.
    fn test_player_position(&self, pos: [f32; 3], hull_idx: u32) -> Option<u32>;

    /// Probes for contact with a ladder entity (`func_ladder`) intersecting the player hull.
    fn find_ladder(&self, _origin: [f32; 3], _hull_idx: u32) -> Option<LadderContact> {
        None
    }
}

/// Abstract audio emitter service for playing sound effects and ambient tracks.
pub trait AudioSink {
    /// Plays a one-shot 2D spatialized or UI sound effect.
    fn play_sound_2d(&mut self, sound_path: &str, volume: f32, pitch: f32);

    /// Plays a 3D positioned sound effect in the world.
    fn play_sound_3d(
        &mut self,
        sound_path: &str,
        position: [f32; 3],
        volume: f32,
        pitch: f32,
        attenuation: f32,
    );

    /// Stops any active instance of the designated sound.
    fn stop_sound(&mut self, sound_path: &str);
}

/// Abstract game tick and continuous time source.
pub trait Clock {
    /// Current continuous simulation time in seconds.
    fn now_secs(&self) -> f64;

    /// Fixed simulation delta time for the current tick in seconds.
    fn delta_secs(&self) -> f32;

    /// Monotonically increasing tick counter.
    fn current_tick(&self) -> u64;
}

/// Abstract asset loader interface.
pub trait AssetSource {
    /// Loads raw byte content for a designated virtual file path.
    fn load_bytes(&self, path: &str) -> Option<&[u8]>;
}

/// Universal Game Rules lifecycle and policy SPI.
/// Implemented by game modules (`goldhub-game-cstrike`, deathmatch, mods) to govern
/// gameplay loops, player spawning, round resets, and scoring.
pub trait GameRules {
    /// Returns the human-readable identifier of the game rules (e.g. "Counter-Strike 1.6").
    fn game_name(&self) -> &'static str;

    /// Invoked at the start of a round to reset entity states and map objectives.
    fn on_round_start(&mut self);

    /// Invoked every tick during the simulation phase.
    fn update_round(&mut self, dt: f32);

    /// Determines whether friendly fire is currently permitted.
    fn is_friendly_fire_enabled(&self) -> bool {
        false
    }

    /// Determines whether a player is currently eligible to access the buy menu.
    fn can_player_buy(&self, player_id: u32) -> bool {
        let _ = player_id;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockWorld;
    impl WorldTrace for MockWorld {
        fn trace_player_hull(
            &self,
            _start: [f32; 3],
            end: [f32; 3],
            _hull_idx: u32,
        ) -> TraceResult {
            TraceResult {
                fraction: 1.0,
                endpos: end,
                normal: [0.0, 0.0, 1.0],
                start_solid: false,
                all_solid: false,
                hit_entity: 0,
            }
        }

        fn test_player_position(&self, _pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
            None
        }
    }

    #[test]
    fn test_trace_result_defaults_and_trait_object() {
        let world: &dyn WorldTrace = &MockWorld;
        let res = world.trace_player_hull([0.0, 0.0, 0.0], [0.0, 0.0, 100.0], 0);
        assert_eq!(res.fraction, 1.0);
        assert_eq!(res.endpos, [0.0, 0.0, 100.0]);
        assert_eq!(world.test_player_position([10.0, 20.0, 30.0], 0), None);
        assert_eq!(world.find_ladder([0.0, 0.0, 0.0], 0), None);
    }
}
