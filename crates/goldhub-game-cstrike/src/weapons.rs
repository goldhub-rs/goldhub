//! Canonical CS 1.6 weapon specifications, damage tables, and animation parameters.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Canonical CS 1.6 Weapon Identifiers matching ReGameDLL `weapontype.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u32)]
pub enum WeaponId {
    P228 = 1,
    Glock = 2,
    Scout = 3,
    HeGrenade = 4,
    XM1014 = 5,
    C4 = 6,
    Mac10 = 7,
    Aug = 8,
    SmokeGrenade = 9,
    Elite = 10,
    FiveSeven = 11,
    Ump45 = 12,
    Sg550 = 13,
    Galil = 14,
    Famas = 15,
    USP45 = 16,
    Glock18 = 17,
    AWP = 18,
    Mp5 = 19,
    M249 = 20,
    M3 = 21,
    M4A1 = 22,
    Tmp = 23,
    G3SG1 = 24,
    Flashbang = 25,
    Deagle = 26,
    Sg552 = 27,
    AK47 = 28,
    #[default]
    Knife = 29,
    P90 = 30,
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

/// Canonical CS 1.6 HUD caliber & grenade icon type (`sprites/640hud7.spr` 24x24 `ammo` rects).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AmmoCaliber {
    None,
    Buckshot12g,
    Ae50,
    Mm9,
    Nato762,
    Acp45,
    Sig357,
    Nato556,
    Magnum338,
    Flashbang,
    HeGrenade,
    C4,
    Mm57,
    SmokeGrenade,
}

impl AmmoCaliber {
    /// Returns the `[x, y]` origin of this 24x24 icon inside `sprites/640hud7.spr`, if any.
    #[must_use]
    pub const fn hud7_rect(self) -> Option<(u32, u32, u32, u32)> {
        match self {
            Self::None => None,
            Self::Buckshot12g => Some((0, 72, 24, 24)),
            Self::Ae50 => Some((24, 72, 24, 24)),
            Self::Mm9 => Some((48, 72, 24, 24)),
            Self::Nato762 => Some((72, 72, 24, 24)),
            Self::Acp45 => Some((96, 72, 24, 24)),
            Self::Sig357 => Some((120, 72, 24, 24)),
            Self::Nato556 => Some((0, 96, 24, 24)),
            Self::Magnum338 => Some((24, 96, 24, 24)),
            Self::Flashbang => Some((48, 96, 24, 24)),
            Self::HeGrenade => Some((72, 96, 24, 24)),
            Self::C4 => Some((96, 96, 24, 24)),
            Self::Mm57 => Some((120, 96, 24, 24)),
            Self::SmokeGrenade => Some((144, 96, 24, 24)),
        }
    }
}

/// Posture-specific kickback parameters passed to [`crate::PlayerRecoil::apply_kickback`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KickBackParams {
    pub up_base: f32,
    pub lateral_base: f32,
    pub up_mod: f32,
    pub lateral_mod: f32,
    pub up_max: f32,
    pub lateral_max: f32,
    pub direction_change: u32,
}

impl KickBackParams {
    /// Creates a complete ReGameDLL `KickBack` parameter tuple.
    #[must_use]
    pub const fn new(
        up_base: f32,
        lateral_base: f32,
        up_mod: f32,
        lateral_mod: f32,
        up_max: f32,
        lateral_max: f32,
        direction_change: u32,
    ) -> Self {
        Self {
            up_base,
            lateral_base,
            up_mod,
            lateral_mod,
            up_max,
            lateral_max,
            direction_change,
        }
    }

    /// Creates an unbounded single-impulse kickback (used by CS 1.6 pistols & bolt-action rifles).
    #[must_use]
    pub const fn uniform_unbounded(up_base: f32) -> Self {
        Self::new(up_base, 0.0, 0.0, 0.0, 0.0, 0.0, 0)
    }
}

/// Posture-aware kickback curves matching ReGameDLL branch priority: moving > airborne > ducking > standing.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RecoilPostures {
    pub moving: KickBackParams,
    pub air: KickBackParams,
    pub ducking: KickBackParams,
    pub standing: KickBackParams,
}

impl RecoilPostures {
    /// Uses identical kickback across all four movement states.
    #[must_use]
    pub const fn uniform(params: KickBackParams) -> Self {
        Self {
            moving: params,
            air: params,
            ducking: params,
            standing: params,
        }
    }

    /// Selects the active posture parameters matching ReGameDLL `pev->velocity.Length2D() > 0` check order.
    #[must_use]
    pub const fn select(&self, speed_2d: f32, on_ground: bool, is_ducking: bool) -> KickBackParams {
        if speed_2d > 0.5 {
            self.moving
        } else if !on_ground {
            self.air
        } else if is_ducking {
            self.ducking
        } else {
            self.standing
        }
    }
}

impl WeaponId {
    /// Returns the inventory slot for this weapon.
    #[must_use]
    pub const fn slot(self) -> WeaponSlot {
        match self {
            Self::Knife => WeaponSlot::Melee,
            Self::P228
            | Self::Glock
            | Self::Elite
            | Self::FiveSeven
            | Self::USP45
            | Self::Glock18
            | Self::Deagle => WeaponSlot::Secondary,
            Self::HeGrenade | Self::SmokeGrenade | Self::Flashbang => WeaponSlot::Grenade,
            Self::C4 => WeaponSlot::C4,
            Self::Scout
            | Self::XM1014
            | Self::Mac10
            | Self::Aug
            | Self::Ump45
            | Self::Sg550
            | Self::Galil
            | Self::Famas
            | Self::AWP
            | Self::Mp5
            | Self::M249
            | Self::M3
            | Self::M4A1
            | Self::Tmp
            | Self::G3SG1
            | Self::Sg552
            | Self::AK47
            | Self::P90 => WeaponSlot::Primary,
        }
    }

