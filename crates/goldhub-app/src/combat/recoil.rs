//! Authoritative GoldSrc weapon recoil (`punchangle`) and kickback tables.

use bevy::prelude::*;

/// Player punch angle component representing visual and physical muzzle kickback.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct PlayerRecoil {
    pub punch_pitch: f32,
    pub punch_yaw: f32,
}

impl PlayerRecoil {
    /// Applies authentic CS 1.6 KickBack impulses to player camera angles.
    pub fn apply_kickback(&mut self, up_base: f32, lateral_base: f32, up_mod: f32, lateral_mod: f32, shots_fired: u32, seed: f32) {
        let shot_f = shots_fired as f32;
        let up_kick = (up_base + shot_f * up_mod).min(6.0);
        let side_sign = if seed > 0.5 { 1.0 } else { -1.0 };
        let lateral_kick = side_sign * (lateral_base + shot_f * lateral_mod).min(2.5);

        // In GoldSrc: positive punchangle.x tilts camera UP (negative pitch in Bevy)
        self.punch_pitch -= up_kick;
        self.punch_yaw += lateral_kick;
    }
}

/// Decays camera punch angles smoothly back to center according to GoldSrc client prediction.
pub fn decay_punchangle_system(
    time: Res<Time>,
    mut query: Query<&mut PlayerRecoil>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // GoldSrc standard punchangle decay: ~10.0 decay factor per second
    let decay = (12.0 * dt).min(1.0);

    for mut recoil in &mut query {
        if recoil.punch_pitch.abs() > 1e-4 || recoil.punch_yaw.abs() > 1e-4 {
            recoil.punch_pitch *= 1.0 - decay;
            recoil.punch_yaw *= 1.0 - decay;
            if recoil.punch_pitch.abs() < 1e-3 {
                recoil.punch_pitch = 0.0;
            }
            if recoil.punch_yaw.abs() < 1e-3 {
                recoil.punch_yaw = 0.0;
            }
        }
    }
}
