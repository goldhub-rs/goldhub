//! Counter-Strike 1.6 Economy Engine, Buy Menu Specification, and Cash Transactions.
//!
//! Canonical money balance, purchase validation, and equipment pricing matching ReGameDLL.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::rules::CsTeam;
use crate::weapons::{WeaponId, WeaponSlot};

/// Maximum player money limit in standard CS 1.6 rules ($16,000).
pub const MAX_MONEY: i32 = 16_000;

/// Default starting money for fresh player joins or match resets ($800).
pub const START_MONEY: i32 = 800;

/// Kill reward per standard enemy frag ($300).
pub const REWARD_KILL: i32 = 300;

/// Penalty deducted for killing a teammate (-$3,300).
pub const PENALTY_TEAMKILL: i32 = 3300;

/// Reward given to player who planted the C4 bomb ($300).
pub const REWARD_BOMB_PLANTER: i32 = 300;

/// Reward given to player who successfully defused the bomb ($300).
pub const REWARD_BOMB_DEFUSER: i32 = 300;

/// Cash reward for initial hostage touch ($150).
pub const REWARD_HOSTAGE_TOUCH: i32 = 150;

/// Cash reward for successfully rescuing a hostage ($1,000 to rescuer, $150 team).
pub const REWARD_HOSTAGE_RESCUE_PLAYER: i32 = 1000;
pub const REWARD_HOSTAGE_RESCUE_TEAM: i32 = 150;

/// Cost of primary ammunition box.
pub const COST_AMMO_PRIMARY: i32 = 60;

/// Cost of secondary pistol ammunition box.
pub const COST_AMMO_SECONDARY: i32 = 50;

/// Equipment purchase prices matching ReGameDLL `CHalfLifeMultiplay::BuyEquipment`.
pub const COST_KEVLAR: i32 = 650;
pub const COST_KEVLAR_HELMET: i32 = 1000;
pub const COST_HELMET_UPGRADE: i32 = 350;
pub const COST_FLASHBANG: i32 = 200;
pub const COST_HE_GRENADE: i32 = 300;
pub const COST_SMOKE_GRENADE: i32 = 300;
pub const COST_DEFUSAL_KIT: i32 = 200;
pub const COST_NIGHT_VISION: i32 = 1250;

/// Team availability constraint for buyable items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuyTeamRequirement {
    Any,
    TerroristOnly,
    CtOnly,
}

/// Categorized items available through the CS 1.6 Buy Menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuyItem {
    Weapon(WeaponId),
    PrimaryAmmo,
    SecondaryAmmo,
    Kevlar,
    KevlarHelmet,
    Flashbang,
    HeGrenade,
    SmokeGrenade,
    DefusalKit,
    NightVision,
}

/// Root categories in the CS 1.6 Buy Menu (`B` key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuyMenuCategory {
    Pistols = 1,
    Shotguns = 2,
    SubMachineGuns = 3,
    Rifles = 4,
    MachineGuns = 5,
    PrimaryAmmo = 6,
    SecondaryAmmo = 7,
    Equipment = 8,
}

impl BuyItem {
    /// Returns the canonical purchase price in dollars ($).
    #[must_use]
    pub const fn price(self) -> i32 {
        match self {
            Self::Weapon(id) => weapon_price(id),
            Self::PrimaryAmmo => COST_AMMO_PRIMARY,
            Self::SecondaryAmmo => COST_AMMO_SECONDARY,
            Self::Kevlar => COST_KEVLAR,
            Self::KevlarHelmet => COST_KEVLAR_HELMET,
            Self::Flashbang => COST_FLASHBANG,
            Self::HeGrenade => COST_HE_GRENADE,
            Self::SmokeGrenade => COST_SMOKE_GRENADE,
            Self::DefusalKit => COST_DEFUSAL_KIT,
            Self::NightVision => COST_NIGHT_VISION,
        }
    }

    /// Returns the team requirement to purchase this item.
    #[must_use]
    pub const fn team_requirement(self) -> BuyTeamRequirement {
        match self {
            Self::Weapon(id) => weapon_team_requirement(id),
            Self::DefusalKit => BuyTeamRequirement::CtOnly,
            _ => BuyTeamRequirement::Any,
        }
    }

