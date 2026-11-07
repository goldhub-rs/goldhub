//! Physical constants and parameters for tournament CS 1.6 simulation (ReGameDLL standard).

/// Hull index 0: Point / Ray trace.
pub const HULL_POINT: u32 = 0;
/// Hull index 1: Standing Player (32x32x72).
pub const HULL_STANDING: u32 = 1;
/// Hull index 2: Large / Monster (64x64x64).
pub const HULL_LARGE: u32 = 2;
/// Hull index 3: Ducking Player (32x32x36).
pub const HULL_DUCKING: u32 = 3;

/// Standing player bounding box half-extents relative to origin (GoldSrc convention: origin is at player waist).
pub const PLAYER_HULL_MIN_STANDING: [f32; 3] = [-16.0, -16.0, -36.0];
pub const PLAYER_HULL_MAX_STANDING: [f32; 3] = [16.0, 16.0, 36.0];

/// Ducking player bounding box half-extents relative to origin.
pub const PLAYER_HULL_MIN_DUCKING: [f32; 3] = [-16.0, -16.0, -18.0];
pub const PLAYER_HULL_MAX_DUCKING: [f32; 3] = [16.0, 16.0, 18.0];

/// Eye view height offsets (Z) from player center origin.
pub const VEC_VIEW_STANDING: f32 = 17.0;
pub const VEC_VIEW_DUCKING: f32 = 12.0;

/// Standard GoldSrc duck transition duration in seconds (400 ms).
pub const TIME_TO_DUCK: f32 = 0.4;

/// Tournament-grade sub-tick physics rate (128 Hz).
pub const SUBTICK_HZ: f32 = 128.0;
/// Delta time for one fixed sub-tick (exactly 7.8125 ms).
pub const FIXED_DT: f32 = 1.0 / SUBTICK_HZ;

/// Stop epsilon for zero-crossing detection.
pub const STOP_EPSILON: f32 = 0.1;

/// Standard CS 1.6 jump height impulse: `sqrt(2.0 * 800.0 * 45.0)`.
pub const JUMP_IMPULSE: f32 = 268.328_16;

/// Maximum speed factor when bunny hopping (1.2x maxspeed).
pub const BUNNYJUMP_MAX_SPEED_FACTOR: f32 = 1.2;

/// Movement input button bitflags matching GoldSrc `cmd.buttons`.
pub const IN_ATTACK: u32 = 1 << 0;
pub const IN_JUMP: u32 = 1 << 1;
pub const IN_DUCK: u32 = 1 << 2;
pub const IN_FORWARD: u32 = 1 << 3;
pub const IN_BACK: u32 = 1 << 4;
pub const IN_USE: u32 = 1 << 5;
pub const IN_CANCEL: u32 = 1 << 6;
pub const IN_LEFT: u32 = 1 << 7;
pub const IN_RIGHT: u32 = 1 << 8;
pub const IN_MOVELEFT: u32 = 1 << 9;
pub const IN_MOVERIGHT: u32 = 1 << 10;
pub const IN_ATTACK2: u32 = 1 << 11;
pub const IN_RUN: u32 = 1 << 12;
pub const IN_RELOAD: u32 = 1 << 13;
pub const IN_ALT1: u32 = 1 << 14;
pub const IN_SCORE: u32 = 1 << 15;
pub const IN_SPEED: u32 = 1 << 16;
pub const IN_WALK: u32 = 1 << 17;

/// Movement cvars and environment properties (`movevars_t`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveVars {
    /// World gravity acceleration (default 800.0 u/s²).
    pub gravity: f32,
    /// Velocity threshold below which friction applies proportionally (default 100.0 u/s).
    pub stopspeed: f32,
    /// Maximum player horizontal ground speed (default 320.0 u/s for knife/scout).
    pub maxspeed: f32,
    /// Ground acceleration factor (default 10.0).
    pub accelerate: f32,
    /// Air acceleration factor for strafes (default 10.0 in competitive CS 1.6).
    pub airaccelerate: f32,
    /// Ground surface friction coefficient (default 4.0).
    pub friction: f32,
    /// Maximum vertical height the player can step up automatically (default 18.0 units).
    pub stepsize: f32,
    /// Speed multiplier while crouching (default 0.333).
    pub ducking_speed_multiplier: f32,
    /// Speed multiplier while walking (default 0.52).
    pub walking_speed_multiplier: f32,
    /// Velocity bounce coefficient (default 1.0 = full reflection).
    pub bounce: f32,
}

impl Default for MoveVars {
    fn default() -> Self {
        Self {
            gravity: 800.0,
            stopspeed: 100.0,
            maxspeed: 320.0,
            accelerate: 10.0,
            airaccelerate: 10.0,
            friction: 4.0,
            stepsize: 18.0,
            ducking_speed_multiplier: 0.333,
            walking_speed_multiplier: 0.52,
            bounce: 1.0,
        }
    }
}
