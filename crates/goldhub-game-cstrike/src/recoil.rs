//! Pure mathematical CS 1.6 recoil (`punchangle`), muzzle kickback, and decay.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Authoritative player punch angle representing visual and physical muzzle kickback.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlayerRecoil {
    pub punch_pitch: f32,
    pub punch_yaw: f32,
}

impl PlayerRecoil {
    /// Creates a new zeroed recoil component.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            punch_pitch: 0.0,
            punch_yaw: 0.0,
        }
    }

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

    /// Decays camera punch angles smoothly back to center according to GoldSrc client prediction (`PM_DropPunchAngle`).
    pub fn decay(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        let len = self.punch_pitch.hypot(self.punch_yaw);
        if len > 1e-4 {
            let dir_p = self.punch_pitch / len;
            let dir_y = self.punch_yaw / len;
            // ReHLDS PM_DropPunchAngle: len -= (10.0 + len * 0.5) * dt
            let new_len = (len - (10.0 + len * 0.5) * dt).max(0.0);
            self.punch_pitch = dir_p * new_len;
            self.punch_yaw = dir_y * new_len;
        } else {
            self.punch_pitch = 0.0;
            self.punch_yaw = 0.0;
        }
    }
}