    /// Returns the buy menu category under which this item appears.
    #[must_use]
    pub const fn category(self) -> BuyMenuCategory {
        match self {
            Self::Weapon(id) => match id.slot() {
                WeaponSlot::Secondary => BuyMenuCategory::Pistols,
                WeaponSlot::Primary => match id {
                    WeaponId::M3 | WeaponId::XM1014 => BuyMenuCategory::Shotguns,
                    WeaponId::Mac10
                    | WeaponId::Tmp
                    | WeaponId::Mp5
                    | WeaponId::Ump45
                    | WeaponId::P90 => BuyMenuCategory::SubMachineGuns,
                    WeaponId::M249 => BuyMenuCategory::MachineGuns,
                    _ => BuyMenuCategory::Rifles,
                },
                WeaponSlot::Grenade | WeaponSlot::Melee | WeaponSlot::C4 => {
                    BuyMenuCategory::Equipment
                }
            },
            Self::PrimaryAmmo => BuyMenuCategory::PrimaryAmmo,
            Self::SecondaryAmmo => BuyMenuCategory::SecondaryAmmo,
            Self::Kevlar
            | Self::KevlarHelmet
            | Self::Flashbang
            | Self::HeGrenade
            | Self::SmokeGrenade
            | Self::DefusalKit
            | Self::NightVision => BuyMenuCategory::Equipment,
        }
    }
}

/// Canonical price lookup for weapons.
#[must_use]
pub const fn weapon_price(id: WeaponId) -> i32 {
    match id {
        WeaponId::Glock | WeaponId::Glock18 => 400,
        WeaponId::USP45 => 500,
        WeaponId::P228 => 600,
        WeaponId::Deagle => 650,
        WeaponId::FiveSeven => 750,
        WeaponId::Elite => 800,
        WeaponId::M3 | WeaponId::Ump45 => 1700,
        WeaponId::XM1014 => 3000,
        WeaponId::Tmp => 1250,
        WeaponId::Mac10 => 1400,
        WeaponId::Mp5 => 1500,
        WeaponId::P90 => 2350,
        WeaponId::Galil => 2000,
        WeaponId::Famas => 2250,
        WeaponId::AK47 => 2500,
        WeaponId::Scout => 2750,
        WeaponId::M4A1 => 3100,
        WeaponId::Aug | WeaponId::Sg552 => 3500,
        WeaponId::Sg550 => 4200,
        WeaponId::AWP => 4750,
        WeaponId::G3SG1 => 5000,
        WeaponId::M249 => 5750,
        WeaponId::Flashbang => 200,
        WeaponId::HeGrenade | WeaponId::SmokeGrenade => 300,
        WeaponId::Knife | WeaponId::C4 => 0,
    }
}

/// Canonical team availability for weapons.
#[must_use]
pub const fn weapon_team_requirement(id: WeaponId) -> BuyTeamRequirement {
    match id {
        WeaponId::Elite
        | WeaponId::Mac10
        | WeaponId::Galil
        | WeaponId::AK47
        | WeaponId::Sg552
        | WeaponId::G3SG1
        | WeaponId::C4 => BuyTeamRequirement::TerroristOnly,

        WeaponId::FiveSeven
        | WeaponId::Tmp
        | WeaponId::Famas
        | WeaponId::M4A1
        | WeaponId::Aug
        | WeaponId::Sg550 => BuyTeamRequirement::CtOnly,

        _ => BuyTeamRequirement::Any,
    }
}

/// Reason for a rejected buy request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuyRejectionReason {
    NotInBuyZone,
    BuyTimeExpired,
    TeamRestricted,
    InsufficientFunds { cost: i32, current: i32 },
    AlreadyHaveItem,
    FullInventorySlot,
    VipRestricted,
}

/// Player bank account state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlayerAccount {
    money: i32,
}

impl Default for PlayerAccount {
    fn default() -> Self {
        Self::new(START_MONEY)
    }
}

impl PlayerAccount {
    /// Creates a new player bank account initialized to `initial_money` (clamped to `[0, MAX_MONEY]`).
    #[must_use]
    pub const fn new(initial_money: i32) -> Self {
        Self {
            money: if initial_money < 0 {
                0
            } else if initial_money > MAX_MONEY {
                MAX_MONEY
            } else {
                initial_money
            },
        }
    }

    /// Returns current cash balance.
    #[must_use]
    pub const fn money(&self) -> i32 {
        self.money
    }

    /// Credits cash rewards or round bonuses, clamped to `$16,000`.
    pub fn add(&mut self, amount: i32) {
        if amount > 0 {
            self.money = (self.money + amount).min(MAX_MONEY);
        }
    }

    /// Deducts cash penalties (e.g. teamkill penalty), clamped to `$0`.
    pub fn deduct(&mut self, penalty: i32) {
        if penalty > 0 {
            self.money = (self.money - penalty).max(0);
        }
    }

