//! Authority-grade CS 1.6 Combat & Weapon State Machine Runtime (ReGameDLL Parity).
//!
//! Implements CBasePlayerWeapon lifecycle:
//! - Weapon inventory and switching (1: Primary, 2: Secondary, 3: Knife).
//! - Semi-auto, full-auto, and burst-fire modes with exact cycle times.
//! - Weapon recoil kickback tables (`punchangle`) and spread decay.
//! - Reload cycle with animation timing.
//! - Hitscan bullet ballistics with material penetration and damage dispatch.

pub mod buy;
pub mod recoil;
pub mod weapon;

pub use buy::{
    BuyMenuCategory, BuyMenuInputResult, BuyMenuState, execute_buy_transaction, parse_buy_alias,
};
pub use recoil::{PlayerRecoil, decay_punchangle_system};
pub use weapon::{
    ActiveWeaponState, CombatPlugin, PlayerWeaponInventory, WeaponDef, WeaponId, get_weapon_def,
    weapon_fire_and_reload_system, weapon_switch_system,
};
