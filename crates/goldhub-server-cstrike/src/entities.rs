//! Authoritative game entities, brushes, triggers, and scenario state.

use bevy::prelude::*;

/// Material type for `func_breakable` brush entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BreakableMaterial {
    Glass,
    Wood,
    Metal,
    Flesh,
    CinderBlock,
    CeilingTile,
    Computer,
    UnbreakableGlass,
    #[default]
    Rocks,
}

impl BreakableMaterial {
    #[must_use]
    pub const fn from_raw(val: u32) -> Self {
        match val {
            0 => Self::Glass,
            1 => Self::Wood,
            2 => Self::Metal,
            3 => Self::Flesh,
            4 => Self::CinderBlock,
            5 => Self::CeilingTile,
            6 => Self::Computer,
            7 => Self::UnbreakableGlass,
            _ => Self::Rocks,
        }
    }
}

/// Authoritative breakable brush entity (`func_breakable` / `func_pushable`).
#[derive(Component, Debug, Clone)]
pub struct BreakableEntity {
    pub model: String,
    pub targetname: String,
    pub target: String,
    pub health: f32,
    pub max_health: f32,
    pub material: BreakableMaterial,
    pub explodemagnitude: u32,
    pub spawnflags: u32,
    pub is_broken: bool,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Lifecycle state for doors (`func_door` and `func_door_rotating`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoorState {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

/// Interactive door entity (`func_door` or `func_door_rotating`).
#[derive(Component, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct DoorEntity {
    pub model: String,
    pub targetname: String,
    pub target: String,
    pub spawnflags: u32,
    pub speed: f32,
    pub wait: f32,
    pub dmg: f32,
    pub wait_timer: f32,
    pub blocked_cooldown: f32,
    pub state: DoorState,
    pub is_rotating: bool,
    pub is_passable: bool,
    pub opens_clockwise: bool,
    pub move_dir: Vec3,
    pub move_distance: f32,
    pub current_offset: f32,
    pub base_translation: Vec3,
    pub base_rotation: Quat,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub base_gs_origin: [f32; 3],
}

/// Trigger zone marking hostage rescue point (`func_hostage_rescue`).
#[derive(Component, Debug, Clone, Default)]
pub struct HostageRescueZoneEntity {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Push trigger exerting directional force (`trigger_push`).
#[derive(Component, Debug, Clone)]
pub struct TriggerPushEntity {
    pub speed: f32,
    pub angles: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Damage trigger injuring players inside (`trigger_hurt`).
#[derive(Component, Debug, Clone)]
pub struct TriggerHurtEntity {
    pub dmg: f32,
    pub damagetype: u32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Equipment purchase zone (`func_buyzone`).
#[derive(Component, Debug, Clone)]
pub struct BuyZoneEntity {
    pub team: u8,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Bomb site trigger volume (`func_bomb_target`).
#[derive(Component, Debug, Clone, Default)]
pub struct BombTargetZoneEntity {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Point-based bomb target (`info_bomb_target`).
#[derive(Component, Debug, Clone)]
pub struct BombTargetPointEntity {
    pub origin: [f32; 3],
    pub radius: f32,
}

/// Generic reusable trigger brush (`trigger_multiple`).
#[derive(Component, Debug, Clone)]
pub struct TriggerMultipleEntity {
    pub target: String,
    pub targetname: String,
    pub wait: f32,
    pub delay_timer: f32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Single-fire trigger brush (`trigger_once`).
#[derive(Component, Debug, Clone)]
pub struct TriggerOnceEntity {
    pub target: String,
    pub targetname: String,
    pub fired: bool,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// VIP safety extraction zone (`func_vip_safetyzone`).
#[derive(Component, Debug, Clone, Default)]
pub struct VipSafetyZoneEntity {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Terrorist escape zone (`func_escapezone`).
#[derive(Component, Debug, Clone, Default)]
pub struct EscapeZoneEntity {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// Ground dropped defuse kit (`item_thighpack`).
#[derive(Component, Debug, Clone)]
pub struct DefuseKitPickup {
    pub on_ground: bool,
    pub velocity: Vec3,
}

/// Map-wide scenario conditions & spawn points extracted from BSP entities.
#[derive(Resource, Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ScenarioMapData {
    pub ct_spawns: Vec<([f32; 3], f32)>,
    pub t_spawns: Vec<([f32; 3], f32)>,
    pub vip_spawn: Option<([f32; 3], f32)>,
    pub has_bomb_target: bool,
    pub has_bomb_zone: bool,
    pub has_rescue_zone: bool,
    pub has_vip_safety_zone: bool,
    pub has_escape_zone: bool,
    pub has_buy_zone: bool,
    pub hostage_count: u32,
    pub hostages_rescued: u32,
    pub bomb_defused: bool,
    pub bomb_exploded: bool,
}

/// Map-wide bomb parameters parsed from `info_map_parameters`.
#[derive(Resource, Debug, Clone)]
pub struct BombMapParams {
    pub arming_time: f32,
    pub defuse_time_with_kit: f32,
    pub defuse_time_without_kit: f32,
    pub c4_timer: f32,
    pub c4_damage_radius: f32,
}

impl Default for BombMapParams {
    fn default() -> Self {
        Self {
            arming_time: 3.0,
            defuse_time_with_kit: 5.0,
            defuse_time_without_kit: 10.0,
            c4_timer: 35.0,
            c4_damage_radius: 1750.0,
        }
    }
}

/// Event fired when an entity fires a target name or trigger.
#[derive(Event, Debug, Clone)]
pub struct FireTargetEvent {
    pub targetname: String,
    pub activator: Option<Entity>,
}

/// Event fired when a breakable entity reaches zero health and shatters.
#[derive(Event, Debug, Clone)]
pub struct BreakEntityEvent {
    pub entity: Entity,
    pub pos: Vec3,
    pub material: BreakableMaterial,
    pub explodemagnitude: u32,
}

/// Event signaling complete reset of the map environment for a new round.
#[derive(Event, Debug, Clone, Default)]
pub struct RoundResetEvent;

/// Tests whether a 3D ray segment intersects an axis-aligned bounding box.
#[must_use]
pub fn ray_intersects_aabb(
    start: Vec3,
    dir: Vec3,
    max_dist: f32,
    mins: Vec3,
    maxs: Vec3,
) -> Option<f32> {
    let inv_d = Vec3::new(
        if dir.x.abs() > 1e-6 {
            1.0 / dir.x
        } else {
            f32::INFINITY
        },
        if dir.y.abs() > 1e-6 {
            1.0 / dir.y
        } else {
            f32::INFINITY
        },
        if dir.z.abs() > 1e-6 {
            1.0 / dir.z
        } else {
            f32::INFINITY
        },
    );
    let t0 = (mins - start) * inv_d;
    let t1 = (maxs - start) * inv_d;
    let tmin = t0.min(t1);
    let tmax = t0.max(t1);
    let enter = tmin.x.max(tmin.y).max(tmin.z);
    let exit = tmax.x.min(tmax.y).min(tmax.z);

    if enter <= exit && exit >= 0.0 && enter <= max_dist {
        Some(enter.max(0.0))
    } else {
        None
    }
}
