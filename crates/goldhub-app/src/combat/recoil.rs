//! Authoritative GoldSrc weapon recoil (`punchangle`) and kickback tables.

use bevy::prelude::*;
pub use goldhub_game_cstrike::PlayerRecoil as CorePlayerRecoil;

/// Player punch angle component representing visual and physical muzzle kickback.
#[derive(Component, Debug, Clone, Copy, Default, Deref, DerefMut, PartialEq)]
pub struct PlayerRecoil(pub goldhub_game_cstrike::PlayerRecoil);

impl PlayerRecoil {
    /// Applies authentic CS 1.6 KickBack impulses to player camera angles with maximum angular bounds.
    #[allow(clippy::too_many_arguments)]
    #[inline]
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
        self.0.apply_kickback(
            up_base,
            lateral_base,
            up_mod,
            lateral_mod,
            up_max,
            lateral_max,
            shots_fired,
            seed,
        );
    }
}

/// Decays camera punch angles smoothly back to center according to GoldSrc client prediction (PM_DropPunchAngle).
pub fn decay_punchangle_system(time: Res<Time>, mut query: Query<&mut PlayerRecoil>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for mut recoil in &mut query {
        recoil.0.decay(dt);
    }
}
