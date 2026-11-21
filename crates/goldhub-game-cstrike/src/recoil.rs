//! Pure mathematical CS 1.6 recoil (`punchangle`), muzzle kickback, and decay.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Authoritative player punch angle representing visual and physical muzzle kickback.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlayerRecoil {
    pub punch_pitch: f32,
    pub punch_yaw: f32,
    /// Alternating horizontal direction (+1 or -1) matching ReGameDLL `m_iDirection`.
    pub direction: i32,
}

impl PlayerRecoil {
    /// Creates a new zeroed recoil component.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            punch_pitch: 0.0,
            punch_yaw: 0.0,
            direction: 1,
        }
    }

    /// Applies authentic CS 1.6 KickBack impulses to player camera angles with maximum angular bounds.
    ///
    /// Matches ReGameDLL `CBasePlayerWeapon::KickBack`:
    /// - Shot 1 applies `up_base` / `lateral_base` directly; shots `> 1` add `shots_fired * mod`.
    /// - `up_max == 0.0` or `lateral_max == 0.0` indicates unbounded kick (CS 1.6 pistols/snipers).
    /// - Direction flips with probability `1 / (direction_change + 1)`.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_kickback(
        &mut self,
        up_base: f32,
        lateral_base: f32,
        up_mod: f32,
        lateral_mod: f32,
        up_max: f32,
        lateral_max: f32,
        direction_change: u32,
        shots_fired: u32,
        seed: f32,
    ) {
        let (up_kick, lateral_kick) = if shots_fired > 1 {
            let shot_f = shots_fired as f32;
            (
                up_base + shot_f * up_mod,
                lateral_base + shot_f * lateral_mod,
            )
        } else {
            (up_base, lateral_base)
        };

        // In Bevy mouse look convention, positive pitch is looking UP (+pitch tilts view up).
        if up_max == 0.0 {
            self.punch_pitch += up_kick;
        } else if self.punch_pitch < up_max {
            self.punch_pitch = (self.punch_pitch + up_kick).min(up_max);
        }

        // Initialize direction cleanly only when uninitialized (0), preserving left (-1) and right (+1)
        if self.direction == 0 {
            self.direction = 1;
        }
        let sign = if self.direction == 1 { 1.0 } else { -1.0 };
        if lateral_max == 0.0 {
            self.punch_yaw += sign * lateral_kick;
        } else if self.punch_yaw.abs() < lateral_max {
            self.punch_yaw =
                (self.punch_yaw + sign * lateral_kick).clamp(-lateral_max, lateral_max);
        }

        // Direction change check matching ReGameDLL `if (!RANDOM_LONG(0, direction_change))`
        if direction_change > 0 && seed < 1.0 / (direction_change as f32 + 1.0) {
            self.direction = if self.direction == 1 { -1 } else { 1 };
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kickback_preserves_direction_reversal() {
        let mut r = PlayerRecoil::new();
        assert_eq!(r.direction, 1);

        // Trigger a flip (seed < 1 / 9 = 0.111)
        r.apply_kickback(1.0, 0.375, 0.175, 0.0375, 5.75, 1.75, 8, 1, 0.05);
        assert_eq!(r.direction, -1);

        // Next shot with seed >= 0.14 must KEEP spraying in direction -1
        let prev_yaw = r.punch_yaw;
        r.apply_kickback(1.0, 0.375, 0.175, 0.0375, 5.75, 1.75, 8, 2, 0.90);
        assert_eq!(r.direction, -1);
        assert!(
            r.punch_yaw < prev_yaw,
            "Subsequent shot must push yaw further left (-1)"
        );
    }

    #[test]
    fn test_unbounded_vertical_kickback_and_shot1_impulse() {
        let mut r = PlayerRecoil::new();
        r.apply_kickback(2.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0, 1, 0.5);
        assert!((r.punch_pitch - 2.0).abs() < 1e-6);

        // Repeat with up_max == 0.0 (unbounded pistol/sniper kickback)
        r.apply_kickback(2.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0, 2, 0.5);
        assert!((r.punch_pitch - 5.0).abs() < 1e-6);
    }
}