    /// Returns an array of all 30 canonical weapons.
    #[must_use]
    pub const fn all() -> [Self; 30] {
        [
            Self::P228,
            Self::Glock,
            Self::Scout,
            Self::HeGrenade,
            Self::XM1014,
            Self::C4,
            Self::Mac10,
            Self::Aug,
            Self::SmokeGrenade,
            Self::Elite,
            Self::FiveSeven,
            Self::Ump45,
            Self::Sg550,
            Self::Galil,
            Self::Famas,
            Self::USP45,
            Self::Glock18,
            Self::AWP,
            Self::Mp5,
            Self::M249,
            Self::M3,
            Self::M4A1,
            Self::Tmp,
            Self::G3SG1,
            Self::Flashbang,
            Self::Deagle,
            Self::Sg552,
            Self::AK47,
            Self::Knife,
            Self::P90,
        ]
    }

    /// Returns whether this weapon may be voluntarily dropped (`G` key) per ReGameDLL `CanDrop()`.
    #[must_use]
    pub const fn is_droppable(self) -> bool {
        matches!(
            self.slot(),
            WeaponSlot::Primary | WeaponSlot::Secondary | WeaponSlot::C4
        )
    }

    /// Returns `true` if this weapon draws the HUD crosshair on screen.
    /// In Counter-Strike 1.6 (`ammo.cpp:1736`), only sniper rifles do NOT draw a normal crosshair.
    /// Knives, grenades, and C4 all draw crosshairs.
    #[must_use]
    pub const fn has_crosshair(self) -> bool {
        !matches!(self, Self::AWP | Self::Scout | Self::G3SG1 | Self::Sg550)
    }

    /// Returns `true` if empty dryfire uses `"weapons/dryfire_rifle.wav"` (primary) vs `"weapons/dryfire_pistol.wav"`.
    #[must_use]
    pub const fn is_rifle_dryfire(self) -> bool {
        matches!(self.slot(), WeaponSlot::Primary)
    }

    /// Returns the FOV zoom ring (`[90, ...]`) cycled by SecondaryAttack (`IN_ATTACK2`).
    #[must_use]
    pub const fn zoom_fovs(self) -> &'static [u8] {
        match self {
            Self::AWP => &[90, 40, 10],
            Self::Scout | Self::G3SG1 | Self::Sg550 => &[90, 40, 15],
            Self::Aug | Self::Sg552 => &[90, 55],
            _ => &[90],
        }
    }

    /// Returns `true` if this weapon uses the full-screen sniper scope black mask & reticle when FOV <= 40.
    #[must_use]
    pub const fn is_sniper_scope(self) -> bool {
        matches!(self, Self::AWP | Self::Scout | Self::G3SG1 | Self::Sg550)
    }

    /// Returns `true` if secondary zoom plays `"weapons/zoom.wav"`.
    #[must_use]
    pub const fn has_zoom_sound(self) -> bool {
        self.is_sniper_scope()
    }

    /// Returns `true` if scope unzooms and automatically resumes zoom after firing bolt-action (`m_bResumeZoom`).
    #[must_use]
    pub const fn resumes_zoom_after_shot(self) -> bool {
        matches!(self, Self::AWP | Self::Scout)
    }

    /// Returns movement `maxspeed` when zoomed (or standard `max_speed` when unzoomed / no zoom penalty).
    #[must_use]
    pub fn effective_max_speed(self, fov: u8) -> f32 {
        if fov >= 90 {
            return get_weapon_def(self).max_speed;
        }
        match self {
            Self::AWP | Self::G3SG1 | Self::Sg550 => 150.0,
            Self::Scout => 220.0,
            Self::Sg552 => 200.0,
            _ => get_weapon_def(self).max_speed,
        }
    }

    /// Returns `cycle_time` factoring AUG/SG552 zoomed fire-rate slowdown and Glock18/FAMAS burst vs auto.
    #[must_use]
    pub fn effective_cycle_time(self, fov: u8, burst_mode: bool) -> f32 {
        match self {
            Self::Aug | Self::Sg552 => {
                if fov < 90 {
                    0.135
                } else {
                    0.0825
                }
            }
            Self::Famas => {
                if burst_mode {
                    0.55
                } else {
                    0.0825
                }
            }
            Self::Glock | Self::Glock18 => {
                if burst_mode {
                    0.50
                } else {
                    0.15
                }
            }
            _ => get_weapon_def(self).cycle_time,
        }
    }

    /// Returns per-shot damage accounting for USP/M4A1 silencer and FAMAS first burst shot (`34.0` vs `30.0`).
    #[must_use]
    pub fn effective_damage(self, silenced: bool, burst_mode: bool, burst_shot_index: u8) -> f32 {
        match self {
            Self::USP45 => {
                if silenced {
                    30.0
                } else {
                    34.0
                }
            }
            Self::M4A1 => {
                if silenced {
                    33.0
                } else {
                    32.0
                }
            }
            Self::Famas => {
                if burst_mode && burst_shot_index == 0 {
                    34.0
                } else {
                    30.0
                }
            }
            _ => get_weapon_def(self).damage,
        }
    }

    /// Returns ReGameDLL `flRangeModifier` per 500u distance attenuation (`dmg *= pow(range_mod, dist/500)`).
    #[must_use]
    pub const fn range_modifier(self, silenced: bool, burst_follow_up: bool) -> f32 {
        match self {
            Self::AWP => 0.99,
            Self::Scout | Self::G3SG1 | Self::Sg550 | Self::AK47 => 0.98,
            Self::M249 => 0.97,
            Self::M4A1 => {
                if silenced {
                    0.95
                } else {
                    0.97
                }
            }
            Self::Aug | Self::Famas | Self::Galil => 0.96,
            Self::Sg552 => 0.955,
            Self::P90 | Self::Tmp | Self::Mp5 | Self::Mac10 | Self::Ump45 => 0.84,
            Self::Deagle => 0.81,
            Self::P228 | Self::FiveSeven | Self::Elite => 0.80,
            Self::USP45 => 0.79,
            Self::Glock | Self::Glock18 => {
                if burst_follow_up {
                    0.90
                } else {
                    0.75
                }
            }
            Self::M3 | Self::XM1014 => 0.70,
            _ => 1.0,
        }
    }

    /// Returns number of pellets fired per trigger pull (`9` for M3, `6` for XM1014, `1` for others).
    #[must_use]
    pub const fn pellet_count(self) -> u32 {
        match self {
            Self::M3 => 9,
            Self::XM1014 => 6,
            _ => 1,
        }
    }
}

