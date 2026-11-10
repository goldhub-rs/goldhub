//! Authoritative GoldSrc weapon recoil (`punchangle`) and kickback tables.

use bevy::prelude::*;

/// Player punch angle component representing visual and physical muzzle kickback.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct PlayerRecoil {
    pub punch_pitch: f32,
    pub punch_yaw: f32,
}

impl PlayerRecoil {
    /// Applies authentic CS 1.6 KickBack impulses to player camera angles with maximum angular bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_kickback(
        &mut self,
        up_base: f32,
        lateral_base: f32,
        up_mod: f32,
        lateral_mod: f32,
        up_max: f32,
        lateral_max: f32,
        shots_fired: u32,
        seed: f32,
    ) {
        let shot_f = shots_fired.saturating_sub(1) as f32;
        let up_kick = up_base + shot_f * up_mod;
        let side_sign = if seed > 0.5 { 1.0 } else { -1.0 };
        let lateral_kick = side_sign * (lateral_base + shot_f * lateral_mod);

        // In Bevy mouse look convention, positive pitch is looking UP (+pitch tilts view up).
        // Enforce boundary clamping matching ReGameDLL CBasePlayerWeapon::KickBack.
        self.punch_pitch = (self.punch_pitch + up_kick).min(up_max);
        self.punch_yaw = (self.punch_yaw + lateral_kick).clamp(-lateral_max, lateral_max);
    }
}

/// Decays camera punch angles smoothly back to center according to GoldSrc client prediction (PM_DropPunchAngle).
pub fn decay_punchangle_system(time: Res<Time>, mut query: Query<&mut PlayerRecoil>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for mut recoil in &mut query {
        let len = recoil.punch_pitch.hypot(recoil.punch_yaw);
        if len > 1e-4 {
            let dir_p = recoil.punch_pitch / len;
            let dir_y = recoil.punch_yaw / len;
            // ReHLDS PM_DropPunchAngle: len -= (10.0 + len * 0.5) * dt
            let new_len = (len - (10.0 + len * 0.5) * dt).max(0.0);
            recoil.punch_pitch = dir_p * new_len;
            recoil.punch_yaw = dir_y * new_len;
        } else {
            recoil.punch_pitch = 0.0;
            recoil.punch_yaw = 0.0;
        }
    }
}
