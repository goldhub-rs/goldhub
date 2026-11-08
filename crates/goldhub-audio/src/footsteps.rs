//! Canonical 1-to-1 port of GoldSrc / CS 1.6 step and landing physics sound system.
//!
//! Directly ports:
//! - `PM_PlayStepSound`
//! - `PM_UpdateStepSound`
//! - `PM_CheckFalling`
//!
//! Preserving exact volume scaling (35% on ducking), interval timing (concrete 300ms, ladder 350ms,
//! ducking +100ms), and alternating foot selection (`iStepLeft = !iStepLeft`, `irand = RandomLong(0,1) + (iStepLeft * 2)`).

use crate::materials::{MaterialTable, MaterialType, StepType};

/// Channel used for player footstep and body sounds in GoldSrc (`CHAN_BODY`).
pub const CHAN_BODY: i32 = 4;

/// Fall damage / scrape sound threshold constants from `pm_shared.h`.
pub const PM_PLAYER_MAX_SAFE_FALL_SPEED: f32 = 580.0;
pub const PM_PLAYER_MIN_BOUNCE_SPEED: f32 = 350.0;
pub const PM_PLAYER_FALL_PUNCH_THRESHHOLD: f32 = 250.0;

/// Water content identification matching GoldSrc BSP contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterPosition {
    /// Not in water
    NotInWater,
    /// Feet in water (`CONTENTS_WATER` at feet level) -> slosh sound
    FeetInWater,
    /// Knee in water (`CONTENTS_WATER` at knee level) -> wade sound
    KneeInWater,
}

/// Sound event emitted by the footstep simulation for external mixer or playback.
#[derive(Debug, Clone, PartialEq)]
pub struct FootstepSoundEvent {
    /// Sound channel (usually `CHAN_BODY`).
    pub channel: i32,
    /// Sound asset path (e.g., `"player/pl_step1.wav"`).
    pub sound_path: &'static str,
    /// Playback volume (0.0 to 1.0).
    pub volume: f32,
    /// Attenuation factor (typically `ATTN_NORM` = 0.8).
    pub attenuation: f32,
    /// Playback pitch (100 = `PITCH_NORM`).
    pub pitch: i32,
    /// Step category.
    pub step_type: StepType,
}

/// Player state inputs required for footstep sound evaluation.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct StepSoundContext<'a> {
    /// Ground planar or 3D velocity vector `[vx, vy, vz]`.
    pub velocity: [f32; 3],
    /// True if the player is resting on walkable ground.
    pub on_ground: bool,
    /// True if mounted on ladder (`MOVETYPE_FLY`).
    pub on_ladder: bool,
    /// True if ducking hull or ducking flag (`FL_DUCKING`) is active.
    pub is_ducking: bool,
    /// Elapsed ducking progress timer in milliseconds (`flDuckTime`).
    pub duck_time_ms: f32,
    /// True if frozen by game rule (`FL_FROZEN`).
    pub is_frozen: bool,
    /// Downward landing fall velocity (`flFallVelocity`).
    pub fall_velocity: f32,
    /// Player alive status.
    pub is_dead: bool,
    /// Water submersion state at knee/feet level.
    pub water_pos: WaterPosition,
    /// Texture name hit directly beneath player feet (for material classification).
    pub ground_texture: Option<&'a str>,
}

/// Footstep simulation controller preserving alternating feet state and cooldown timers.
#[derive(Debug, Clone)]
pub struct StepSoundController {
    /// Alternating foot state: 0 = right foot, 1 = left foot (`pmove->iStepLeft`).
    pub step_left: bool,
    /// Cooldown timer in milliseconds until next step sound is allowed (`pmove->flTimeStepSound`).
    pub time_step_sound_ms: f32,
    /// Skip counter for wade splash cadence (`iSkipStep` in `PM_PlayStepSound`).
    pub skip_step: i32,
    /// Footstep sounds enabled (cvar `mp_footsteps`).
    pub footsteps_enabled: bool,
}

impl Default for StepSoundController {
    fn default() -> Self {
        Self {
            step_left: false,
            time_step_sound_ms: 0.0,
            skip_step: 0,
            footsteps_enabled: true,
        }
    }
}

