//! Buy Menu & Cash Transaction Engine for CS 1.6 GoldHub Runtime.
//!
//! Handles buy alias resolution (`buy ak47`, `autobuy`, `rebuy`), price deduction,
//! team validation, and player inventory updating.

use goldhub_game_cstrike::{
    BuyItem, BuyRejectionReason, CsGameRules, PlayerAccount, WeaponId, WeaponSlot,
    validate_purchase,
};

use crate::combat::weapon::{PlayerWeaponInventory, make_slot_data};
use crate::player::PlayerController;

/// Resolves standard CS 1.6 console/config buy aliases into [`BuyItem`].
#[must_use]
pub fn parse_buy_alias(alias: &str) -> Option<BuyItem> {
    let lower = alias.trim().to_ascii_lowercase();
    match lower.as_str() {
        "ak47" | "cv47" => Some(BuyItem::Weapon(WeaponId::AK47)),
        "m4a1" | "m4" => Some(BuyItem::Weapon(WeaponId::M4A1)),
        "awp" | "magnum" => Some(BuyItem::Weapon(WeaponId::AWP)),
        "deagle" | "nighthawk" => Some(BuyItem::Weapon(WeaponId::Deagle)),
        "usp" => Some(BuyItem::Weapon(WeaponId::USP45)),
        "glock" | "glock18" => Some(BuyItem::Weapon(WeaponId::Glock18)),
        "mp5" | "smg" | "mp5navy" => Some(BuyItem::Weapon(WeaponId::Mp5)),
        "p90" | "c90" => Some(BuyItem::Weapon(WeaponId::P90)),
        "galil" | "defender" => Some(BuyItem::Weapon(WeaponId::Galil)),
        "famas" | "clarion" => Some(BuyItem::Weapon(WeaponId::Famas)),
        "scout" => Some(BuyItem::Weapon(WeaponId::Scout)),
        "sg552" | "krieg552" => Some(BuyItem::Weapon(WeaponId::Sg552)),
        "aug" | "bullpup" => Some(BuyItem::Weapon(WeaponId::Aug)),
        "m3" | "12gauge" => Some(BuyItem::Weapon(WeaponId::M3)),
        "xm1014" | "autoshotgun" => Some(BuyItem::Weapon(WeaponId::XM1014)),
        "mac10" => Some(BuyItem::Weapon(WeaponId::Mac10)),
        "tmp" => Some(BuyItem::Weapon(WeaponId::Tmp)),
        "ump45" | "ump" => Some(BuyItem::Weapon(WeaponId::Ump45)),
        "m249" => Some(BuyItem::Weapon(WeaponId::M249)),
        "p228" | "228compact" => Some(BuyItem::Weapon(WeaponId::P228)),
        "elites" | "berettas" | "elite" => Some(BuyItem::Weapon(WeaponId::Elite)),
        "fn57" | "fiveseven" => Some(BuyItem::Weapon(WeaponId::FiveSeven)),
        "sg550" | "krieg550" => Some(BuyItem::Weapon(WeaponId::Sg550)),
        "g3sg1" | "d3au1" => Some(BuyItem::Weapon(WeaponId::G3SG1)),
        "vest" | "kevlar" => Some(BuyItem::Kevlar),
        "vesthelm" => Some(BuyItem::KevlarHelmet),
        "flash" | "flashbang" => Some(BuyItem::Flashbang),
        "hegren" | "hegrenade" => Some(BuyItem::HeGrenade),
        "sgren" | "smokegrenade" => Some(BuyItem::SmokeGrenade),
        "defuser" => Some(BuyItem::DefusalKit),
        "nvgs" | "nightvision" => Some(BuyItem::NightVision),
        "primammo" | "buyammo1" => Some(BuyItem::PrimaryAmmo),
        "secammo" | "buyammo2" => Some(BuyItem::SecondaryAmmo),
        _ => None,
    }
}