/// Posture and firing-history context for ReGameDLL `ComputeSpread` calculation.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpreadContext {
    pub weapon: WeaponId,
    pub silenced: bool,
    pub burst_mode: bool,
    pub shots_fired: u32,
    pub prev_accuracy: f32,
    pub time_since_last_fire: f32,
    pub has_fired_before: bool,
    pub speed_2d: f32,
    pub on_ground: bool,
    pub is_ducking: bool,
    pub fov: u8,
}

/// Result of spread calculation: exact spread cone (`flSpread`) and next weapon `m_flAccuracy`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpreadResult {
    pub spread: f32,
    pub next_accuracy: f32,
}

/// Computes canonical ReGameDLL initial `m_flAccuracy` after weapon deploy / reload.
#[must_use]
pub const fn initial_weapon_accuracy(weapon: WeaponId) -> f32 {
    match weapon {
        WeaponId::USP45 => 0.92,
        WeaponId::Glock
        | WeaponId::Glock18
        | WeaponId::Deagle
        | WeaponId::P228
        | WeaponId::FiveSeven
        | WeaponId::Elite => 0.90,
        WeaponId::G3SG1 | WeaponId::Sg550 => 0.98,
        WeaponId::AK47
        | WeaponId::M4A1
        | WeaponId::Aug
        | WeaponId::Sg552
        | WeaponId::Famas
        | WeaponId::Galil
        | WeaponId::M249 => 0.20,
        _ => 0.0,
    }
}