impl StepSoundController {
    /// Creates a new footstep controller initialized to default HLSDK state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            step_left: false,
            time_step_sound_ms: 0.0,
            skip_step: 0,
            footsteps_enabled: true,
        }
    }

    /// Advances the internal step sound cooldown timer by `delta_time_ms`.
    pub fn advance_time(&mut self, delta_time_ms: f32) {
        if self.time_step_sound_ms > 0.0 {
            self.time_step_sound_ms = (self.time_step_sound_ms - delta_time_ms).max(0.0);
        }
    }

    /// Maps texture and step type to the canonical sample path given the pseudo-random index `irand` (0..=3 or 4).
    ///
    /// Matches `PM_PlayStepSound` switch-cases in `pm_shared.cpp`.
    #[must_use]
    pub fn resolve_step_sound_path(step: StepType, irand: usize) -> &'static str {
        match step {
            StepType::Concrete => match irand {
                0 => "player/pl_step1.wav",
                1 => "player/pl_step3.wav",
                2 => "player/pl_step2.wav",
                _ => "player/pl_step4.wav",
            },
            StepType::Metal => match irand {
                0 => "player/pl_metal1.wav",
                1 => "player/pl_metal3.wav",
                2 => "player/pl_metal2.wav",
                _ => "player/pl_metal4.wav",
            },
            StepType::Dirt => match irand {
                0 => "player/pl_dirt1.wav",
                1 => "player/pl_dirt3.wav",
                2 => "player/pl_dirt2.wav",
                _ => "player/pl_dirt4.wav",
            },
            StepType::Vent => match irand {
                0 => "player/pl_duct1.wav",
                1 => "player/pl_duct3.wav",
                2 => "player/pl_duct2.wav",
                _ => "player/pl_duct4.wav",
            },
            StepType::Grate => match irand {
                0 => "player/pl_grate1.wav",
                1 => "player/pl_grate3.wav",
                2 => "player/pl_grate2.wav",
                _ => "player/pl_grate4.wav",
            },
            StepType::Tile => match irand {
                0 => "player/pl_tile1.wav",
                1 => "player/pl_tile3.wav",
                2 => "player/pl_tile2.wav",
                3 => "player/pl_tile4.wav",
                _ => "player/pl_tile5.wav",
            },
            StepType::Slosh => match irand {
                0 => "player/pl_slosh1.wav",
                1 => "player/pl_slosh3.wav",
                2 => "player/pl_slosh2.wav",
                _ => "player/pl_slosh4.wav",
            },
            StepType::Wade => match irand {
                0 => "player/pl_wade1.wav",
                1 => "player/pl_wade2.wav",
                2 => "player/pl_wade3.wav",
                _ => "player/pl_wade4.wav",
            },
            StepType::Ladder => match irand {
                0 => "player/pl_ladder1.wav",
                1 => "player/pl_ladder3.wav",
                2 => "player/pl_ladder2.wav",
                _ => "player/pl_ladder4.wav",
            },
            StepType::Snow => match irand {
                0 => "player/pl_snow1.wav",
                1 => "player/pl_snow3.wav",
                2 => "player/pl_snow2.wav",
                _ => "player/pl_snow4.wav",
            },
        }
    }

    /// Generates a step sound event using the supplied random bit (0 or 1).
    ///
    /// Alternates feet (`iStepLeft = !iStepLeft`), computes `irand = random_bit + (iStepLeft * 2)`.
    /// Emulates canonical `PM_PlayStepSound`.
    pub fn play_step_sound(
        &mut self,
        step: StepType,
        volume: f32,
        random_bit: u8,
        random_tile_pick: u8,
    ) -> Option<FootstepSoundEvent> {
        // Toggle foot alternating state
        self.step_left = !self.step_left;

        if !self.footsteps_enabled {
            return None;
        }

        let bit = (random_bit & 1) as usize;
        let irand = if step == StepType::Tile && random_tile_pick == 0 {
            4
        } else {
            bit + if self.step_left { 2 } else { 0 }
        };

        // STEP_WADE cadence skip check:
        if step == StepType::Wade {
            if self.skip_step == 0 {
                self.skip_step += 1;
                return None;
            }
            if self.skip_step == 3 {
                self.skip_step = 0;
            } else {
                self.skip_step += 1;
            }
        }

        let sound_path = Self::resolve_step_sound_path(step, irand);

        Some(FootstepSoundEvent {
            channel: CHAN_BODY,
            sound_path,
            volume,
            attenuation: crate::attenuation::ATTN_NORM,
            pitch: 100, // PITCH_NORM
            step_type: step,
        })
    }

    /// Evaluates step sounds for the current frame/tick, strictly matching `PM_UpdateStepSound`.
    ///
    /// - If cooldown `time_step_sound_ms > 0`, returns None.
    /// - If speed <= 150.0, resets cooldown to 400ms and returns None.
    /// - Selects step category (Ladder, Wade, Slosh, or Texture-mapped).
    /// - Sets cooldown (Ladder: 350ms, Wade: 600ms, Ground: 300ms).
    /// - If ducking or on ladder: cooldown += 100ms.
    /// - If ducking and `duck_time_ms < 950.0`: volume *= 0.35.
    pub fn update_step_sound(
        &mut self,
        ctx: &StepSoundContext,
        materials: &MaterialTable,
        random_bit: u8,
        random_tile_pick: u8,
    ) -> Option<FootstepSoundEvent> {
        if self.time_step_sound_ms > 0.0 || ctx.is_frozen || ctx.is_dead {
            return None;
        }

        let speed = (ctx.velocity[0] * ctx.velocity[0]
            + ctx.velocity[1] * ctx.velocity[1]
            + ctx.velocity[2] * ctx.velocity[2])
            .sqrt();

        if speed <= 150.0 {
            self.time_step_sound_ms = 400.0;
            return None;
        }

        let f_ladder = ctx.on_ladder;
        let on_ground = ctx.on_ground;

        if !f_ladder && !on_ground {
            return None;
        }

        let mut fvol: f32;
        let step: StepType;

        if f_ladder {
            step = StepType::Ladder;
            fvol = 0.35;
            self.time_step_sound_ms = 350.0;
        } else if ctx.water_pos == WaterPosition::KneeInWater {
            step = StepType::Wade;
            fvol = 0.65;
            self.time_step_sound_ms = 600.0;
        } else if ctx.water_pos == WaterPosition::FeetInWater {
            step = StepType::Slosh;
            fvol = 0.5;
            self.time_step_sound_ms = 300.0;
        } else {
            let mat = ctx.ground_texture.map_or(MaterialType::Concrete, |tex| {
                materials.find_texture_type(tex)
            });
            step = mat.to_step_type();

            self.time_step_sound_ms = 300.0;
            fvol = match mat {
                MaterialType::Dirt => 0.55,
                MaterialType::Vent => 0.7,
                _ => 0.5,
            };
        }

        if ctx.is_ducking || f_ladder {
            // Slower step time if ducking or on ladder
            self.time_step_sound_ms += 100.0;

            // 35% volume if ducking
            if ctx.is_ducking && ctx.duck_time_ms < 950.0 {
                fvol *= 0.35;
            }
        }

        self.play_step_sound(step, fvol, random_bit, random_tile_pick)
    }

    /// Evaluates landing / fall impact sound strictly matching `PM_CheckFalling`.
    ///
    /// - If `fall_velocity >= PM_PLAYER_FALL_PUNCH_THRESHHOLD (250.0)` and on ground:
    ///   - If `fall_velocity > PM_PLAYER_MAX_SAFE_FALL_SPEED (580.0)`: volume = 1.0
    ///   - Else if `fall_velocity > PM_PLAYER_MAX_SAFE_FALL_SPEED / 2 (290.0)`: volume = 0.85
    ///   - Else if `fall_velocity < PM_PLAYER_MIN_BOUNCE_SPEED (350.0)`: volume = 0.0
    ///   - If volume > 0.0, plays step sound for current texture and sets cooldown to 300ms.
    pub fn check_falling(
        &mut self,
        ctx: &StepSoundContext,
        materials: &MaterialTable,
        random_bit: u8,
        random_tile_pick: u8,
    ) -> Option<FootstepSoundEvent> {
        if !ctx.on_ground || ctx.is_dead || ctx.fall_velocity < PM_PLAYER_FALL_PUNCH_THRESHHOLD {
            return None;
        }

        let mut fvol = 0.5f32;

        if ctx.water_pos == WaterPosition::NotInWater {
            if ctx.fall_velocity > PM_PLAYER_MAX_SAFE_FALL_SPEED {
                fvol = 1.0;
            } else if ctx.fall_velocity > (PM_PLAYER_MAX_SAFE_FALL_SPEED * 0.5) {
                fvol = 0.85;
            } else if ctx.fall_velocity < PM_PLAYER_MIN_BOUNCE_SPEED {
                fvol = 0.0;
            }
        }

        if fvol > 0.0 {
            let mat = ctx.ground_texture.map_or(MaterialType::Concrete, |tex| {
                materials.find_texture_type(tex)
            });
            let step = mat.to_step_type();

            self.time_step_sound_ms = 300.0;
            self.play_step_sound(step, fvol, random_bit, random_tile_pick)
        } else {
            None
        }
    }
}