fn purchase_grenade(inv: &mut PlayerWeaponInventory, wid: WeaponId) -> Result<(), &'static str> {
    if let Some(pos) = inv.bucket_weapons[3].iter().position(|w| w.id == Some(wid)) {
        if wid == WeaponId::Flashbang && inv.bucket_weapons[3][pos].reserve == 0 {
            // CS 1.6: player can carry up to 2 Flashbangs (1 active + 1 reserve)
            inv.bucket_weapons[3][pos].reserve = 1;
            if inv.slot4.id == Some(WeaponId::Flashbang) {
                inv.slot4.reserve = 1;
            }
            inv.bucket_cursor[3] = pos;
            inv.pending_switch = Some((4, pos));
            Ok(())
        } else {
            Err("#Cannot_Carry_More")
        }
    } else {
        let mut slot_data = make_slot_data(wid);
        slot_data.reserve = 0;
        inv.slot4 = slot_data;
        inv.bucket_weapons[3].push(slot_data);
        let target_idx = inv.bucket_weapons[3].len().saturating_sub(1);
        inv.bucket_cursor[3] = target_idx;
        inv.pending_switch = Some((4, target_idx));
        Ok(())
    }
}

/// Executes a validated purchase transaction for the local player.
#[allow(clippy::too_many_lines)]
pub fn execute_buy_transaction(
    player: &mut PlayerController,
    inv_opt: Option<&mut PlayerWeaponInventory>,
    rules: &CsGameRules,
    item: BuyItem,
) -> Result<(i32, &'static str), &'static str> {
    let account = PlayerAccount::new(player.money);
    let can_buy_time = rules.can_team_buy(player.team, player.in_buy_zone);

    let cost = match validate_purchase(
        &account,
        item,
        player.team,
        player.in_buy_zone,
        can_buy_time,
        player.is_vip,
    ) {
        Ok(c) => c,
        Err(BuyRejectionReason::NotInBuyZone | BuyRejectionReason::TeamRestricted) => {
            return Err("#Cannot_Buy_This");
        }
        Err(BuyRejectionReason::BuyTimeExpired) => return Err("#Cant_buy"),
        Err(BuyRejectionReason::InsufficientFunds { .. }) => return Err("#Not_Enough_Money"),
        Err(BuyRejectionReason::VipRestricted) => return Err("#VIP_cant_buy"),
        Err(BuyRejectionReason::AlreadyHaveItem | BuyRejectionReason::FullInventorySlot) => {
            return Err("#Cannot_Carry_More");
        }
    };

    // Apply item effects into inventory and player state
    match item {
        BuyItem::Weapon(wid) => {
            let Some(inv) = inv_opt else {
                return Err("#Cannot_Buy_This");
            };
            let slot_data = make_slot_data(wid);
            match wid.slot() {
                WeaponSlot::Primary => {
                    inv.slot1 = slot_data;
                    inv.bucket_weapons[0] = vec![slot_data];
                    inv.bucket_cursor[0] = 0;
                    inv.pending_switch = Some((1, 0));
                }
                WeaponSlot::Secondary => {
                    inv.slot2 = slot_data;
                    inv.bucket_weapons[1] = vec![slot_data];
                    inv.bucket_cursor[1] = 0;
                    inv.pending_switch = Some((2, 0));
                }
                WeaponSlot::Grenade => {
                    purchase_grenade(inv, wid)?;
                }
                _ => {}
            }
        }
        BuyItem::Kevlar => {
            player.armor = 100.0;
        }
        BuyItem::KevlarHelmet => {
            player.armor = 100.0;
            player.has_helmet = true;
        }
        BuyItem::Flashbang | BuyItem::HeGrenade | BuyItem::SmokeGrenade => {
            let wid = match item {
                BuyItem::Flashbang => WeaponId::Flashbang,
                BuyItem::HeGrenade => WeaponId::HeGrenade,
                _ => WeaponId::SmokeGrenade,
            };
            let Some(inv) = inv_opt else {
                return Err("#Cannot_Buy_This");
            };
            purchase_grenade(inv, wid)?;
        }
        BuyItem::DefusalKit => {
            player.has_defuser = true;
        }
        BuyItem::NightVision => {}
        BuyItem::PrimaryAmmo => {
            let Some(inv) = inv_opt else {
                return Err("#Cannot_Buy_This");
            };
            if let Some(wid) = inv.slot1.id {
                let def = crate::combat::get_weapon_def(wid);
                inv.slot1.reserve = def.max_reserve;
            } else {
                return Err("#Cannot_Buy_This");
            }
        }
        BuyItem::SecondaryAmmo => {
            let Some(inv) = inv_opt else {
                return Err("#Cannot_Buy_This");
            };
            if let Some(wid) = inv.slot2.id {
                let def = crate::combat::get_weapon_def(wid);
                inv.slot2.reserve = def.max_reserve;
            } else {
                return Err("#Cannot_Buy_This");
            }
        }
    }

    // Deduct cost and update player money
    player.money = (player.money - cost).max(0);
    Ok((cost, "Bought item successfully"))
}

/// Category pages for the authentic CS 1.6 VGUI/Text Buy Menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuyMenuCategory {
    #[default]
    Closed,
    Main,
    Pistols,
    Shotguns,
    Smgs,
    Rifles,
    MachineGuns,
    Equipment,
}