/// Computes exact ReGameDLL `flSpread` and updated `m_flAccuracy` across all CS 1.6 weapons.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn compute_weapon_spread(ctx: SpreadContext) -> SpreadResult {
    let s = (ctx.shots_fired.saturating_add(1)) as f32;
    let s3 = s * s * s;

    match ctx.weapon {
        WeaponId::AWP => {
            let mut spread = if !ctx.on_ground {
                0.85
            } else if ctx.speed_2d > 140.0 {
                0.25
            } else if ctx.speed_2d > 10.0 {
                0.10
            } else if ctx.is_ducking {
                0.0
            } else {
                0.001
            };
            if ctx.fov >= 90 {
                spread += 0.08;
            }
            SpreadResult {
                spread,
                next_accuracy: 0.0,
            }
        }
        WeaponId::Scout => {
            let mut spread = if !ctx.on_ground {
                0.20
            } else if ctx.speed_2d > 170.0 {
                0.075
            } else if ctx.is_ducking {
                0.0
            } else {
                0.007
            };
            if ctx.fov >= 90 {
                spread += 0.025;
            }
            SpreadResult {
                spread,
                next_accuracy: 0.0,
            }
        }
        WeaponId::G3SG1 => {
            let acc = if ctx.has_fired_before {
                (ctx.time_since_last_fire * 0.3 + 0.55).min(0.98)
            } else {
                0.98
            };
            let base = if !ctx.on_ground {
                0.45
            } else if ctx.speed_2d > 0.0 {
                0.15
            } else if ctx.is_ducking {
                0.035
            } else {
                0.055
            };
            let mut spread = (1.0 - acc) * base;
            if ctx.fov >= 90 {
                spread += 0.025;
            }
            SpreadResult {
                spread,
                next_accuracy: acc,
            }
        }
        WeaponId::Sg550 => {
            let acc = if ctx.has_fired_before {
                (ctx.time_since_last_fire * 0.35 + 0.65).min(0.98)
            } else {
                0.90
            };
            let mut spread = if !ctx.on_ground {
                0.45 * (1.0 - acc)
            } else if ctx.speed_2d > 0.0 {
                0.15
            } else if ctx.is_ducking {
                0.04 * (1.0 - acc)
            } else {
                0.05 * (1.0 - acc)
            };
            if ctx.fov >= 90 {
                spread += 0.025;
            }
            SpreadResult {
                spread,
                next_accuracy: acc,
            }
        }
        WeaponId::AK47 => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let spread = if !ctx.on_ground {
                0.04 + 0.4 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.04 + 0.07 * cur_acc
            } else {
                0.0275 * cur_acc
            };
            let next_accuracy = (s3 / 200.0 + 0.35).min(1.25);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::M4A1 => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let spread = if !ctx.on_ground {
                0.035 + 0.4 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.035 + 0.07 * cur_acc
            } else if ctx.silenced {
                0.025 * cur_acc
            } else {
                0.02 * cur_acc
            };
            let next_accuracy = (s3 / 220.0 + 0.30).min(1.0);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::Aug => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let spread = if !ctx.on_ground {
                0.035 + 0.4 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.035 + 0.07 * cur_acc
            } else {
                0.02 * cur_acc
            };
            let next_accuracy = (s3 / 215.0 + 0.30).min(1.0);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::Sg552 => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let spread = if !ctx.on_ground {
                0.035 + 0.45 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.035 + 0.075 * cur_acc
            } else {
                0.02 * cur_acc
            };
            let next_accuracy = (s3 / 220.0 + 0.30).min(1.0);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::Famas => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let mut spread = if !ctx.on_ground {
                0.030 + 0.3 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.030 + 0.07 * cur_acc
            } else {
                0.02 * cur_acc
            };
            if !ctx.burst_mode {
                spread += 0.01;
            }
            let next_accuracy = (s3 / 215.0 + 0.30).min(1.0);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::Galil => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let spread = if !ctx.on_ground {
                0.04 + 0.3 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.04 + 0.07 * cur_acc
            } else {
                0.0375 * cur_acc
            };
            let next_accuracy = (s3 / 200.0 + 0.35).min(1.25);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::M249 => {
            let cur_acc = if ctx.shots_fired == 0 {
                0.20
            } else {
                ctx.prev_accuracy
            };
            let spread = if !ctx.on_ground {
                0.045 + 0.5 * cur_acc
            } else if ctx.speed_2d > 140.0 {
                0.045 + 0.095 * cur_acc
            } else {
                0.03 * cur_acc
            };
            let next_accuracy = (s3 / 175.0 + 0.40).min(0.90);
            SpreadResult {
                spread,
                next_accuracy,
            }
        }
        WeaponId::USP45 => {
            let cur_acc = if ctx.has_fired_before {
                (ctx.prev_accuracy - (0.30 - ctx.time_since_last_fire) * 0.275).clamp(0.60, 0.92)
            } else {
                0.92
            };
            let base = if ctx.silenced {
                if !ctx.on_ground {
                    1.3
                } else if ctx.speed_2d > 0.0 {
                    0.25
                } else if ctx.is_ducking {
                    0.125
                } else {
                    0.15
                }
            } else if !ctx.on_ground {
                1.2
            } else if ctx.speed_2d > 0.0 {
                0.225
            } else if ctx.is_ducking {
                0.08
            } else {
                0.10
            };
            SpreadResult {
                spread: base * (1.0 - cur_acc),
                next_accuracy: cur_acc,
            }
        }
        WeaponId::Glock | WeaponId::Glock18 => {
            let cur_acc = if ctx.has_fired_before {
                (ctx.prev_accuracy - (0.325 - ctx.time_since_last_fire) * 0.275).clamp(0.60, 0.90)
            } else {
                0.90
            };
            let base = if ctx.burst_mode {
                if !ctx.on_ground {
                    1.2
                } else if ctx.speed_2d > 0.0 {
                    0.185
                } else if ctx.is_ducking {
                    0.095
                } else {
                    0.30
                }
            } else if !ctx.on_ground {
                1.0
            } else if ctx.speed_2d > 0.0 {
                0.165
            } else if ctx.is_ducking {
                0.075
            } else {
                0.10
            };
            SpreadResult {
                spread: base * (1.0 - cur_acc),
                next_accuracy: cur_acc,
            }
        }
        WeaponId::Deagle => {
            let cur_acc = if ctx.has_fired_before {
                (ctx.prev_accuracy - (0.40 - ctx.time_since_last_fire) * 0.35).clamp(0.55, 0.90)
            } else {
                0.90
            };
            let base = if !ctx.on_ground {
                1.5
            } else if ctx.speed_2d > 0.0 {
                0.25
            } else if ctx.is_ducking {
                0.115
            } else {
                0.13
            };
            SpreadResult {
                spread: base * (1.0 - cur_acc),
                next_accuracy: cur_acc,
            }
        }
        WeaponId::P228 | WeaponId::FiveSeven | WeaponId::Elite => {
            let cur_acc = if ctx.has_fired_before {
                (ctx.prev_accuracy - (0.325 - ctx.time_since_last_fire) * 0.275).clamp(0.60, 0.90)
            } else {
                0.90
            };
            let base = if !ctx.on_ground {
                1.5
            } else if ctx.speed_2d > 0.0 {
                0.255
            } else if ctx.is_ducking {
                0.075
            } else {
                0.15
            };
            SpreadResult {
                spread: base * (1.0 - cur_acc),
                next_accuracy: cur_acc,
            }
        }
        WeaponId::Mp5 | WeaponId::Mac10 | WeaponId::Tmp | WeaponId::Ump45 | WeaponId::P90 => {
            let spread = if ctx.on_ground {
                0.04 * (0.2 + s * 0.06).min(1.0)
            } else {
                0.20 * (0.2 + s * 0.08).min(1.0)
            };
            SpreadResult {
                spread,
                next_accuracy: (s3 / 210.0).min(1.0),
            }
        }
        WeaponId::M3 | WeaponId::XM1014 => SpreadResult {
            spread: 0.0675,
            next_accuracy: 0.0,
        },
        _ => SpreadResult {
            spread: 0.0,
            next_accuracy: 0.0,
        },
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
    pub uses_clip: bool,
    pub caliber: AmmoCaliber,
    pub max_speed: f32,
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
    pub recoil: RecoilPostures,
    pub is_automatic: bool,
    pub penetration: u32,
    pub penetration_power: f32,
    pub penetration_distance: f32,
}

const fn no_recoil() -> RecoilPostures {
    RecoilPostures::uniform(KickBackParams::uniform_unbounded(0.0))
}

