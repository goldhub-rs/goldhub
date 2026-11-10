//! # GoldHub Game CStrike
//!
//! Canonical implementation of Counter-Strike 1.6 game logic, weapon balance,
//! recoil tables, and round lifecycle rules.
//!
//! This crate depends exclusively on `goldhub-engine-api` and has ZERO dependencies
//! on Bevy or concrete rendering engines, ensuring pure portability and testability.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod hitbox;
pub mod recoil;
pub mod rules;
pub mod weapons;

pub use hitbox::{HitboxGroup, calculate_damage};
pub use recoil::PlayerRecoil;
pub use rules::{CsGameRules, CsTeam, RoundState};
pub use weapons::{WeaponDef, WeaponId, WeaponSlot, get_weapon_def};

#[cfg(test)]
mod tests {
    use super::*;
    use goldhub_engine_api::GameRules;

    #[test]
    fn test_weapon_defs_integrity() {
        let ak = get_weapon_def(WeaponId::AK47);
        assert_eq!(ak.name, "AK-47");
        assert_eq!(ak.clip_size, 30);
        assert!(ak.is_automatic);
        assert_eq!(ak.damage, 36.0);

        let knife = get_weapon_def(WeaponId::Knife);
        assert_eq!(knife.clip_size, 0);
        assert_eq!(knife.damage, 25.0);

        // Verify all 30 canonical weapons define non-empty names and valid slots
        let all_weapons = WeaponId::all();
        assert_eq!(all_weapons.len(), 30);
        for id in all_weapons {
            let def = get_weapon_def(id);
            assert!(!def.name.is_empty(), "Weapon {id:?} must have a name");
            assert!(!def.v_model.is_empty(), "Weapon {id:?} must have a v_model");
            assert!(!def.p_model.is_empty(), "Weapon {id:?} must have a p_model");
            assert!(
                !def.shoot_sound.is_empty(),
                "Weapon {id:?} must have a shoot sound"
            );
        }
    }

    #[test]
    fn test_recoil_decay_simulation() {
        let mut recoil = PlayerRecoil::new();
        recoil.apply_kickback(1.0, 0.5, 0.2, 0.1, 6.0, 2.0, 1, 0.8);
        assert!(recoil.punch_pitch > 0.9);
        assert!(recoil.punch_yaw > 0.4);

        recoil.decay(0.5);
        assert!(recoil.punch_pitch < 1.0);
    }

    #[test]
    fn test_hitbox_damage_calculation() {
        let (head_dmg, _) = calculate_damage(30.0, HitboxGroup::Head, 0.0, 0.5);
        assert_eq!(head_dmg, 120.0); // 30 * 4.0

        let (leg_dmg, _) = calculate_damage(30.0, HitboxGroup::LeftLeg, 100.0, 0.5);
        assert_eq!(leg_dmg, 22.5); // 30 * 0.75, unarmored leg
    }

    #[test]
    fn test_round_rules_state_machine() {
        let mut rules = CsGameRules::new(3.0, 100.0, false);
        assert_eq!(rules.state, RoundState::FreezeTime);
        assert!(rules.can_player_buy(1));

        rules.update_round(3.1);
        assert_eq!(rules.state, RoundState::RoundLive);
        assert!(rules.can_player_buy(1));

        // After buy time expires
        rules.update_round(20.0);
        assert!(!rules.can_player_buy(1));
    }
}
