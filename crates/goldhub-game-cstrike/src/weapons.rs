//! Canonical CS 1.6 weapon specifications, damage tables, and animation parameters.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Canonical CS 1.6 Weapon Identifiers matching ReGameDLL `weapontype.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u32)]
pub enum WeaponId {
    #[default]
    Knife = 29,
    Glock18 = 17,
    USP45 = 16,
    Deagle = 26,
    AK47 = 28,
    M4A1 = 22,
    AWP = 18,
}

/// Weapon classification slot in the player's inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum WeaponSlot {
    Primary = 1,
    Secondary = 2,
    Melee = 3,
    Grenade = 4,
    C4 = 5,
}

impl WeaponId {
    /// Returns the inventory slot for this weapon.
    #[must_use]
    pub const fn slot(self) -> WeaponSlot {
        match self {
            Self::Knife => WeaponSlot::Melee,
            Self::Glock18 | Self::USP45 | Self::Deagle => WeaponSlot::Secondary,
            Self::AK47 | Self::M4A1 | Self::AWP => WeaponSlot::Primary,
        }
    }
}

/// Static weapon balance and asset definition matching ReGameDLL_CS.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct WeaponDef {
    pub name: &'static str,
    pub damage: f32,
    pub cycle_time: f32,
    pub clip_size: i32,
    pub max_reserve: i32,
    pub reload_time: f32,
    pub v_model: &'static str,
    pub p_model: &'static str,
    pub anim_extension: &'static str,
    pub shoot_sound: &'static str,
    pub reload_sound: &'static str,
    pub deploy_anim: usize,
    pub shoot_anim: usize,
    pub reload_anim: usize,
    pub recoil_up: f32,
    pub recoil_lateral: f32,
    pub up_max: f32,
    pub lateral_max: f32,
    pub is_automatic: bool,
    pub penetration: u32,
    pub penetration_power: f32,
    pub penetration_distance: f32,
}

