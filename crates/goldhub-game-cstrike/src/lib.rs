//! # GoldHub Game CStrike
//!
//! Canonical implementation of Counter-Strike 1.6 game logic, weapon balance,
//! recoil tables, and round lifecycle rules.
//!
//! This crate depends exclusively on `goldhub-engine-api` and has ZERO dependencies
//! on Bevy or concrete rendering engines, ensuring pure portability and testability.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod hitbox;
pub mod localization;
pub mod recoil;
pub mod rules;
pub mod weapons;

pub use hitbox::{HitboxGroup, calculate_damage};
pub use localization::builtin_fallback;
#[cfg(feature = "std")]
pub use localization::{CsLocalizationTable, decode_resource_text};
pub use recoil::PlayerRecoil;
pub use rules::{
    CsGameRules, CsTeam, MapScenarioFlags, MapScenarioKind, RoundEndReason, RoundOutcome,
    RoundState, ScenarioTickCounts,
};
pub use weapons::{
    AmmoCaliber, KickBackParams, RecoilPostures, SpreadContext, SpreadResult, WeaponDef, WeaponId,
    WeaponSlot, compute_weapon_spread, get_weapon_def, initial_weapon_accuracy,
};

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
        recoil.apply_kickback(1.0, 0.5, 0.2, 0.1, 6.0, 2.0, 8, 1, 0.8);
        assert!(recoil.punch_pitch > 0.9);
        assert!(recoil.punch_yaw > 0.4);

        // Trigger a flip & unbounded pistol accumulation (USP has up_max = 0.0)
        let mut usp_recoil = PlayerRecoil::new();
        let usp_k = get_weapon_def(WeaponId::USP45).recoil.standing;
        assert_eq!(usp_k.up_base, 2.0);
        assert_eq!(usp_k.up_max, 0.0);
        usp_recoil.apply_kickback(
            usp_k.up_base,
            usp_k.lateral_base,
            usp_k.up_mod,
            usp_k.lateral_mod,
            usp_k.up_max,
            usp_k.lateral_max,
            usp_k.direction_change,
            1,
            0.5,
        );
        usp_recoil.apply_kickback(
            usp_k.up_base,
            usp_k.lateral_base,
            usp_k.up_mod,
            usp_k.lateral_mod,
            usp_k.up_max,
            usp_k.lateral_max,
            usp_k.direction_change,
            2,
            0.5,
        );
        assert_eq!(usp_recoil.punch_pitch, 4.0);

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

    #[test]
    fn test_weapon_spread_and_zoom_parity() {
        // AWP zoomed standing vs hipfire
        let awp_zoom = compute_weapon_spread(SpreadContext {
            weapon: WeaponId::AWP,
            silenced: false,
            burst_mode: false,
            shots_fired: 0,
            prev_accuracy: 0.0,
            time_since_last_fire: 0.0,
            has_fired_before: false,
            speed_2d: 0.0,
            on_ground: true,
            is_ducking: false,
            fov: 40,
        });
        let awp_hip = compute_weapon_spread(SpreadContext {
            weapon: WeaponId::AWP,
            silenced: false,
            burst_mode: false,
            shots_fired: 0,
            prev_accuracy: 0.0,
            time_since_last_fire: 0.0,
            has_fired_before: false,
            speed_2d: 0.0,
            on_ground: true,
            is_ducking: false,
            fov: 90,
        });
        assert!((awp_zoom.spread - 0.001).abs() < 1e-6);
        assert!((awp_hip.spread - 0.081).abs() < 1e-6);

        // First shot AK47 matches m_flAccuracy = 0.2 -> 0.0275 * 0.2 = 0.0055
        let ak1 = compute_weapon_spread(SpreadContext {
            weapon: WeaponId::AK47,
            silenced: false,
            burst_mode: false,
            shots_fired: 0,
            prev_accuracy: 0.2,
            time_since_last_fire: 0.0,
            has_fired_before: false,
            speed_2d: 0.0,
            on_ground: true,
            is_ducking: false,
            fov: 90,
        });
        assert!((ak1.spread - 0.0055).abs() < 1e-6);
        assert_eq!(WeaponId::AWP.zoom_fovs(), &[90, 40, 10]);
        assert!(!WeaponId::Knife.is_droppable());
        assert!(!WeaponId::HeGrenade.is_droppable());
        assert!(WeaponId::C4.is_droppable());
        assert_eq!(WeaponId::M3.pellet_count(), 9);
    }

    #[test]
    fn test_scenario_rules_and_localization() {
        let mut rules = CsGameRules::new(0.0, 105.0, false);
        rules.map_flags.has_bomb_target = true;
        rules.state = RoundState::RoundLive;

        // While C4 is planted, timer never expires into TargetSaved
        let expired_while_planted = rules.evaluate_scenario(
            120.0,
            ScenarioTickCounts {
                bomb_planted: true,
                ..Default::default()
            },
        );
        assert!(expired_while_planted.is_none());

        // Explosion awards Terrorists
        let blown = rules.evaluate_scenario(
            0.1,
            ScenarioTickCounts {
                bomb_planted: true,
                target_bombed: true,
                ..Default::default()
            },
        );
        assert_eq!(blown.map(|o| o.reason), Some(RoundEndReason::TargetBombed));
        assert_eq!(rules.t_score, 1);

        // UTF-16LE + VDF localization parser check
        let vdf = "\"lang\"\n{\n\"Tokens\"\n{\n\"Cstrike_TitlesTXT_Weapon_Cannot_Be_Dropped\" \"Это оружие нельзя бросить.\"\n}\n}";
        let mut table = CsLocalizationTable::new();
        let mut utf16 = vec![0xFFu8, 0xFE];
        for cu in vdf.encode_utf16() {
            utf16.extend_from_slice(&cu.to_le_bytes());
        }
        table.load_vdf_bytes(&utf16, false);
        assert_eq!(
            table.resolve("#Weapon_Cannot_Be_Dropped"),
            "Это оружие нельзя бросить."
        );
    }
}