    /// Checks whether the account has sufficient funds to purchase `cost`.
    #[must_use]
    pub const fn can_afford(&self, cost: i32) -> bool {
        self.money >= cost
    }

    /// Attempts to deduct `cost` dollars. Returns `true` if successful, or `false` on insufficient funds.
    pub fn try_spend(&mut self, cost: i32) -> bool {
        if cost < 0 || self.money < cost {
            return false;
        }
        self.money -= cost;
        true
    }
}

/// Validates whether a transaction is permissible under active game conditions.
#[allow(clippy::too_many_arguments)]
pub fn validate_purchase(
    account: &PlayerAccount,
    item: BuyItem,
    player_team: CsTeam,
    in_buy_zone: bool,
    can_buy_time: bool,
    is_vip: bool,
) -> Result<i32, BuyRejectionReason> {
    if !in_buy_zone {
        return Err(BuyRejectionReason::NotInBuyZone);
    }
    if !can_buy_time {
        return Err(BuyRejectionReason::BuyTimeExpired);
    }
    if is_vip {
        return Err(BuyRejectionReason::VipRestricted);
    }

    // Check team restriction
    match item.team_requirement() {
        BuyTeamRequirement::TerroristOnly if player_team != CsTeam::Terrorist => {
            return Err(BuyRejectionReason::TeamRestricted);
        }
        BuyTeamRequirement::CtOnly if player_team != CsTeam::CounterTerrorist => {
            return Err(BuyRejectionReason::TeamRestricted);
        }
        _ => {}
    }

    let cost = item.price();
    if !account.can_afford(cost) {
        return Err(BuyRejectionReason::InsufficientFunds {
            cost,
            current: account.money(),
        });
    }

    Ok(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_account_clamp_and_transactions() {
        let mut acc = PlayerAccount::default();
        assert_eq!(acc.money(), 800);

        // Try spend
        assert!(acc.try_spend(400));
        assert_eq!(acc.money(), 400);

        // Insufficient funds
        assert!(!acc.try_spend(500));
        assert_eq!(acc.money(), 400);

        // Add reward with max clamp
        acc.add(17_000);
        assert_eq!(acc.money(), 16_000);

        // Penalty with zero clamp
        acc.deduct(20_000);
        assert_eq!(acc.money(), 0);
    }

    #[test]
    fn test_buy_item_pricing_and_team_constraints() {
        let ak = BuyItem::Weapon(WeaponId::AK47);
        assert_eq!(ak.price(), 2500);
        assert_eq!(ak.team_requirement(), BuyTeamRequirement::TerroristOnly);

        let m4 = BuyItem::Weapon(WeaponId::M4A1);
        assert_eq!(m4.price(), 3100);
        assert_eq!(m4.team_requirement(), BuyTeamRequirement::CtOnly);

        let defuser = BuyItem::DefusalKit;
        assert_eq!(defuser.price(), 200);
        assert_eq!(defuser.team_requirement(), BuyTeamRequirement::CtOnly);

        let awp = BuyItem::Weapon(WeaponId::AWP);
        assert_eq!(awp.price(), 4750);
        assert_eq!(awp.team_requirement(), BuyTeamRequirement::Any);
    }

    #[test]
    fn test_validate_purchase_rules() {
        let acc = PlayerAccount::new(3000);
        let ak = BuyItem::Weapon(WeaponId::AK47);

        // T buying AK in buyzone during buytime: success
        assert_eq!(
            validate_purchase(&acc, ak, CsTeam::Terrorist, true, true, false),
            Ok(2500)
        );

        // CT buying AK: team restricted
        assert_eq!(
            validate_purchase(&acc, ak, CsTeam::CounterTerrorist, true, true, false),
            Err(BuyRejectionReason::TeamRestricted)
        );

        // Outside buyzone
        assert_eq!(
            validate_purchase(&acc, ak, CsTeam::Terrorist, false, true, false),
            Err(BuyRejectionReason::NotInBuyZone)
        );

        // Expired buytime
        assert_eq!(
            validate_purchase(&acc, ak, CsTeam::Terrorist, true, false, false),
            Err(BuyRejectionReason::BuyTimeExpired)
        );

        // Insufficient funds
        let broke_acc = PlayerAccount::new(1000);
        assert_eq!(
            validate_purchase(&broke_acc, ak, CsTeam::Terrorist, true, true, false),
            Err(BuyRejectionReason::InsufficientFunds {
                cost: 2500,
                current: 1000
            })
        );
    }
}