/// Returns the authoritative weapon definition matching ReGameDLL.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn get_weapon_def(id: WeaponId) -> WeaponDef {
    match id {
        WeaponId::Knife => WeaponDef {
            name: "Knife",
            damage: 25.0,
            cycle_time: 0.35,
            clip_size: 0,
            max_reserve: 0,
            reload_time: 0.0,
            v_model: "models/v_knife.mdl",
            p_model: "models/p_knife.mdl",
            anim_extension: "knife",
            shoot_sound: "weapons/knife_slash1.wav",
            reload_sound: "",
            deploy_anim: 3,
            shoot_anim: 6,
            reload_anim: 0,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            up_max: 0.0,
            lateral_max: 0.0,
            is_automatic: false,
            penetration: 0,
            penetration_power: 0.0,
            penetration_distance: 0.0,
        },
        WeaponId::Glock18 => WeaponDef {
            name: "Glock-18",
            damage: 20.0,
            cycle_time: 0.15,
            clip_size: 20,
            max_reserve: 120,
            reload_time: 2.2,
            v_model: "models/v_glock18.mdl",
            p_model: "models/p_glock18.mdl",
            anim_extension: "onehanded",
            shoot_sound: "weapons/glock18-1.wav",
            reload_sound: "weapons/glock_clipout.wav",
            deploy_anim: 6,
            shoot_anim: 1,
            reload_anim: 5,
            recoil_up: 0.8,
            recoil_lateral: 0.3,
            up_max: 3.5,
            lateral_max: 1.0,
            is_automatic: false,
            penetration: 1,
            penetration_power: 21.0,
            penetration_distance: 800.0,
        },
        WeaponId::USP45 => WeaponDef {
            name: "USP-45",
            damage: 34.0,
            cycle_time: 0.16,
            clip_size: 12,
            max_reserve: 100,
            reload_time: 2.7,
            v_model: "models/v_usp.mdl",
            p_model: "models/p_usp.mdl",
            anim_extension: "onehanded",
            shoot_sound: "weapons/usp_unsil-1.wav",
            reload_sound: "weapons/usp_clipout.wav",
            deploy_anim: 14, // draw_unsil
            shoot_anim: 9,   // shoot1_unsil
            reload_anim: 13, // reload_unsil
            recoil_up: 0.8,
            recoil_lateral: 0.35,
            up_max: 4.0,
            lateral_max: 1.25,
            is_automatic: false,
            penetration: 1,
            penetration_power: 15.0,
            penetration_distance: 500.0,
        },
        WeaponId::Deagle => WeaponDef {
            name: "Desert Eagle",
            damage: 54.0,
            cycle_time: 0.22,
            clip_size: 7,
            max_reserve: 35,
            reload_time: 2.2,
            v_model: "models/v_deagle.mdl",
            p_model: "models/p_deagle.mdl",
            anim_extension: "onehanded",
            shoot_sound: "weapons/deagle-1.wav",
            reload_sound: "weapons/de_clipout.wav",
            deploy_anim: 5,
            shoot_anim: 1,
            reload_anim: 4,
            recoil_up: 1.8,
            recoil_lateral: 0.6,
            up_max: 6.0,
            lateral_max: 2.5,
            is_automatic: false,
            penetration: 2,
            penetration_power: 30.0,
            penetration_distance: 2000.0,
        },
        WeaponId::AK47 => WeaponDef {
            name: "AK-47",
            damage: 36.0,
            cycle_time: 0.0955, // ~600 RPM
            clip_size: 30,
            max_reserve: 90,
            reload_time: 2.45,
            v_model: "models/v_ak47.mdl",
            p_model: "models/p_ak47.mdl",
            anim_extension: "ak47",
            shoot_sound: "weapons/ak47-1.wav",
            reload_sound: "weapons/ak47_clipout.wav",
            deploy_anim: 2,
            shoot_anim: 3,
            reload_anim: 1,
            recoil_up: 1.3,
            recoil_lateral: 0.45,
            up_max: 6.0,
            lateral_max: 2.0,
            is_automatic: true,
            penetration: 2,
            penetration_power: 42.0,
            penetration_distance: 4000.0,
        },
        WeaponId::M4A1 => WeaponDef {
            name: "M4A1",
            damage: 32.0,
            cycle_time: 0.0875, // ~685 RPM
            clip_size: 30,
            max_reserve: 90,
            reload_time: 3.1,
            v_model: "models/v_m4a1.mdl",
            p_model: "models/p_m4a1.mdl",
            anim_extension: "m4a1",
            shoot_sound: "weapons/m4a1_unsil-1.wav",
            reload_sound: "weapons/m4a1_clipout.wav",
            deploy_anim: 12, // draw_unsil
            shoot_anim: 7,   // shoot1_unsil
            reload_anim: 11, // reload_unsil
            recoil_up: 1.0,
            recoil_lateral: 0.35,
            up_max: 5.0,
            lateral_max: 1.5,
            is_automatic: true,
            penetration: 2,
            penetration_power: 35.0,
            penetration_distance: 4000.0,
        },
        WeaponId::AWP => WeaponDef {
            name: "AWP",
            damage: 115.0,
            cycle_time: 1.45,
            clip_size: 10,
            max_reserve: 30,
            reload_time: 2.5,
            v_model: "models/v_awp.mdl",
            p_model: "models/p_awp.mdl",
            anim_extension: "rifle",
            shoot_sound: "weapons/awp1.wav",
            reload_sound: "weapons/boltpull1.wav",
            deploy_anim: 3,
            shoot_anim: 1,
            reload_anim: 2,
            recoil_up: 3.5,
            recoil_lateral: 0.8,
            up_max: 8.0,
            lateral_max: 3.0,
            is_automatic: false,
            penetration: 3,
            penetration_power: 45.0,
            penetration_distance: 8000.0,
        },
    }
}
