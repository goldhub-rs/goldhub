//! Player state representation and input command structures.

use crate::constants::{HULL_DUCKING, HULL_STANDING, VEC_VIEW_STANDING};

/// Input commands issued by a player during a physical tick (`usercmd_t`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerCmd {
    /// Desired forward/backward impulse (-maxspeed .. +maxspeed).
    pub forward_move: f32,
    /// Desired lateral strafe impulse (-maxspeed .. +maxspeed).
    pub side_move: f32,
    /// Desired vertical impulse.
    pub up_move: f32,
    /// Input button bitflags (`IN_JUMP`, `IN_DUCK`, `IN_SPEED`, etc.).
    pub buttons: u32,
    /// Player camera angles [Pitch, Yaw, Roll] in degrees.
    pub view_angles: [f32; 3],
}

impl Default for PlayerCmd {
    fn default() -> Self {
        Self {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: 0,
            view_angles: [0.0; 3],
        }
    }
}

/// Full authoritative physical state of a GoldSrc player.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerState {
    /// World position of player center origin (waist).
    pub origin: [f32; 3],
    /// Current velocity vector (units per second).
    pub velocity: [f32; 3],
    /// World orientation angles [Pitch, Yaw, Roll] in degrees.
    pub angles: [f32; 3],
    /// Camera eye height offset along the Z axis relative to `origin`.
    pub view_height: f32,
    /// True if the player is resting on a walkable solid floor.
    pub on_ground: bool,
    /// True if the player's bounding box is compressed to ducking hull (`HULL_DUCKING`).
    pub is_ducking: bool,
    /// True if ducking transition is currently active (`bInDuck`).
    pub in_duck: bool,
    /// Remaining time for duck transition in milliseconds (`flDuckTime`).
    pub duck_time_ms: f32,
    /// Jump stamina penalty timer in milliseconds (`fuser2`).
    pub jump_penalty_ms: f32,
    /// Step sound cooldown timer in milliseconds.
    pub step_sound_time_ms: f32,
    /// Downward vertical velocity prior to landing (for falling damage / sound).
    pub fall_velocity: f32,
    /// True if player is actively mounted on a ladder (`g_onladder`).
    pub on_ladder: bool,
    /// Previous tick's button state for edge-detection (jump/duck triggers).
    pub old_buttons: u32,
    /// True if the player is dead and physics simulation should halt.
    pub dead: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            origin: [0.0; 3],
            velocity: [0.0; 3],
            angles: [0.0; 3],
            view_height: VEC_VIEW_STANDING,
            on_ground: false,
            is_ducking: false,
            in_duck: false,
            duck_time_ms: 0.0,
            jump_penalty_ms: 0.0,
            step_sound_time_ms: 0.0,
            fall_velocity: 0.0,
            on_ladder: false,
            old_buttons: 0,
            dead: false,
        }
    }
}

impl PlayerState {
    /// Creates a fresh player state at the designated spawn origin and yaw angle.
    #[must_use]
    pub fn new_at_spawn(origin: [f32; 3], yaw: f32) -> Self {
        Self {
            origin,
            angles: [0.0, yaw, 0.0],
            view_height: VEC_VIEW_STANDING,
            on_ground: false,
            ..Default::default()
        }
    }

    /// Returns the active collision hull index (Hull 1 = Standing, Hull 3 = Ducking).
    #[inline]
    #[must_use]
    pub const fn active_hull(&self) -> u32 {
        if self.is_ducking {
            HULL_DUCKING
        } else {
            HULL_STANDING
        }
    }

    /// Computes total horizontal speed $v_{xy} = \sqrt{v_x^2 + v_y^2}$.
    #[inline]
    #[must_use]
    pub fn horizontal_speed(&self) -> f32 {
        self.velocity[0].hypot(self.velocity[1])
    }
}