/// Returns the authoritative weapon definition matching ReGameDLL.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn get_weapon_def(id: WeaponId) -> WeaponDef {
    match id {
        WeaponId::P228 => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "SIG P228",
                damage: 32.0,
                cycle_time: 0.15,
                clip_size: 13,
                max_reserve: 52,
                uses_clip: true,
                caliber: AmmoCaliber::Sig357,
                max_speed: 250.0,
                reload_time: 2.7,
                v_model: "models/v_p228.mdl",
                p_model: "models/p_p228.mdl",
                anim_extension: "onehanded",
                shoot_sound: "weapons/p228-1.wav",
                reload_sound: "weapons/p228_clipout.wav",
                deploy_anim: 6,
                shoot_anim: 1,
                reload_anim: 5,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: false,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::Glock | WeaponId::Glock18 => WeaponDef {
            name: "Glock 18",
            damage: 25.0,
            cycle_time: 0.15,
            clip_size: 20,
            max_reserve: 120,
            uses_clip: true,
            caliber: AmmoCaliber::Mm9,
            max_speed: 250.0,
            reload_time: 2.2,
            v_model: "models/v_glock18.mdl",
            p_model: "models/p_glock18.mdl",
            anim_extension: "onehanded",
            shoot_sound: "weapons/glock18-2.wav",
            reload_sound: "weapons/glock18_clipout.wav",
            deploy_anim: 8,
            shoot_anim: 5,
            reload_anim: 7,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            up_max: 0.0,
            lateral_max: 0.0,
            recoil: no_recoil(),
            is_automatic: false,
            penetration: 1,
            penetration_power: 100.0,
            penetration_distance: 4096.0,
        },
        WeaponId::Scout => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "Steyr Scout",
                damage: 75.0,
                cycle_time: 1.25,
                clip_size: 10,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato762,
                max_speed: 260.0,
                reload_time: 2.0,
                v_model: "models/v_scout.mdl",
                p_model: "models/p_scout.mdl",
                anim_extension: "rifle",
                shoot_sound: "weapons/scout_fire-1.wav",
                reload_sound: "weapons/scout_clipout.wav",
                deploy_anim: 4,
                shoot_anim: 1,
                reload_anim: 3,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: false,
                penetration: 3,
                penetration_power: 250.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::HeGrenade => WeaponDef {
            name: "HE Grenade",
            damage: 100.0,
            cycle_time: 0.5,
            clip_size: 1,
            max_reserve: 1,
            uses_clip: false,
            caliber: AmmoCaliber::HeGrenade,
            max_speed: 250.0,
            reload_time: 0.0,
            v_model: "models/v_hegrenade.mdl",
            p_model: "models/p_hegrenade.mdl",
            anim_extension: "grenade",
            shoot_sound: "weapons/pinpull.wav",
            reload_sound: "",
            deploy_anim: 3,
            shoot_anim: 1,
            reload_anim: 0,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            up_max: 0.0,
            lateral_max: 0.0,
            recoil: no_recoil(),
            is_automatic: false,
            penetration: 0,
            penetration_power: 0.0,
            penetration_distance: 0.0,
        },
        WeaponId::XM1014 => {
            let r = RecoilPostures {
                moving: KickBackParams::uniform_unbounded(4.0),
                air: KickBackParams::uniform_unbounded(8.5),
                ducking: KickBackParams::uniform_unbounded(4.0),
                standing: KickBackParams::uniform_unbounded(4.0),
            };
            WeaponDef {
                name: "Benelli XM1014",
                damage: 20.0,
                cycle_time: 0.25,
                clip_size: 7,
                max_reserve: 32,
                uses_clip: true,
                caliber: AmmoCaliber::Buckshot12g,
                max_speed: 240.0,
                reload_time: 0.5,
                v_model: "models/v_xm1014.mdl",
                p_model: "models/p_xm1014.mdl",
                anim_extension: "m249",
                shoot_sound: "weapons/xm1014-1.wav",
                reload_sound: "weapons/reload1.wav",
                deploy_anim: 6,
                shoot_anim: 1,
                reload_anim: 3,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: true,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 3048.0,
            }
        }
        WeaponId::C4 => WeaponDef {
            name: "C4 Explosive",
            damage: 500.0,
            cycle_time: 1.0,
            clip_size: 1,
            max_reserve: 1,
            uses_clip: false,
            caliber: AmmoCaliber::C4,
            max_speed: 250.0,
            reload_time: 0.0,
            v_model: "models/v_c4.mdl",
            p_model: "models/p_c4.mdl",
            anim_extension: "c4",
            shoot_sound: "weapons/c4_plant.wav",
            reload_sound: "",
            deploy_anim: 1,
            shoot_anim: 3,
            reload_anim: 0,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            up_max: 0.0,
            lateral_max: 0.0,
            recoil: no_recoil(),
            is_automatic: false,
            penetration: 0,
            penetration_power: 0.0,
            penetration_distance: 0.0,
        },
        WeaponId::Mac10 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(0.9, 0.45, 0.25, 0.035, 3.5, 2.75, 7),
                air: KickBackParams::new(1.3, 0.55, 0.4, 0.05, 4.75, 3.75, 5),
                ducking: KickBackParams::new(0.75, 0.4, 0.175, 0.03, 2.75, 2.5, 10),
                standing: KickBackParams::new(0.775, 0.425, 0.2, 0.03, 3.0, 2.75, 9),
            };
            WeaponDef {
                name: "Ingram MAC-10",
                damage: 29.0,
                cycle_time: 0.07,
                clip_size: 30,
                max_reserve: 100,
                uses_clip: true,
                caliber: AmmoCaliber::Acp45,
                max_speed: 250.0,
                reload_time: 3.15,
                v_model: "models/v_mac10.mdl",
                p_model: "models/p_mac10.mdl",
                anim_extension: "onehanded",
                shoot_sound: "weapons/mac10-1.wav",
                reload_sound: "weapons/mac10_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::Aug => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.0, 0.45, 0.275, 0.05, 4.0, 2.5, 7),
                air: KickBackParams::new(1.25, 0.45, 0.22, 0.18, 5.5, 4.0, 5),
                ducking: KickBackParams::new(0.575, 0.325, 0.2, 0.011, 3.25, 2.0, 8),
                standing: KickBackParams::new(0.625, 0.375, 0.25, 0.0125, 3.5, 2.25, 8),
            };
            WeaponDef {
                name: "Steyr AUG A1",
                damage: 32.0,
                cycle_time: 0.0825,
                clip_size: 30,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 240.0,
                reload_time: 3.3,
                v_model: "models/v_aug.mdl",
                p_model: "models/p_aug.mdl",
                anim_extension: "carbine",
                shoot_sound: "weapons/aug-1.wav",
                reload_sound: "weapons/aug_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::SmokeGrenade => WeaponDef {
            name: "Smoke Grenade",
            damage: 0.0,
            cycle_time: 0.5,
            clip_size: 1,
            max_reserve: 1,
            uses_clip: false,
            caliber: AmmoCaliber::SmokeGrenade,
            max_speed: 250.0,
            reload_time: 0.0,
            v_model: "models/v_smokegrenade.mdl",
            p_model: "models/p_smokegrenade.mdl",
            anim_extension: "grenade",
            shoot_sound: "weapons/pinpull.wav",
            reload_sound: "",
            deploy_anim: 3,
            shoot_anim: 1,
            reload_anim: 0,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            up_max: 0.0,
            lateral_max: 0.0,
            recoil: no_recoil(),
            is_automatic: false,
            penetration: 0,
            penetration_power: 0.0,
            penetration_distance: 0.0,
        },
        WeaponId::Elite => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "Dual Berettas",
                damage: 36.0,
                cycle_time: 0.12,
                clip_size: 30,
                max_reserve: 120,
                uses_clip: true,
                caliber: AmmoCaliber::Mm9,
                max_speed: 250.0,
                reload_time: 4.5,
                v_model: "models/v_elite.mdl",
                p_model: "models/p_elite.mdl",
                anim_extension: "dualpistols",
                shoot_sound: "weapons/elite_fire.wav",
                reload_sound: "weapons/elite_reloadstart.wav",
                deploy_anim: 15,
                shoot_anim: 2,
                reload_anim: 14,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: false,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::FiveSeven => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "FN Five-SeveN",
                damage: 20.0,
                cycle_time: 0.15,
                clip_size: 20,
                max_reserve: 100,
                uses_clip: true,
                caliber: AmmoCaliber::Mm57,
                max_speed: 250.0,
                reload_time: 2.7,
                v_model: "models/v_fiveseven.mdl",
                p_model: "models/p_fiveseven.mdl",
                anim_extension: "onehanded",
                shoot_sound: "weapons/fiveseven-1.wav",
                reload_sound: "weapons/fiveseven_clipout.wav",
                deploy_anim: 5,
                shoot_anim: 1,
                reload_anim: 4,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: false,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::Ump45 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(0.55, 0.3, 0.225, 0.03, 3.5, 2.5, 10),
                air: KickBackParams::new(0.125, 0.65, 0.55, 0.0475, 5.5, 4.0, 10),
                ducking: KickBackParams::new(0.25, 0.175, 0.125, 0.02, 2.25, 1.25, 10),
                standing: KickBackParams::new(0.275, 0.2, 0.15, 0.0225, 2.5, 1.5, 10),
            };
            WeaponDef {
                name: "H&K UMP45",
                damage: 30.0,
                cycle_time: 0.10,
                clip_size: 25,
                max_reserve: 100,
                uses_clip: true,
                caliber: AmmoCaliber::Acp45,
                max_speed: 250.0,
                reload_time: 3.5,
                v_model: "models/v_ump45.mdl",
                p_model: "models/p_ump45.mdl",
                anim_extension: "carbine",
                shoot_sound: "weapons/ump45-1.wav",
                reload_sound: "weapons/ump45_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::Sg550 => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(1.25));
            WeaponDef {
                name: "Sig SG-550 Sniper",
                damage: 70.0,
                cycle_time: 0.25,
                clip_size: 30,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 210.0,
                reload_time: 3.35,
                v_model: "models/v_sg550.mdl",
                p_model: "models/p_sg550.mdl",
                anim_extension: "rifle",
                shoot_sound: "weapons/sg550-1.wav",
                reload_sound: "weapons/sg550_clipout.wav",
                deploy_anim: 4,
                shoot_anim: 1,
                reload_anim: 3,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: true,
                penetration: 3,
                penetration_power: 250.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::Galil => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.0, 0.45, 0.28, 0.045, 3.75, 3.0, 7),
                air: KickBackParams::new(1.2, 0.5, 0.23, 0.15, 5.5, 3.5, 6),
                ducking: KickBackParams::new(0.6, 0.3, 0.2, 0.0125, 3.25, 2.0, 7),
                standing: KickBackParams::new(0.65, 0.35, 0.25, 0.015, 3.5, 2.25, 7),
            };
            WeaponDef {
                name: "IMI Galil",
                damage: 30.0,
                cycle_time: 0.088,
                clip_size: 35,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 240.0,
                reload_time: 2.45,
                v_model: "models/v_galil.mdl",
                p_model: "models/p_galil.mdl",
                anim_extension: "ak47",
                shoot_sound: "weapons/galil-1.wav",
                reload_sound: "weapons/galil_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::Famas => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.0, 0.45, 0.275, 0.05, 4.0, 2.5, 7),
                air: KickBackParams::new(1.25, 0.45, 0.22, 0.18, 5.5, 4.0, 5),
                ducking: KickBackParams::new(0.575, 0.325, 0.2, 0.011, 3.25, 2.0, 8),
                standing: KickBackParams::new(0.625, 0.375, 0.25, 0.0125, 3.5, 2.25, 8),
            };
            WeaponDef {
                name: "FAMAS",
                damage: 30.0,
                cycle_time: 0.0825,
                clip_size: 25,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 240.0,
                reload_time: 3.3,
                v_model: "models/v_famas.mdl",
                p_model: "models/p_famas.mdl",
                anim_extension: "carbine",
                shoot_sound: "weapons/famas-1.wav",
                reload_sound: "weapons/famas_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::USP45 => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "H&K USP .45",
                damage: 34.0,
                cycle_time: 0.15,
                clip_size: 12,
                max_reserve: 100,
                uses_clip: true,
                caliber: AmmoCaliber::Acp45,
                max_speed: 250.0,
                reload_time: 2.7,
                v_model: "models/v_usp.mdl",
                p_model: "models/p_usp.mdl",
                anim_extension: "onehanded",
                shoot_sound: "weapons/usp_unsil-1.wav",
                reload_sound: "weapons/usp_clipout.wav",
                deploy_anim: 14,
                shoot_anim: 9,
                reload_anim: 13,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: false,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::AWP => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "AWP",
                damage: 115.0,
                cycle_time: 1.45,
                clip_size: 10,
                max_reserve: 30,
                uses_clip: true,
                caliber: AmmoCaliber::Magnum338,
                max_speed: 210.0,
                reload_time: 2.5,
                v_model: "models/v_awp.mdl",
                p_model: "models/p_awp.mdl",
                anim_extension: "rifle",
                shoot_sound: "weapons/awp1.wav",
                reload_sound: "weapons/awp_clipout.wav",
                deploy_anim: 5,
                shoot_anim: 1,
                reload_anim: 4,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: false,
                penetration: 3,
                penetration_power: 300.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::Mp5 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(0.5, 0.275, 0.2, 0.03, 3.0, 2.0, 10),
                air: KickBackParams::new(0.9, 0.475, 0.35, 0.0425, 5.0, 3.0, 6),
                ducking: KickBackParams::new(0.225, 0.15, 0.1, 0.015, 2.0, 1.0, 10),
                standing: KickBackParams::new(0.25, 0.175, 0.125, 0.02, 2.25, 1.25, 10),
            };
            WeaponDef {
                name: "H&K MP5-Navy",
                damage: 26.0,
                cycle_time: 0.075,
                clip_size: 30,
                max_reserve: 120,
                uses_clip: true,
                caliber: AmmoCaliber::Mm9,
                max_speed: 250.0,
                reload_time: 2.63,
                v_model: "models/v_mp5.mdl",
                p_model: "models/p_mp5.mdl",
                anim_extension: "mp5",
                shoot_sound: "weapons/mp5-1.wav",
                reload_sound: "weapons/mp5_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::M249 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.1, 0.5, 0.3, 0.06, 4.0, 3.0, 8),
                air: KickBackParams::new(1.8, 0.65, 0.45, 0.125, 5.0, 3.5, 8),
                ducking: KickBackParams::new(0.75, 0.325, 0.25, 0.025, 3.5, 2.5, 9),
                standing: KickBackParams::new(0.8, 0.35, 0.3, 0.03, 3.75, 3.0, 9),
            };
            WeaponDef {
                name: "FN M249 Para",
                damage: 32.0,
                cycle_time: 0.08,
                clip_size: 100,
                max_reserve: 200,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 220.0,
                reload_time: 4.7,
                v_model: "models/v_m249.mdl",
                p_model: "models/p_m249.mdl",
                anim_extension: "m249",
                shoot_sound: "weapons/m249-1.wav",
                reload_sound: "weapons/m249_boxout.wav",
                deploy_anim: 4,
                shoot_anim: 1,
                reload_anim: 3,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::M3 => {
            let r = RecoilPostures {
                moving: KickBackParams::uniform_unbounded(5.0),
                air: KickBackParams::uniform_unbounded(9.5),
                ducking: KickBackParams::uniform_unbounded(5.0),
                standing: KickBackParams::uniform_unbounded(5.0),
            };
            WeaponDef {
                name: "Benelli M3 Super90",
                damage: 20.0,
                cycle_time: 0.88,
                clip_size: 8,
                max_reserve: 32,
                uses_clip: true,
                caliber: AmmoCaliber::Buckshot12g,
                max_speed: 230.0,
                reload_time: 0.5,
                v_model: "models/v_m3.mdl",
                p_model: "models/p_m3.mdl",
                anim_extension: "shotgun",
                shoot_sound: "weapons/m3-1.wav",
                reload_sound: "weapons/reload1.wav",
                deploy_anim: 6,
                shoot_anim: 1,
                reload_anim: 3,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: false,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 3048.0,
            }
        }
        WeaponId::M4A1 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.0, 0.45, 0.28, 0.045, 3.75, 3.0, 7),
                air: KickBackParams::new(1.2, 0.5, 0.23, 0.15, 5.5, 3.5, 6),
                ducking: KickBackParams::new(0.6, 0.3, 0.2, 0.0125, 3.25, 2.0, 7),
                standing: KickBackParams::new(0.65, 0.35, 0.25, 0.015, 3.5, 2.25, 7),
            };
            WeaponDef {
                name: "Colt M4A1",
                damage: 32.0,
                cycle_time: 0.0875,
                clip_size: 30,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 230.0,
                reload_time: 3.05,
                v_model: "models/v_m4a1.mdl",
                p_model: "models/p_m4a1.mdl",
                anim_extension: "rifle",
                shoot_sound: "weapons/m4a1_unsil-1.wav",
                reload_sound: "weapons/m4a1_clipout.wav",
                deploy_anim: 12,
                shoot_anim: 8,
                reload_anim: 11,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::Tmp => {
            let r = RecoilPostures {
                moving: KickBackParams::new(0.8, 0.4, 0.2, 0.03, 3.0, 2.5, 7),
                air: KickBackParams::new(1.1, 0.5, 0.35, 0.045, 4.5, 3.5, 6),
                ducking: KickBackParams::new(0.7, 0.35, 0.125, 0.025, 2.5, 2.0, 10),
                standing: KickBackParams::new(0.725, 0.375, 0.15, 0.025, 2.75, 2.25, 9),
            };
            WeaponDef {
                name: "Steyr Tactical MP",
                damage: 20.0,
                cycle_time: 0.07,
                clip_size: 30,
                max_reserve: 120,
                uses_clip: true,
                caliber: AmmoCaliber::Mm9,
                max_speed: 250.0,
                reload_time: 2.12,
                v_model: "models/v_tmp.mdl",
                p_model: "models/p_tmp.mdl",
                anim_extension: "onehanded",
                shoot_sound: "weapons/tmp-1.wav",
                reload_sound: "weapons/tmp_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::G3SG1 => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(1.25));
            WeaponDef {
                name: "H&K G3SG1 Sniper",
                damage: 80.0,
                cycle_time: 0.25,
                clip_size: 20,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato762,
                max_speed: 210.0,
                reload_time: 3.5,
                v_model: "models/v_g3sg1.mdl",
                p_model: "models/p_g3sg1.mdl",
                anim_extension: "mp5",
                shoot_sound: "weapons/g3sg1-1.wav",
                reload_sound: "weapons/g3sg1_clipout.wav",
                deploy_anim: 4,
                shoot_anim: 1,
                reload_anim: 3,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: true,
                penetration: 3,
                penetration_power: 250.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::Flashbang => WeaponDef {
            name: "Flashbang",
            damage: 0.0,
            cycle_time: 0.5,
            clip_size: 1,
            max_reserve: 2,
            uses_clip: false,
            caliber: AmmoCaliber::Flashbang,
            max_speed: 250.0,
            reload_time: 0.0,
            v_model: "models/v_flashbang.mdl",
            p_model: "models/p_flashbang.mdl",
            anim_extension: "grenade",
            shoot_sound: "weapons/pinpull.wav",
            reload_sound: "",
            deploy_anim: 3,
            shoot_anim: 1,
            reload_anim: 0,
            recoil_up: 0.0,
            recoil_lateral: 0.0,
            up_max: 0.0,
            lateral_max: 0.0,
            recoil: no_recoil(),
            is_automatic: false,
            penetration: 0,
            penetration_power: 0.0,
            penetration_distance: 0.0,
        },
        WeaponId::Deagle => {
            let r = RecoilPostures::uniform(KickBackParams::uniform_unbounded(2.0));
            WeaponDef {
                name: "Desert Eagle",
                damage: 54.0,
                cycle_time: 0.225,
                clip_size: 7,
                max_reserve: 35,
                uses_clip: true,
                caliber: AmmoCaliber::Ae50,
                max_speed: 250.0,
                reload_time: 2.2,
                v_model: "models/v_deagle.mdl",
                p_model: "models/p_deagle.mdl",
                anim_extension: "onehanded",
                shoot_sound: "weapons/deagle-1.wav",
                reload_sound: "weapons/de_clipout.wav",
                deploy_anim: 5,
                shoot_anim: 1,
                reload_anim: 4,
                recoil_up: r.standing.up_base,
                recoil_lateral: 0.0,
                up_max: 0.0,
                lateral_max: 0.0,
                recoil: r,
                is_automatic: false,
                penetration: 2,
                penetration_power: 150.0,
                penetration_distance: 4096.0,
            }
        }
        WeaponId::Sg552 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.0, 0.45, 0.28, 0.04, 4.25, 2.5, 7),
                air: KickBackParams::new(1.25, 0.45, 0.22, 0.18, 6.0, 4.0, 5),
                ducking: KickBackParams::new(0.6, 0.35, 0.2, 0.0125, 3.7, 2.0, 10),
                standing: KickBackParams::new(0.625, 0.375, 0.25, 0.0125, 4.0, 2.25, 9),
            };
            WeaponDef {
                name: "Sig SG-552 Commando",
                damage: 33.0,
                cycle_time: 0.0825,
                clip_size: 30,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato556,
                max_speed: 235.0,
                reload_time: 3.0,
                v_model: "models/v_sg552.mdl",
                p_model: "models/p_sg552.mdl",
                anim_extension: "mp5",
                shoot_sound: "weapons/sg552-1.wav",
                reload_sound: "weapons/sg552_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::AK47 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(1.5, 0.45, 0.225, 0.05, 6.5, 2.5, 7),
                air: KickBackParams::new(2.0, 1.0, 0.5, 0.35, 9.0, 6.0, 5),
                ducking: KickBackParams::new(0.9, 0.35, 0.15, 0.025, 5.5, 1.5, 9),
                standing: KickBackParams::new(1.0, 0.375, 0.175, 0.0375, 5.75, 1.75, 8),
            };
            WeaponDef {
                name: "AK-47",
                damage: 36.0,
                cycle_time: 0.095,
                clip_size: 30,
                max_reserve: 90,
                uses_clip: true,
                caliber: AmmoCaliber::Nato762,
                max_speed: 221.0,
                reload_time: 2.45,
                v_model: "models/v_ak47.mdl",
                p_model: "models/p_ak47.mdl",
                anim_extension: "ak47",
                shoot_sound: "weapons/ak47-1.wav",
                reload_sound: "weapons/ak47_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 2,
                penetration_power: 200.0,
                penetration_distance: 8192.0,
            }
        }
        WeaponId::Knife => WeaponDef {
            name: "Knife",
            damage: 25.0,
            cycle_time: 0.35,
            clip_size: 0,
            max_reserve: 0,
            uses_clip: false,
            caliber: AmmoCaliber::None,
            max_speed: 250.0,
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
            recoil: no_recoil(),
            is_automatic: false,
            penetration: 0,
            penetration_power: 0.0,
            penetration_distance: 0.0,
        },
        WeaponId::P90 => {
            let r = RecoilPostures {
                moving: KickBackParams::new(0.45, 0.3, 0.2, 0.0275, 4.0, 2.25, 7),
                air: KickBackParams::new(0.9, 0.45, 0.35, 0.04, 5.25, 3.5, 4),
                ducking: KickBackParams::new(0.275, 0.2, 0.125, 0.02, 3.0, 1.0, 9),
                standing: KickBackParams::new(0.3, 0.225, 0.125, 0.02, 3.25, 1.25, 8),
            };
            WeaponDef {
                name: "FN P90",
                damage: 21.0,
                cycle_time: 0.066,
                clip_size: 50,
                max_reserve: 100,
                uses_clip: true,
                caliber: AmmoCaliber::Mm57,
                max_speed: 245.0,
                reload_time: 3.4,
                v_model: "models/v_p90.mdl",
                p_model: "models/p_p90.mdl",
                anim_extension: "carbine",
                shoot_sound: "weapons/p90-1.wav",
                reload_sound: "weapons/p90_clipout.wav",
                deploy_anim: 2,
                shoot_anim: 3,
                reload_anim: 1,
                recoil_up: r.standing.up_base,
                recoil_lateral: r.standing.lateral_base,
                up_max: r.standing.up_max,
                lateral_max: r.standing.lateral_max,
                recoil: r,
                is_automatic: true,
                penetration: 1,
                penetration_power: 100.0,
                penetration_distance: 4096.0,
            }
        }
    }
}