/// Resource managing Buy Menu state and active categories.
#[derive(bevy::prelude::Resource, Debug, Default)]
pub struct BuyMenuState {
    pub category: BuyMenuCategory,
}

/// Action produced by buy menu key navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyMenuInputResult {
    None,
    Close,
    Navigated,
    Purchase(BuyItem),
}

impl BuyMenuState {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.category != BuyMenuCategory::Closed
    }

    pub fn close(&mut self) {
        self.category = BuyMenuCategory::Closed;
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self, team: goldhub_game_cstrike::CsTeam, money: i32) -> String {
        let is_t = team == goldhub_game_cstrike::CsTeam::Terrorist;
        match self.category {
            BuyMenuCategory::Closed => String::new(),
            BuyMenuCategory::Main => format!(
                "=== BUY MENU (${money}) ===\n\
                 1. Pistols\n\
                 2. Shotguns\n\
                 3. Sub-Machine Guns\n\
                 4. Rifles\n\
                 5. Machine Guns\n\
                 6. Primary Ammo ($60)\n\
                 7. Secondary Ammo ($25)\n\
                 8. Equipment\n\
                 0. Cancel"
            ),
            BuyMenuCategory::Pistols => {
                if is_t {
                    "=== PISTOLS ===\n\
                 1. 228 Compact ($600)\n\
                 2. Glock 18 ($400)\n\
                 3. Dual 96G Elites ($800)\n\
                 4. Desert Eagle ($650)\n\
                 5. USP .45 ($500)\n\
                 0. Back"
                        .to_string()
                } else {
                    "=== PISTOLS ===\n\
                 1. 228 Compact ($600)\n\
                 2. Glock 18 ($400)\n\
                 3. Five-Seven ($750)\n\
                 4. Desert Eagle ($650)\n\
                 5. USP .45 ($500)\n\
                 0. Back"
                        .to_string()
                }
            }
            BuyMenuCategory::Shotguns => "=== SHOTGUNS ===\n\
                 1. Leone 12 Gauge Super ($1700)\n\
                 2. Leone YG1265 Auto Shotgun ($3000)\n\
                 0. Back"
                .to_string(),
            BuyMenuCategory::Smgs => {
                if is_t {
                    "=== SUB-MACHINE GUNS ===\n\
                 1. Ingram MAC-10 ($1400)\n\
                 2. KM Sub-Machine Gun ($1500)\n\
                 3. KM UMP45 ($1700)\n\
                 4. ES C90 ($2350)\n\
                 0. Back"
                        .to_string()
                } else {
                    "=== SUB-MACHINE GUNS ===\n\
                 1. Schmidt TMP ($1250)\n\
                 2. KM Sub-Machine Gun ($1500)\n\
                 3. KM UMP45 ($1700)\n\
                 4. ES C90 ($2350)\n\
                 0. Back"
                        .to_string()
                }
            }
            BuyMenuCategory::Rifles => {
                if is_t {
                    "=== RIFLES ===\n\
                 1. IDF Defender ($2000)\n\
                 2. CV-47 ($2500)\n\
                 3. Schmidt Scout ($2750)\n\
                 4. Krieg 552 ($3500)\n\
                 5. Magnum Sniper Rifle ($4750)\n\
                 6. D3/AU-1 ($5000)\n\
                 0. Back"
                        .to_string()
                } else {
                    "=== RIFLES ===\n\
                 1. Clarion 5.56 ($2250)\n\
                 2. Maverick M4A1 Carbine ($3100)\n\
                 3. Schmidt Scout ($2750)\n\
                 4. Bullpup ($3500)\n\
                 5. Magnum Sniper Rifle ($4750)\n\
                 6. Krieg 550 Commando ($4200)\n\
                 0. Back"
                        .to_string()
                }
            }
            BuyMenuCategory::MachineGuns => "=== MACHINE GUNS ===\n\
                 1. M249 ($5750)\n\
                 0. Back"
                .to_string(),
            BuyMenuCategory::Equipment => {
                if is_t {
                    "=== EQUIPMENT ===\n\
                 1. Kevlar Vest ($650)\n\
                 2. Kevlar + Helmet ($1000)\n\
                 3. Flashbang ($200)\n\
                 4. HE Grenade ($300)\n\
                 5. Smoke Grenade ($300)\n\
                 7. NightVision Goggles ($1250)\n\
                 0. Back"
                        .to_string()
                } else {
                    "=== EQUIPMENT ===\n\
                 1. Kevlar Vest ($650)\n\
                 2. Kevlar + Helmet ($1000)\n\
                 3. Flashbang ($200)\n\
                 4. HE Grenade ($300)\n\
                 5. Smoke Grenade ($300)\n\
                 6. Defusal Kit ($200)\n\
                 7. NightVision Goggles ($1250)\n\
                 0. Back"
                        .to_string()
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn handle_key(
        &mut self,
        digit: u8,
        team: goldhub_game_cstrike::CsTeam,
    ) -> BuyMenuInputResult {
        let is_t = team == goldhub_game_cstrike::CsTeam::Terrorist;
        if digit == 0 {
            if self.category == BuyMenuCategory::Main {
                self.category = BuyMenuCategory::Closed;
                return BuyMenuInputResult::Close;
            }
            self.category = BuyMenuCategory::Main;
            return BuyMenuInputResult::Navigated;
        }

        match self.category {
            BuyMenuCategory::Closed => BuyMenuInputResult::None,
            BuyMenuCategory::Main => match digit {
                1 => {
                    self.category = BuyMenuCategory::Pistols;
                    BuyMenuInputResult::Navigated
                }
                2 => {
                    self.category = BuyMenuCategory::Shotguns;
                    BuyMenuInputResult::Navigated
                }
                3 => {
                    self.category = BuyMenuCategory::Smgs;
                    BuyMenuInputResult::Navigated
                }
                4 => {
                    self.category = BuyMenuCategory::Rifles;
                    BuyMenuInputResult::Navigated
                }
                5 => {
                    self.category = BuyMenuCategory::MachineGuns;
                    BuyMenuInputResult::Navigated
                }
                6 => BuyMenuInputResult::Purchase(BuyItem::PrimaryAmmo),
                7 => BuyMenuInputResult::Purchase(BuyItem::SecondaryAmmo),
                8 => {
                    self.category = BuyMenuCategory::Equipment;
                    BuyMenuInputResult::Navigated
                }
                _ => BuyMenuInputResult::None,
            },
            BuyMenuCategory::Pistols => match digit {
                1 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::P228)),
                2 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::Glock18)),
                3 => BuyMenuInputResult::Purchase(BuyItem::Weapon(if is_t {
                    WeaponId::Elite
                } else {
                    WeaponId::FiveSeven
                })),
                4 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::Deagle)),
                5 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::USP45)),
                _ => BuyMenuInputResult::None,
            },
            BuyMenuCategory::Shotguns => match digit {
                1 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::M3)),
                2 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::XM1014)),
                _ => BuyMenuInputResult::None,
            },
            BuyMenuCategory::Smgs => match digit {
                1 => BuyMenuInputResult::Purchase(BuyItem::Weapon(if is_t {
                    WeaponId::Mac10
                } else {
                    WeaponId::Tmp
                })),
                2 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::Mp5)),
                3 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::Ump45)),
                4 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::P90)),
                _ => BuyMenuInputResult::None,
            },
            BuyMenuCategory::Rifles => match digit {
                1 => BuyMenuInputResult::Purchase(BuyItem::Weapon(if is_t {
                    WeaponId::Galil
                } else {
                    WeaponId::Famas
                })),
                2 => BuyMenuInputResult::Purchase(BuyItem::Weapon(if is_t {
                    WeaponId::AK47
                } else {
                    WeaponId::M4A1
                })),
                3 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::Scout)),
                4 => BuyMenuInputResult::Purchase(BuyItem::Weapon(if is_t {
                    WeaponId::Sg552
                } else {
                    WeaponId::Aug
                })),
                5 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::AWP)),
                6 => BuyMenuInputResult::Purchase(BuyItem::Weapon(if is_t {
                    WeaponId::G3SG1
                } else {
                    WeaponId::Sg550
                })),
                _ => BuyMenuInputResult::None,
            },
            BuyMenuCategory::MachineGuns => match digit {
                1 => BuyMenuInputResult::Purchase(BuyItem::Weapon(WeaponId::M249)),
                _ => BuyMenuInputResult::None,
            },
            BuyMenuCategory::Equipment => match digit {
                1 => BuyMenuInputResult::Purchase(BuyItem::Kevlar),
                2 => BuyMenuInputResult::Purchase(BuyItem::KevlarHelmet),
                3 => BuyMenuInputResult::Purchase(BuyItem::Flashbang),
                4 => BuyMenuInputResult::Purchase(BuyItem::HeGrenade),
                5 => BuyMenuInputResult::Purchase(BuyItem::SmokeGrenade),
                6 if !is_t => BuyMenuInputResult::Purchase(BuyItem::DefusalKit),
                7 => BuyMenuInputResult::Purchase(BuyItem::NightVision),
                _ => BuyMenuInputResult::None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_buy_alias_canonical() {
        assert_eq!(
            parse_buy_alias("ak47"),
            Some(BuyItem::Weapon(WeaponId::AK47))
        );
        assert_eq!(
            parse_buy_alias("cv47"),
            Some(BuyItem::Weapon(WeaponId::AK47))
        );
        assert_eq!(
            parse_buy_alias("m4a1"),
            Some(BuyItem::Weapon(WeaponId::M4A1))
        );
        assert_eq!(parse_buy_alias("awp"), Some(BuyItem::Weapon(WeaponId::AWP)));
        assert_eq!(
            parse_buy_alias("deagle"),
            Some(BuyItem::Weapon(WeaponId::Deagle))
        );
        assert_eq!(parse_buy_alias("vest"), Some(BuyItem::Kevlar));
        assert_eq!(parse_buy_alias("vesthelm"), Some(BuyItem::KevlarHelmet));
        assert_eq!(parse_buy_alias("defuser"), Some(BuyItem::DefusalKit));
        assert_eq!(parse_buy_alias("primammo"), Some(BuyItem::PrimaryAmmo));
        assert_eq!(parse_buy_alias("secammo"), Some(BuyItem::SecondaryAmmo));
        assert_eq!(parse_buy_alias("unknown_item"), None);
    }

    #[test]
    fn test_execute_buy_transaction_flow() {
        let mut pc = PlayerController {
            money: 3000,
            team: goldhub_game_cstrike::CsTeam::Terrorist,
            in_buy_zone: true,
            ..Default::default()
        };

        let mut inv = PlayerWeaponInventory::default();
        inv.bucket_weapons[3].clear();
        inv.slot4.id = None;
        let rules = CsGameRules::default();

        // 1. Buy AK47 ($2500)
        let res = execute_buy_transaction(
            &mut pc,
            Some(&mut inv),
            &rules,
            BuyItem::Weapon(WeaponId::AK47),
        );
        assert!(res.is_ok());
        assert_eq!(pc.money, 500);
        assert_eq!(inv.slot1.id, Some(WeaponId::AK47));

        // 2. Buy Kevlar+Helmet ($1000) -> insufficient funds
        let res2 = execute_buy_transaction(&mut pc, Some(&mut inv), &rules, BuyItem::KevlarHelmet);
        assert_eq!(res2, Err("#Not_Enough_Money"));

        // 3. Buy Flashbang ($200) -> success, remaining 300
        let res3 = execute_buy_transaction(&mut pc, Some(&mut inv), &rules, BuyItem::Flashbang);
        assert!(res3.is_ok());
        assert_eq!(pc.money, 300);
        assert_eq!(inv.slot4.id, Some(WeaponId::Flashbang));
    }
}
