//! Canonical CS 1.6 hitbox damage multipliers and armor penetration physics.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Hitbox spatial zones matching ReGameDLL `m_flHitbox`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum HitboxGroup {
    #[default]
    Generic = 0,
    Head = 1,
    Chest = 2,
    Stomach = 3,
    LeftArm = 4,
    RightArm = 5,
    LeftLeg = 6,
    RightLeg = 7,
}

impl HitboxGroup {
    /// Returns the canonical damage multiplier for this body area.
    #[must_use]
    pub const fn damage_multiplier(self) -> f32 {
        match self {
            Self::Head => 4.0,
            Self::Stomach => 1.25,
            Self::LeftLeg | Self::RightLeg => 0.75,
            Self::Generic | Self::Chest | Self::LeftArm | Self::RightArm => 1.0,
        }
    }
}

/// Computes authentic CS 1.6 bullet impact damage considering hitbox group and armor.
#[must_use]
pub fn calculate_damage(
    base_damage: f32,
    hitbox: HitboxGroup,
    armor_value: f32,
    armor_ratio: f32,
) -> (f32, f32) {
    let raw_damage = base_damage * hitbox.damage_multiplier();

    if armor_value > 0.0 && hitbox != HitboxGroup::LeftLeg && hitbox != HitboxGroup::RightLeg {
        // Armor ratio: fraction of damage that bypasses Kevlar vest
        let mut damage_dealt = raw_damage * (armor_ratio * 0.5);
        let mut damage_absorbed = (raw_damage - damage_dealt) * 0.5;

        if damage_absorbed > armor_value {
            damage_absorbed = armor_value;
            damage_dealt = raw_damage - damage_absorbed;
        }

        let new_armor = (armor_value - damage_absorbed).max(0.0);
        (damage_dealt, new_armor)
    } else {
        (raw_damage, armor_value)
    }
}
