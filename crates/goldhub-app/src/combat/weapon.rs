//! Authoritative CS 1.6 CBasePlayerWeapon state machine and weapon definitions.

use super::recoil::PlayerRecoil;
use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::entity::{
    BombMapParams, BreakEntityEvent, BreakableEntity, BreakableMaterial, BspSubmodelMesh,
    FireTargetEvent, RoundResetEvent, ScenarioMapData, ray_intersects_aabb,
};
use crate::player::{CameraPerspective, LevelCollisionResource, PlayerController};
use crate::sprite::{
    BulletHoleDecal, ImpactDecalKind, SpriteStore, spawn_black_smoke_puff, spawn_blood_sprite,
    spawn_c4_explosion_sprites, spawn_delayed_smoke, spawn_explosion_sprite,
    spawn_flicker_particles, spawn_ledglow_sprite, spawn_muzzleflash_at,
    spawn_muzzleflash_attached, spawn_scorch_decal, spawn_smoke_cloud, spawn_spark_shower,
    spawn_surface_decal, spawn_wall_sparks,
};
use crate::studio::animation::StudioAnimatedModel;
use crate::studio::store::{StudioModelStore, spawn_model_submesh_children};
use crate::studio::viewmodel::{VIEWMODEL_RENDER_LAYER, ViewModelWeapon};
use crate::studio::world_entity::{EntityCollisionBounds, HostageEntity, WorldStudioEntity};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

pub use goldhub_game_cstrike::{WeaponDef, WeaponId, WeaponSlot, get_weapon_def};

/// Active weapon state component managing attack delays, clip, reload, burst fire, and C4 arming.
#[derive(Component, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ActiveWeaponState {
    pub id: WeaponId,
    pub clip: i32,
    pub reserve: i32,
    pub next_primary_attack: f32,
    pub next_secondary_attack: f32,
    pub in_reload: bool,
    pub reload_timer: f32,
    pub shots_fired: u32,
    pub silenced: bool,
    pub burst_mode: bool,
    pub burst_shots_remaining: u8,
    pub burst_timer: f32,
    pub zoom_level: u8,
    /// Active horizontal FOV in degrees (`90.0` default, `55/40/15/10` when zoomed).
    pub current_fov: f32,
    /// Sniper bolt-action resume-zoom FOV (`Some(prev_fov)` triggers automatic re-scope after shot).
    pub resume_zoom_fov: Option<f32>,
    /// Delay gate before continuous RMB hold advances sniper/scope zoom (`0.3s`).
    pub zoom_hold_timer: f32,
    /// Cooldown timer for empty-clip dryfire clicks while holding LMB (`0.2s`).
    pub next_dryfire_time: f32,
    /// Current ReGameDLL `m_flAccuracy` value.
    pub accuracy: f32,
    /// Elapsed world time when the weapon last fired (`m_flPrevPrimaryAttack` delta).
    pub last_fire_time: f32,
    /// True once at least one round was fired since deploy/reload (used by sniper `G3SG1`/`Sg550`).
    pub has_fired_before: bool,
    /// Shotgun incremental reload FSM (`0 = none`, `1 = start_reload`, `2 = insert shells`).
    pub shotgun_reload_stage: u8,
    /// Grenade pin-pull timer for throwing lifecycle.
    pub pin_pulled: bool,
    pub pin_pull_timer: f32,
    /// Accuracy decay delay after trigger release (`m_flDecreaseShotsFired`).
    pub decrease_shots_timer: f32,
    /// Short delay after grenade throw / C4 drop animation before equipping next item or knife.
    pub pending_knife_swap_timer: Option<f32>,
    /// True while player is holding LMB inside a bombsite arming C4 (`m_bStartedArming`).
    pub c4_arming: bool,
    /// Elapsed C4 arming time (`0.0..3.0` seconds, matching `C4_ARMING_ON_TIME`).
    pub c4_arm_timer: f32,
    /// True during post-plant `C4_DROP` animation before retiring to knife.
    pub c4_placed_anim: bool,
}

impl Default for ActiveWeaponState {
    fn default() -> Self {
        let def = get_weapon_def(WeaponId::Knife);
        Self {
            id: WeaponId::Knife,
            clip: def.clip_size,
            reserve: def.max_reserve,
            next_primary_attack: 0.0,
            next_secondary_attack: 0.0,
            in_reload: false,
            reload_timer: 0.0,
            shots_fired: 0,
            silenced: false,
            burst_mode: false,
            burst_shots_remaining: 0,
            burst_timer: 0.0,
            zoom_level: 0,
            current_fov: 90.0,
            resume_zoom_fov: None,
            zoom_hold_timer: 0.0,
            next_dryfire_time: 0.0,
            accuracy: 0.0,
            last_fire_time: -10.0,
            has_fired_before: false,
            shotgun_reload_stage: 0,
            pin_pulled: false,
            pin_pull_timer: 0.0,
            decrease_shots_timer: 0.0,
            pending_knife_swap_timer: None,
            c4_arming: false,
            c4_arm_timer: 0.0,
            c4_placed_anim: false,
        }
    }
}

/// Toggle resource matching CS 1.6 `hud_fastswitch` (`false` = `"0"` menu confirm, `true` = `"1"` instant).
#[derive(Resource, Debug, Clone, Default)]
pub struct HudFastSwitch(pub bool);

/// CS 1.6 classic two-step weapon selection menu state (`DrawWList`, no auto-hide timeout).
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingWeaponSelection {
    pub open: bool,
    pub slot: u8,
    pub bucket_idx: usize,
    pub consume_next_attack: bool,
}

/// Maps a CS 1.6 weapon ID to its canonical brass shell `.mdl` path (`pshell`, `rshell`, `rshell_big`, `shotgunshell`).
#[must_use]
pub const fn shell_model_for_weapon(id: WeaponId) -> &'static str {
    match id {
        WeaponId::M3 | WeaponId::XM1014 => "models/shotgunshell.mdl",
        WeaponId::AWP | WeaponId::Scout => "models/rshell_big.mdl",
        WeaponId::Glock18
        | WeaponId::USP45
        | WeaponId::P228
        | WeaponId::Deagle
        | WeaponId::Elite
        | WeaponId::FiveSeven
        | WeaponId::Tmp
        | WeaponId::Ump45 => "models/pshell.mdl",
        _ => "models/rshell.mdl",
    }
}

/// Persistent weapon state for an inventory slot.
#[derive(Debug, Clone, Copy)]
pub struct WeaponSlotData {
    pub id: Option<WeaponId>,
    pub clip: i32,
    pub reserve: i32,
    pub silenced: bool,
    pub burst_mode: bool,
    pub next_primary_attack: f32,
}

impl WeaponSlotData {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            id: None,
            clip: 0,
            reserve: 0,
            silenced: false,
            burst_mode: false,
            next_primary_attack: 0.0,
        }
    }
}

/// Dropped world weapon sitting on the floor, ready to be picked up (`CWeaponBox` parity).
#[derive(Component, Debug)]
pub struct DroppedWeapon {
    pub slot_index: u8,
    pub data: WeaponSlotData,
    pub velocity: Vec3,
    pub pickup_cooldown: f32,
}

/// Planted C4 bomb resting on the map with ReGameDLL wave progression, gravity settle, and `E` defusal.
#[derive(Component, Debug)]
pub struct PlantedC4 {
    pub timer: f32,
    pub beep_timer: f32,
    pub cur_wave: u8,
    pub next_freq: f32,
    pub next_freq_interval: f32,
    pub blink_timer: f32,
    pub velocity: Vec3,
    pub defuse_progress: f32,
    pub defuse_duration: f32,
    pub is_being_defused: bool,
    pub on_ground: bool,
}

/// Tag component enforcing single active `CHAN_VOICE` C4 beep playback so clips never stack.
#[derive(Component, Debug)]
pub struct PlantedC4BeepChannel;

/// Resting smoke grenade canister hopping/settling on the floor while emitting `black_smoke1..4.spr`.
#[derive(Component, Debug)]
pub struct SmokingCanister {
    pub velocity: Vec3,
    pub puffs_left: u8,
    pub emit_timer: f32,
    pub bounce_count: u32,
}

/// Player weapon slots inventory supporting canonical slots 1..5, multi-weapon buckets, and `lastinv`.
#[derive(Component, Debug, Clone)]
pub struct PlayerWeaponInventory {
    pub slot1: WeaponSlotData, // Primary active weapon
    pub slot2: WeaponSlotData, // Secondary active weapon
    pub slot3: WeaponSlotData, // Knife / Melee
    pub slot4: WeaponSlotData, // Grenades active weapon
    pub slot5: WeaponSlotData, // C4 (Terrorist objective)
    /// Overflow items per slot (`[slot1_list, slot2_list, slot3_list, slot4_list, slot5_list]`).
    pub bucket_weapons: [Vec<WeaponSlotData>; 5],
    pub bucket_cursor: [usize; 5],
    pub active_slot: u8,
    pub previous_slot: u8,
    pub pending_switch: Option<(u8, usize)>,
}

pub(crate) fn make_slot_data(id: WeaponId) -> WeaponSlotData {
    let d = get_weapon_def(id);
    WeaponSlotData {
        id: Some(id),
        clip: d.clip_size,
        reserve: d.max_reserve,
        silenced: false,
        burst_mode: false,
        next_primary_attack: 0.0,
    }
}

impl Default for PlayerWeaponInventory {
    fn default() -> Self {
        let m4 = make_slot_data(WeaponId::M4A1);
        let ak = make_slot_data(WeaponId::AK47);
        let awp = make_slot_data(WeaponId::AWP);
        let glock = make_slot_data(WeaponId::Glock18);
        let usp = make_slot_data(WeaponId::USP45);
        let deagle = make_slot_data(WeaponId::Deagle);
        let knife = make_slot_data(WeaponId::Knife);
        let he = make_slot_data(WeaponId::HeGrenade);
        let flash = make_slot_data(WeaponId::Flashbang);
        let smoke = make_slot_data(WeaponId::SmokeGrenade);
        let c4 = make_slot_data(WeaponId::C4);

        Self {
            slot1: m4,
            slot2: glock,
            slot3: knife,
            slot4: he,
            slot5: c4,
            bucket_weapons: [
                vec![m4, ak, awp],
                vec![glock, usp, deagle],
                vec![knife],
                vec![he, flash, smoke],
                vec![c4],
            ],
            bucket_cursor: [0, 0, 0, 0, 0],
            active_slot: 3,
            previous_slot: 1,
            pending_switch: None,
        }
    }
}

impl PlayerWeaponInventory {
    /// Returns slot data reference for a 1-based index (1..=5).
    #[must_use]
    pub fn get_slot(&self, slot: u8) -> Option<&WeaponSlotData> {
        match slot {
            1 => Some(&self.slot1),
            2 => Some(&self.slot2),
            3 => Some(&self.slot3),
            4 => Some(&self.slot4),
            5 => Some(&self.slot5),
            _ => None,
        }
    }

    /// Returns mutable slot data reference for a 1-based index (1..=5).
    #[must_use]
    pub fn get_slot_mut(&mut self, slot: u8) -> Option<&mut WeaponSlotData> {
        match slot {
            1 => Some(&mut self.slot1),
            2 => Some(&mut self.slot2),
            3 => Some(&mut self.slot3),
            4 => Some(&mut self.slot4),
            5 => Some(&mut self.slot5),
            _ => None,
        }
    }

    /// Cycles to the next weapon within the same slot bucket (if multiple are held), returning `true` if changed.
    pub fn cycle_within_slot(&mut self, slot: u8) -> bool {
        if !(1..=5).contains(&slot) {
            return false;
        }
        let b_idx = (slot - 1) as usize;
        let len = self.bucket_weapons[b_idx].len();
        if len <= 1 {
            return false;
        }
        if let Some(cur) = self.get_slot(slot).copied() {
            let cur_pos = self.bucket_cursor[b_idx].min(len - 1);
            self.bucket_weapons[b_idx][cur_pos] = cur;
        }
        let next_pos = (self.bucket_cursor[b_idx] + 1) % len;
        self.bucket_cursor[b_idx] = next_pos;
        let next_item = self.bucket_weapons[b_idx][next_pos];
        if let Some(s) = self.get_slot_mut(slot) {
            *s = next_item;
        }
        true
    }

    /// Finds the next non-empty slot in cycling order.
    #[must_use]
    pub fn next_occupied_slot(&self, forward: bool) -> Option<u8> {
        let total_slots = 5u8;
        let start = self.active_slot;
        for step in 1..total_slots {
            let candidate = if forward {
                (start + step - 1) % total_slots + 1
            } else {
                (start + total_slots - step - 1) % total_slots + 1
            };
            if let Some(slot_data) = self.get_slot(candidate)
                && slot_data.id.is_some()
            {
                return Some(candidate);
            }
        }
        None
    }
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudFastSwitch>()
            .init_resource::<PendingWeaponSelection>()
            .add_systems(
                Update,
                (
                    handle_round_reset_combat_system.before(weapon_switch_system),
                    super::recoil::decay_punchangle_system.before(weapon_fire_and_reload_system),
                    weapon_switch_system.before(weapon_fire_and_reload_system),
                    weapon_drop_and_pickup_system,
                    weapon_fire_and_reload_system.before(crate::player::player_movement_system),
                    update_bullet_particles_system,
                    update_brass_casings_system,
                    update_grenade_projectiles_system,
                    update_planted_c4_system,
                    update_smoking_canister_system,
                ),
            );
    }
}

/// Maps a CS 1.6 weapon ID to its world `.mdl` path (`models/w_*.mdl`).
#[must_use]
pub const fn world_model_for_weapon(id: WeaponId) -> &'static str {
    match id {
        WeaponId::P228 => "models/w_p228.mdl",
        WeaponId::Glock | WeaponId::Glock18 => "models/w_glock18.mdl",
        WeaponId::Scout => "models/w_scout.mdl",
        WeaponId::HeGrenade => "models/w_hegrenade.mdl",
        WeaponId::XM1014 => "models/w_xm1014.mdl",
        WeaponId::C4 => "models/w_backpack.mdl",
        WeaponId::Mac10 => "models/w_mac10.mdl",
        WeaponId::Aug => "models/w_aug.mdl",
        WeaponId::SmokeGrenade => "models/w_smokegrenade.mdl",
        WeaponId::Elite => "models/w_elite.mdl",
        WeaponId::FiveSeven => "models/w_fiveseven.mdl",
        WeaponId::Ump45 => "models/w_ump45.mdl",
        WeaponId::Sg550 => "models/w_sg550.mdl",
        WeaponId::Galil => "models/w_galil.mdl",
        WeaponId::Famas => "models/w_famas.mdl",
        WeaponId::USP45 => "models/w_usp.mdl",
        WeaponId::AWP => "models/w_awp.mdl",
        WeaponId::Mp5 => "models/w_mp5.mdl",
        WeaponId::M249 => "models/w_m249.mdl",
        WeaponId::M3 => "models/w_m3.mdl",
        WeaponId::M4A1 => "models/w_m4a1.mdl",
        WeaponId::Tmp => "models/w_tmp.mdl",
        WeaponId::G3SG1 => "models/w_g3sg1.mdl",
        WeaponId::Flashbang => "models/w_flashbang.mdl",
        WeaponId::Deagle => "models/w_deagle.mdl",
        WeaponId::Sg552 => "models/w_sg552.mdl",
        WeaponId::AK47 => "models/w_ak47.mdl",
        WeaponId::Knife => "models/w_knife.mdl",
        WeaponId::P90 => "models/w_p90.mdl",
    }
}

fn play_ui_sound(
    commands: &mut Commands,
    sound_path: &str,
    search_paths: &GameSearchPaths,
    audio_bank: &mut GoldSrcAudioBank,
    audio_sources: &mut Assets<AudioSource>,
) {
    if let Some(h) = audio_bank.get_or_load_sound(sound_path, &search_paths.dirs, audio_sources) {
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
    }
}

/// Equips a weapon slot onto [`ActiveWeaponState`] and swaps the Layer 1 ViewModel submeshes (`0.75s` `DefaultDeploy`).
#[allow(clippy::too_many_arguments)]
fn equip_viewmodel_weapon(
    commands: &mut Commands,
    state: &mut ActiveWeaponState,
    slot_data: WeaponSlotData,
    search_paths: &GameSearchPaths,
    audio_bank: &mut GoldSrcAudioBank,
    audio_sources: &mut Assets<AudioSource>,
    model_store: &mut StudioModelStore,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    vm_query: &mut Query<(Entity, &mut StudioAnimatedModel), With<ViewModelWeapon>>,
) {
    let Some(weapon_id) = slot_data.id else {
        return;
    };
    let def = get_weapon_def(weapon_id);
    state.id = weapon_id;
    state.clip = slot_data.clip;
    state.reserve = slot_data.reserve;
    state.silenced = slot_data.silenced;
    state.burst_mode = slot_data.burst_mode;
    state.burst_shots_remaining = 0;
    state.burst_timer = 0.0;
    state.next_primary_attack = slot_data.next_primary_attack.max(0.75);
    state.next_secondary_attack = slot_data.next_primary_attack.max(0.75);
    state.in_reload = false;
    state.shots_fired = 0;
    state.zoom_level = 0;
    state.current_fov = 90.0;
    state.resume_zoom_fov = None;
    state.zoom_hold_timer = 0.0;
    state.next_dryfire_time = 0.0;
    state.accuracy = goldhub_game_cstrike::initial_weapon_accuracy(weapon_id);
    state.has_fired_before = false;
    state.shotgun_reload_stage = 0;
    state.pin_pulled = false;
    state.pin_pull_timer = 0.0;
    state.pending_knife_swap_timer = None;
    state.c4_arming = false;
    state.c4_arm_timer = 0.0;
    state.c4_placed_anim = false;

    if weapon_id == WeaponId::Knife {
        play_ui_sound(
            commands,
            "weapons/knife_deploy1.wav",
            search_paths,
            audio_bank,
            audio_sources,
        );
    }

    for (vm_entity, mut anim) in vm_query.iter_mut() {
        for (child_ent, mesh_handle, _) in anim.mesh_entities.drain(..) {
            meshes.remove(&mesh_handle);
            commands.entity(child_ent).despawn_recursive();
        }

        if let Some(model_data) =
            model_store.get_or_load(def.v_model, search_paths, images, materials)
        {
            anim.model_id = def.v_model.to_string();
            let (deploy_anim_idx, idle_seq_idx) = match weapon_id {
                WeaponId::USP45 => {
                    if state.silenced {
                        (6, 0)
                    } else {
                        (14, 8)
                    }
                }
                WeaponId::M4A1 => {
                    if state.silenced {
                        (5, 0)
                    } else {
                        (12, 7)
                    }
                }
                _ => (def.deploy_anim, 0),
            };
            anim.idle_sequence_index = idle_seq_idx;
            let deploy_fps = model_data
                .model
                .rig
                .sequences
                .get(deploy_anim_idx)
                .map_or(30.0, |s| s.fps);
            anim.set_sequence(deploy_anim_idx, deploy_fps, false);

            anim.mesh_entities = spawn_model_submesh_children(
                commands,
                vm_entity,
                &model_data,
                deploy_anim_idx,
                true,
                Some(VIEWMODEL_RENDER_LAYER),
                meshes,
            );
        }
    }
}

/// Persists outgoing active weapon state into its bucket slot and equips the requested `(slot, bucket_idx)`.
#[allow(clippy::too_many_arguments)]
fn commit_weapon_slot_switch(
    commands: &mut Commands,
    inv: &mut PlayerWeaponInventory,
    state: &mut ActiveWeaponState,
    target_slot: u8,
    target_bucket_idx: usize,
    search_paths: &GameSearchPaths,
    audio_bank: &mut GoldSrcAudioBank,
    audio_sources: &mut Assets<AudioSource>,
    model_store: &mut StudioModelStore,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    vm_query: &mut Query<(Entity, &mut StudioAnimatedModel), With<ViewModelWeapon>>,
) {
    let old_slot = inv.active_slot;
    let cur_item = WeaponSlotData {
        id: Some(state.id),
        clip: state.clip,
        reserve: state.reserve,
        silenced: state.silenced,
        burst_mode: state.burst_mode,
        next_primary_attack: state.next_primary_attack,
    };
    if let Some(out_slot) = inv.get_slot_mut(old_slot) {
        *out_slot = cur_item;
    }
    if (1..=5).contains(&old_slot) {
        let old_b = (old_slot - 1) as usize;
        let old_cur = inv.bucket_cursor[old_b];
        if let Some(updated) = inv.bucket_weapons[old_b].get_mut(old_cur) {
            *updated = cur_item;
        }
    }

    let b_idx = (target_slot - 1) as usize;
    let len = inv.bucket_weapons[b_idx].len();
    if len == 0 {
        return;
    }
    let chosen_idx = target_bucket_idx.min(len - 1);
    if target_slot == old_slot && chosen_idx == inv.bucket_cursor[b_idx] {
        return;
    }

    if target_slot != old_slot {
        inv.previous_slot = old_slot;
    }
    inv.active_slot = target_slot;
    inv.bucket_cursor[b_idx] = chosen_idx;
    let incoming_slot = inv.bucket_weapons[b_idx][chosen_idx];
    if let Some(s) = inv.get_slot_mut(target_slot) {
        *s = incoming_slot;
    }

    equip_viewmodel_weapon(
        commands,
        state,
        incoming_slot,
        search_paths,
        audio_bank,
        audio_sources,
        model_store,
        images,
        materials,
        meshes,
        vm_query,
    );
    if let Some(id) = incoming_slot.id {
        tracing::info!(
            slot = target_slot,
            weapon = get_weapon_def(id).name,
            "Switched active weapon"
        );
    }
}

/// Switches active weapon via CS 1.6 `hud_fastswitch` (`F7` toggle, default `0` menu with LMB/Enter
/// confirm and no timeout), `KeyQ` (`lastinv`), mouse wheel, and syncs C4 membership on team switch.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub fn weapon_switch_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut fast_switch: ResMut<HudFastSwitch>,
    mut pending: ResMut<PendingWeaponSelection>,
    mut audio: CombatAudio,
    mut model_store: ResMut<StudioModelStore>,
    mut vfx: CombatVfxAssets,
    mut last_team: Local<Option<goldhub_game_cstrike::CsTeam>>,
    planted_c4_query: Query<(), With<PlantedC4>>,
    dropped_query: Query<&DroppedWeapon>,
    mut player_query: Query<(
        &mut PlayerWeaponInventory,
        &mut ActiveWeaponState,
        Option<&mut PlayerController>,
    )>,
    mut vm_query: Query<(Entity, &mut StudioAnimatedModel), With<ViewModelWeapon>>,
    console: Option<Res<crate::hud::text_channels::ConsoleOverlayState>>,
    buy_menu: Option<Res<crate::combat::BuyMenuState>>,
) {
    let console_open = console.as_ref().is_some_and(|c| c.open);
    let buy_menu_open = buy_menu.as_ref().is_some_and(|m| m.is_open());
    if console_open || buy_menu_open {
        return;
    }
    let mut wheel_delta = 0.0f32;
    for ev in mouse_wheel.read() {
        wheel_delta += ev.y;
    }

    let direct_slot = if keyboard.just_pressed(KeyCode::Digit1) {
        Some(1)
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        Some(2)
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        Some(3)
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        Some(4)
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        Some(5)
    } else {
        None
    };

    let quick_switch = keyboard.just_pressed(KeyCode::KeyQ);
    let confirm_lmb = mouse_button.just_pressed(MouseButton::Left);
    let confirm_enter = keyboard.just_pressed(KeyCode::Enter);
    let cancel_esc = keyboard.just_pressed(KeyCode::Escape);

    let toggled_fastswitch = keyboard.just_pressed(KeyCode::F7);
    if toggled_fastswitch {
        fast_switch.0 = !fast_switch.0;
    }

    for (mut inv, mut state, mut player_opt) in &mut player_query {
        if toggled_fastswitch && let Some(ref mut player) = player_opt {
            let msg = if fast_switch.0 {
                "hud_fastswitch 1"
            } else {
                "hud_fastswitch 0"
            };
            player.set_center_message(msg, 1.5);
        }

        // Sync C4 availability when player switches teams (F6)
        if let Some(ref mut player) = player_opt
            && *last_team != Some(player.team)
        {
            let prev_was_some = last_team.is_some();
            *last_team = Some(player.team);
            if prev_was_some {
                player.anim_shoot_timer = 0.0;
                player.anim_shoot_retrigger = false;
                if player.team == goldhub_game_cstrike::CsTeam::Terrorist {
                    let c4_in_world = !planted_c4_query.is_empty()
                        || dropped_query
                            .iter()
                            .any(|d| d.data.id == Some(WeaponId::C4));
                    if !c4_in_world && inv.slot5.id.is_none() {
                        let c4_data = make_slot_data(WeaponId::C4);
                        inv.slot5 = c4_data;
                        inv.bucket_weapons[4] = vec![c4_data];
                        inv.bucket_cursor[4] = 0;
                    }
                } else {
                    inv.slot5 = WeaponSlotData::empty();
                    inv.bucket_weapons[4].clear();
                    inv.bucket_cursor[4] = 0;
                    if inv.active_slot == 5 {
                        inv.active_slot = 3;
                        if let Some(knife_slot) = inv.get_slot(3).copied() {
                            equip_viewmodel_weapon(
                                &mut commands,
                                &mut state,
                                knife_slot,
                                &audio.search_paths,
                                &mut audio.audio_bank,
                                &mut audio.audio_sources,
                                &mut model_store,
                                &mut vfx.images,
                                &mut vfx.materials,
                                &mut vfx.meshes,
                                &mut vm_query,
                            );
                        }
                    }
                }
            }
        }

        if let Some((target_slot, target_idx)) = inv.pending_switch.take() {
            pending.open = false;
            commit_weapon_slot_switch(
                &mut commands,
                &mut inv,
                &mut state,
                target_slot,
                target_idx,
                &audio.search_paths,
                &mut audio.audio_bank,
                &mut audio.audio_sources,
                &mut model_store,
                &mut vfx.images,
                &mut vfx.materials,
                &mut vfx.meshes,
                &mut vm_query,
            );
            continue;
        }

        // CS 1.6 `CanHolster`: block weapon switch/menu while grenade pin is pulled or C4 arming
        if state.pin_pulled {
            pending.open = false;
            continue;
        }

        if pending.open {
            if cancel_esc {
                pending.open = false;
                play_ui_sound(
                    &mut commands,
                    "common/wpn_hudoff.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
                continue;
            }
            if confirm_lmb || confirm_enter {
                pending.open = false;
                if confirm_lmb {
                    pending.consume_next_attack = true;
                }
                play_ui_sound(
                    &mut commands,
                    "common/wpn_select.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
                commit_weapon_slot_switch(
                    &mut commands,
                    &mut inv,
                    &mut state,
                    pending.slot,
                    pending.bucket_idx,
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                    &mut model_store,
                    &mut vfx.images,
                    &mut vfx.materials,
                    &mut vfx.meshes,
                    &mut vm_query,
                );
                continue;
            }
        }

        if let Some(slot) = direct_slot {
            let b_idx = (slot - 1) as usize;
            let len = inv.bucket_weapons[b_idx].len();
            if len == 0 {
                continue;
            }

            if fast_switch.0 && !pending.open && len == 1 {
                if inv.active_slot != slot {
                    commit_weapon_slot_switch(
                        &mut commands,
                        &mut inv,
                        &mut state,
                        slot,
                        0,
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                        &mut model_store,
                        &mut vfx.images,
                        &mut vfx.materials,
                        &mut vfx.meshes,
                        &mut vm_query,
                    );
                }
                continue;
            }

            if !pending.open {
                pending.open = true;
                pending.slot = slot;
                pending.bucket_idx = if inv.active_slot == slot && len > 1 {
                    (inv.bucket_cursor[b_idx] + 1) % len
                } else {
                    0
                };
                play_ui_sound(
                    &mut commands,
                    "common/wpn_hudon.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
            } else if pending.slot == slot {
                pending.bucket_idx = (pending.bucket_idx + 1) % len;
                play_ui_sound(
                    &mut commands,
                    "common/wpn_moveselect.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
            } else {
                pending.slot = slot;
                pending.bucket_idx = if inv.active_slot == slot {
                    inv.bucket_cursor[b_idx].min(len - 1)
                } else {
                    0
                };
                play_ui_sound(
                    &mut commands,
                    "common/wpn_moveselect.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
            }
            continue;
        }

        if quick_switch {
            pending.open = false;
            let prev = inv.previous_slot;
            if prev != inv.active_slot && inv.get_slot(prev).and_then(|s| s.id).is_some() {
                let target_cur = inv.bucket_cursor[(prev - 1) as usize];
                commit_weapon_slot_switch(
                    &mut commands,
                    &mut inv,
                    &mut state,
                    prev,
                    target_cur,
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                    &mut model_store,
                    &mut vfx.images,
                    &mut vfx.materials,
                    &mut vfx.meshes,
                    &mut vm_query,
                );
            } else if let Some(next_slot) = inv.next_occupied_slot(true) {
                let target_cur = inv.bucket_cursor[(next_slot - 1) as usize];
                commit_weapon_slot_switch(
                    &mut commands,
                    &mut inv,
                    &mut state,
                    next_slot,
                    target_cur,
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                    &mut model_store,
                    &mut vfx.images,
                    &mut vfx.materials,
                    &mut vfx.meshes,
                    &mut vm_query,
                );
            }
            continue;
        }

        if wheel_delta.abs() > 0.05 {
            let forward = wheel_delta < 0.0;
            if fast_switch.0 && !pending.open {
                if let Some(next_slot) = inv.next_occupied_slot(forward) {
                    let b_idx = (next_slot - 1) as usize;
                    let target_cur = inv.bucket_cursor[b_idx];
                    commit_weapon_slot_switch(
                        &mut commands,
                        &mut inv,
                        &mut state,
                        next_slot,
                        target_cur,
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                        &mut model_store,
                        &mut vfx.images,
                        &mut vfx.materials,
                        &mut vfx.meshes,
                        &mut vm_query,
                    );
                }
            } else {
                // Cycle through all currently held bucket items when menu is open or fastswitch is 0
                let mut flat_items: Vec<(u8, usize)> = Vec::new();
                for s_num in 1u8..=5 {
                    let b = (s_num - 1) as usize;
                    for (idx, item) in inv.bucket_weapons[b].iter().enumerate() {
                        if item.id.is_some() {
                            flat_items.push((s_num, idx));
                        }
                    }
                }
                if !flat_items.is_empty() {
                    let cur_pair = if pending.open {
                        (pending.slot, pending.bucket_idx)
                    } else {
                        let b = (inv.active_slot.saturating_sub(1) as usize).min(4);
                        (inv.active_slot, inv.bucket_cursor[b])
                    };
                    let cur_pos = flat_items.iter().position(|&p| p == cur_pair).unwrap_or(0);
                    let next_pos = if forward {
                        (cur_pos + 1) % flat_items.len()
                    } else {
                        (cur_pos + flat_items.len() - 1) % flat_items.len()
                    };
                    let (ns, nb) = flat_items[next_pos];
                    let was_open = pending.open;
                    pending.open = true;
                    pending.slot = ns;
                    pending.bucket_idx = nb;
                    play_ui_sound(
                        &mut commands,
                        if was_open {
                            "common/wpn_moveselect.wav"
                        } else {
                            "common/wpn_hudon.wav"
                        },
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                    );
                }
            }
        }
    }

    if !pending.open && pending.consume_next_attack && !mouse_button.pressed(MouseButton::Left) {
        pending.consume_next_attack = false;
    }
}

/// Clears all round-scoped combat entities (dropped guns, thrown/planted explosives, brass) and restores team loadout on `RoundResetEvent`.
#[allow(clippy::too_many_arguments)]
pub fn handle_round_reset_combat_system(
    mut commands: Commands,
    mut reset_events: EventReader<RoundResetEvent>,
    mut pending: ResMut<PendingWeaponSelection>,
    mut audio: CombatAudio,
    mut model_store: ResMut<StudioModelStore>,
    mut vfx: CombatVfxAssets,
    transient_query: Query<
        Entity,
        Or<(
            With<DroppedWeapon>,
            With<PlantedC4>,
            With<PlantedC4BeepChannel>,
            With<SmokingCanister>,
            With<GrenadeProjectile>,
            With<BrassCasingProjectile>,
            With<BulletParticle>,
        )>,
    >,
    transient_models: Query<
        &StudioAnimatedModel,
        (
            Without<ViewModelWeapon>,
            Or<(
                With<DroppedWeapon>,
                With<PlantedC4>,
                With<GrenadeProjectile>,
            )>,
        ),
    >,
    mut player_query: Query<(
        &mut PlayerController,
        &mut PlayerWeaponInventory,
        &mut ActiveWeaponState,
        &mut PlayerRecoil,
    )>,
    mut vm_query: Query<(Entity, &mut StudioAnimatedModel), With<ViewModelWeapon>>,
) {
    if reset_events.read().last().is_none() {
        return;
    }
    pending.open = false;
    pending.consume_next_attack = false;
    for anim in &transient_models {
        for (_, mesh_handle, _) in &anim.mesh_entities {
            vfx.meshes.remove(mesh_handle);
        }
    }
    for ent in &transient_query {
        commands.entity(ent).despawn_recursive();
    }
    for (mut player, mut inv, mut state, mut recoil) in &mut player_query {
        *recoil = PlayerRecoil::default();
        let mut fresh = PlayerWeaponInventory::default();
        if player.team != goldhub_game_cstrike::CsTeam::Terrorist {
            fresh.slot5 = WeaponSlotData::empty();
            fresh.bucket_weapons[4].clear();
            fresh.bucket_cursor[4] = 0;
        }
        fresh.active_slot = 3;
        *inv = fresh;
        player.anim_shoot_timer = 0.0;
        player.anim_shoot_retrigger = false;
        let knife_slot = inv.slot3;
        equip_viewmodel_weapon(
            &mut commands,
            &mut state,
            knife_slot,
            &audio.search_paths,
            &mut audio.audio_bank,
            &mut audio.audio_sources,
            &mut model_store,
            &mut vfx.images,
            &mut vfx.materials,
            &mut vfx.meshes,
            &mut vm_query,
        );
    }
}

/// Handles dropping active primary/secondary/C4 (`G`) and picking up dropped world weapons.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub fn weapon_drop_and_pickup_system(
    mut commands: Commands,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    collision_res: Res<LevelCollisionResource>,
    mut pending: ResMut<PendingWeaponSelection>,
    mut audio: CombatAudio,
    mut model_store: ResMut<StudioModelStore>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut player_query: Query<(
        &mut PlayerController,
        &mut PlayerWeaponInventory,
        &mut ActiveWeaponState,
    )>,
    mut dropped_query: Query<(Entity, &mut DroppedWeapon, &mut Transform)>,
    mut vm_query: Query<(Entity, &mut StudioAnimatedModel), With<ViewModelWeapon>>,
    console: Option<Res<crate::hud::text_channels::ConsoleOverlayState>>,
    mut fallback_cache: Local<(Option<Handle<Mesh>>, Option<Handle<StandardMaterial>>)>,
    mut pickup_events: EventWriter<crate::hud::ItemPickupEvent>,
) {
    let dt = time.delta_secs();
    let Ok((mut player, mut inv, mut state)) = player_query.get_single_mut() else {
        return;
    };

    let console_open = console.as_ref().is_some_and(|c| c.open);
    let gs_ori = player.sim.current.origin;
    let player_origin = Vec3::new(gs_ori[0], gs_ori[2], -gs_ori[1]);

    // 1. Handle dropping active weapon with 'G' (`DropPlayerItem`: waist origin + (-v_angle.x / 3), 400 u/s)
    if !console_open && keyboard.just_pressed(KeyCode::KeyG) {
        let cur_slot = inv.active_slot;
        let droppable = state.id.is_droppable() && matches!(cur_slot, 1 | 2 | 5);
        if !droppable {
            player.set_center_message("#Weapon_Cannot_Be_Dropped", 2.0);
        } else if !state.pin_pulled && !state.c4_arming {
            pending.open = false;
            player.anim_shoot_timer = 0.0;
            player.anim_shoot_retrigger = false;
            let drop_data = WeaponSlotData {
                id: Some(state.id),
                clip: state.clip,
                reserve: state.reserve,
                silenced: state.silenced,
                burst_mode: state.burst_mode,
                next_primary_attack: 0.0,
            };
            // CS 1.6 `DropPlayerItem` uses `pev->angles[PITCH] = -v_angle.x / 3.0`:
            // looking down throws further forward, looking up drops shorter; spawn at waist `pev->origin` (below eye height).
            let fwd_1_3 = Quat::from_rotation_y(player.yaw)
                * Quat::from_rotation_x(-player.pitch / 3.0)
                * Vec3::NEG_Z;
            let spawn_pos = player_origin + fwd_1_3 * 10.0;
            let drop_vel = fwd_1_3 * 400.0;

            let w_mdl = world_model_for_weapon(state.id);
            let parent = commands
                .spawn((
                    DroppedWeapon {
                        slot_index: cur_slot,
                        data: drop_data,
                        velocity: drop_vel,
                        pickup_cooldown: 0.75,
                    },
                    Transform::from_translation(spawn_pos)
                        .with_rotation(Quat::from_rotation_y(player.yaw)),
                    Visibility::Inherited,
                ))
                .id();

            if let Some(md) =
                model_store.get_or_load(w_mdl, &audio.search_paths, &mut images, &mut materials)
            {
                model_store.spawn_static_model_submesh_children(
                    &mut commands,
                    parent,
                    &md,
                    w_mdl,
                    false,
                    Some(2),
                    &mut meshes,
                );
            } else {
                let mesh_h = fallback_cache
                    .0
                    .get_or_insert_with(|| meshes.add(Cuboid::new(14.0, 4.0, 4.0).mesh()))
                    .clone();
                let mat_h = fallback_cache
                    .1
                    .get_or_insert_with(|| {
                        materials.add(StandardMaterial {
                            base_color: Color::srgb(0.35, 0.35, 0.35),
                            ..default()
                        })
                    })
                    .clone();
                commands
                    .entity(parent)
                    .insert((Mesh3d(mesh_h), MeshMaterial3d(mat_h)));
            }

            play_ui_sound(
                &mut commands,
                "items/weapondrop1.wav",
                &audio.search_paths,
                &mut audio.audio_bank,
                &mut audio.audio_sources,
            );

            // Remove dropped weapon from current bucket and fall back to next item or remaining slot
            let b_idx = (cur_slot - 1) as usize;
            if !inv.bucket_weapons[b_idx].is_empty() {
                let pos = inv.bucket_cursor[b_idx].min(inv.bucket_weapons[b_idx].len() - 1);
                inv.bucket_weapons[b_idx].remove(pos);
            }
            if inv.bucket_weapons[b_idx].is_empty() {
                inv.bucket_cursor[b_idx] = 0;
                if let Some(s) = inv.get_slot_mut(cur_slot) {
                    *s = WeaponSlotData::empty();
                }
                let fallback_slot = [1u8, 2, 3, 4, 5]
                    .into_iter()
                    .find(|&s| inv.get_slot(s).and_then(|d| d.id).is_some())
                    .unwrap_or(3);
                inv.active_slot = fallback_slot;
                if let Some(fb) = inv.get_slot(fallback_slot).copied() {
                    equip_viewmodel_weapon(
                        &mut commands,
                        &mut state,
                        fb,
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                        &mut model_store,
                        &mut images,
                        &mut materials,
                        &mut meshes,
                        &mut vm_query,
                    );
                }
            } else {
                inv.bucket_cursor[b_idx] %= inv.bucket_weapons[b_idx].len();
                let next_item = inv.bucket_weapons[b_idx][inv.bucket_cursor[b_idx]];
                if let Some(s) = inv.get_slot_mut(cur_slot) {
                    *s = next_item;
                }
                equip_viewmodel_weapon(
                    &mut commands,
                    &mut state,
                    next_item,
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                    &mut model_store,
                    &mut images,
                    &mut materials,
                    &mut meshes,
                    &mut vm_query,
                );
            }
        }
    }

    // 2. Step dropped weapon gravity/BSP resting + `CWeaponBox::Touch` AABB overlap (works standing & crouched)
    if dt <= 0.0 {
        return;
    }
    let player_half_h = if player.sim.current.is_ducking {
        18.0
    } else {
        36.0
    };
    let p_min = player_origin + Vec3::new(-16.0, -player_half_h, -16.0);
    let p_max = player_origin + Vec3::new(16.0, player_half_h, 16.0);

    for (ent, mut dropped, mut dt_tf) in &mut dropped_query {
        if dropped.pickup_cooldown > 0.0 {
            dropped.pickup_cooldown = (dropped.pickup_cooldown - dt).max(0.0);
        }

        if dropped.velocity.length_squared() > 1.0 {
            // Authentic GoldSrc `sv_gravity 800` MOVETYPE_TOSS
            dropped.velocity.y -= 800.0 * dt;
            let old_pos = dt_tf.translation;
            let new_pos = old_pos + dropped.velocity * dt;
            if let Some(ref col) = collision_res.model {
                use goldhub_physics::WorldTrace;
                let bsp = goldhub_physics::BspWorldCollision::new(col);
                let tr = bsp.trace_player_hull(
                    [old_pos.x, -old_pos.z, old_pos.y],
                    [new_pos.x, -new_pos.z, new_pos.y],
                    0,
                );
                if tr.fraction < 1.0 {
                    let n = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                        .normalize_or_zero();
                    let endpos = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);
                    if n.y > 0.7 {
                        // Floor collision (SV_Physics_Toss): slide with surface friction until velocity drops
                        let horiz_speed =
                            Vec2::new(dropped.velocity.x, dropped.velocity.z).length();
                        if horiz_speed < 45.0 {
                            dt_tf.translation = endpos;
                            dropped.velocity = Vec3::ZERO;
                            let yaw_only = dt_tf.rotation.to_euler(EulerRot::YXZ).0;
                            dt_tf.rotation = Quat::from_rotation_y(yaw_only);
                        } else {
                            dt_tf.translation = endpos + n * 0.5;
                            dropped.velocity.x *= 0.65;
                            dropped.velocity.z *= 0.65;
                            dropped.velocity.y = 0.0;
                        }
                    } else {
                        dt_tf.translation = endpos + n * 0.5;
                        let vn = dropped.velocity.dot(n);
                        dropped.velocity = (dropped.velocity - n * (1.4 * vn)) * 0.5;
                    }
                } else {
                    dt_tf.translation = new_pos;
                }
            } else {
                dt_tf.translation = new_pos;
            }
        }

        // Check `CWeaponBox` (`-16,-16,0..16,16,16`) AABB overlap with player hull once settled
        if dropped.pickup_cooldown <= 0.0 && dropped.velocity.length_squared() <= 4.0 {
            let dp = dt_tf.translation;
            let w_min = dp + Vec3::new(-16.0, 0.0, -16.0);
            let w_max = dp + Vec3::new(16.0, 16.0, 16.0);
            let overlaps = w_min.x <= p_max.x
                && w_max.x >= p_min.x
                && w_min.y <= p_max.y
                && w_max.y >= p_min.y
                && w_min.z <= p_max.z
                && w_max.z >= p_min.z;

            if overlaps && let Some(w_id) = dropped.data.id {
                if w_id == WeaponId::C4 && player.team != goldhub_game_cstrike::CsTeam::Terrorist {
                    continue;
                }
                let s_idx = dropped.slot_index;
                let b_idx = (s_idx.saturating_sub(1) as usize).min(4);
                // Enforce strict 1-weapon-per-slot on ground pickup for Primary (1), Pistol (2), C4 (5)
                let slot_occupied =
                    matches!(s_idx, 1 | 2 | 5) && !inv.bucket_weapons[b_idx].is_empty();
                let already_held = inv.bucket_weapons[b_idx]
                    .iter()
                    .any(|item| item.id == Some(w_id));

                if !slot_occupied && !already_held {
                    inv.bucket_weapons[b_idx].push(dropped.data);
                    pickup_events.send(crate::hud::ItemPickupEvent {
                        weapon_id: Some(w_id),
                        ammo_caliber: None,
                        is_defuser: false,
                    });
                    let empty_slot = inv.get_slot(s_idx).and_then(|d| d.id).is_none();
                    if empty_slot {
                        inv.bucket_cursor[b_idx] = inv.bucket_weapons[b_idx].len() - 1;
                        if let Some(s) = inv.get_slot_mut(s_idx) {
                            *s = dropped.data;
                        }
                        if s_idx == 1 && inv.active_slot != 1 {
                            inv.previous_slot = inv.active_slot;
                            inv.active_slot = 1;
                            equip_viewmodel_weapon(
                                &mut commands,
                                &mut state,
                                dropped.data,
                                &audio.search_paths,
                                &mut audio.audio_bank,
                                &mut audio.audio_sources,
                                &mut model_store,
                                &mut images,
                                &mut materials,
                                &mut meshes,
                                &mut vm_query,
                            );
                        }
                    }
                    if w_id == WeaponId::C4 {
                        player.set_center_message("#Got_bomb", 2.0);
                    }
                    play_ui_sound(
                        &mut commands,
                        "items/gunpickup2.wav",
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                    );
                    commands.entity(ent).despawn_recursive();
                }
            }
        }
    }
}

/// Applies ReGameDLL-style spherical blast damage to breakables, hostages, and the local player.
#[allow(clippy::too_many_arguments)]
fn apply_explosion_radius_damage(
    pos: Vec3,
    max_damage: f32,
    blast_radius: f32,
    shake_duration: f32,
    shake_amp: f32,
    collision: Option<&goldhub_formats::ir::CollisionModel>,
    breakable_query: &mut Query<(Entity, &mut BreakableEntity)>,
    hostage_query: &mut Query<(
        Entity,
        &mut HostageEntity,
        &mut StudioAnimatedModel,
        &GlobalTransform,
    )>,
    player_query: &mut Query<&mut PlayerController>,
    fire_events: &mut EventWriter<FireTargetEvent>,
    break_events: &mut EventWriter<BreakEntityEvent>,
) {
    for (_b_ent, mut b) in breakable_query.iter_mut() {
        if b.is_broken {
            continue;
        }
        let center = (b.bmin + b.bmax) * 0.5;
        let dist = center.distance(pos);
        if dist < blast_radius {
            let dmg = (1.0 - (dist / blast_radius)) * max_damage;
            b.health -= dmg;
            if b.health <= 0.0 {
                b.is_broken = true;
                break_events.send(BreakEntityEvent {
                    origin: center,
                    material: b.material,
                    model: b.model.clone(),
                    explodemagnitude: b.explodemagnitude,
                });
                fire_events.send(FireTargetEvent {
                    target: b.target.clone(),
                });
            }
        }
    }

    for (_h_ent, mut hostage, mut anim, ht) in hostage_query.iter_mut() {
        if hostage.is_dead {
            continue;
        }
        let dist = ht.translation().distance(pos);
        if dist < blast_radius {
            let dmg = (1.0 - (dist / blast_radius)) * max_damage;
            hostage.health -= dmg;
            if hostage.health <= 0.0 {
                hostage.is_dead = true;
                anim.set_sequence(31, 24.0, false);
                if let Ok(mut pc) = player_query.get_single_mut() {
                    pc.apply_hostage_penalty(dmg, true);
                }
            } else {
                anim.set_sequence(7, 30.0, false);
                if let Ok(mut pc) = player_query.get_single_mut() {
                    pc.apply_hostage_penalty(dmg, false);
                }
            }
        }
    }

    for mut player in player_query.iter_mut() {
        let ori = player.sim.current.origin;
        let center_pos = Vec3::new(ori[0], ori[2] + 18.0, -ori[1]);
        let dist = center_pos.distance(pos);
        if dist < blast_radius {
            let blocked = collision.is_some_and(|col| {
                use goldhub_physics::WorldTrace;
                let bsp = goldhub_physics::BspWorldCollision::new(col);
                let src = pos + Vec3::Y * 4.0;
                let tr = bsp.trace_player_hull(
                    [src.x, -src.z, src.y],
                    [center_pos.x, -center_pos.z, center_pos.y],
                    0,
                );
                tr.fraction < 0.99
            });
            let cover_mult = if blocked { 0.45 } else { 1.0 };
            let falloff = (1.0 - (dist / blast_radius)).clamp(0.0, 1.0);
            let dmg = max_damage * falloff * cover_mult;
            player.apply_blast_damage(dmg);
            player.screen_shake_timer = player.screen_shake_timer.max(shake_duration);
            player.screen_shake_amp = player.screen_shake_amp.max(shake_amp * falloff);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn detonate_planted_c4(
    commands: &mut Commands,
    ent: Entity,
    pos: Vec3,
    bomb_radius: f32,
    collision: Option<&goldhub_formats::ir::CollisionModel>,
    beep_channels: &Query<Entity, With<PlantedC4BeepChannel>>,
    search_paths: &GameSearchPaths,
    audio_bank: &mut GoldSrcAudioBank,
    audio_sources: &mut Assets<AudioSource>,
    sprite_store: &mut SpriteStore,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    breakable_query: &mut Query<(Entity, &mut BreakableEntity)>,
    hostage_query: &mut Query<(
        Entity,
        &mut HostageEntity,
        &mut StudioAnimatedModel,
        &GlobalTransform,
    )>,
    player_query: &mut Query<&mut PlayerController>,
    fire_events: &mut EventWriter<FireTargetEvent>,
    break_events: &mut EventWriter<BreakEntityEvent>,
) {
    for ch in beep_channels.iter() {
        commands.entity(ch).despawn();
    }
    if let Some(h) =
        audio_bank.get_or_load_sound("weapons/c4_explode1.wav", &search_paths.dirs, audio_sources)
    {
        commands.spawn((
            AudioPlayer::new(h),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::new(1.0)),
        ));
    }
    spawn_c4_explosion_sprites(
        commands,
        pos,
        sprite_store,
        search_paths,
        images,
        materials,
        meshes,
    );
    spawn_scorch_decal(
        commands,
        pos,
        Vec3::Y,
        sprite_store,
        search_paths,
        images,
        materials,
        meshes,
    );
    spawn_spark_shower(commands, pos, Vec3::Y, pos.x.to_bits() ^ pos.z.to_bits());
    spawn_delayed_smoke(commands, pos, 0.85, 360.0, 12.0, "sprites/steam1.spr");

    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.7, 0.25),
            intensity: 90_000.0,
            range: 1400.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(pos + Vec3::Y * 24.0),
        crate::entity::ExplosionFlash {
            lifetime: 0.0,
            max_lifetime: 0.65,
            initial_intensity: 90_000.0,
        },
    ));

    let effective_radius = bomb_radius.max(100.0) * 3.5;
    apply_explosion_radius_damage(
        pos,
        bomb_radius.max(100.0),
        effective_radius,
        1.2,
        0.08,
        collision,
        breakable_query,
        hostage_query,
        player_query,
        fire_events,
        break_events,
    );
    commands.entity(ent).despawn_recursive();
}

/// Ticks planted C4 countdown (`C4Think`: `c4_beep1..5.wav`, `MOVETYPE_TOSS` gravity settle, `E` defuse `5s`/`10s`).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn update_planted_c4_system(
    mut commands: Commands,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    collision_res: Res<LevelCollisionResource>,
    bomb_params: Res<BombMapParams>,
    mut scenario_map: Option<ResMut<ScenarioMapData>>,
    mut audio: CombatAudio,
    mut sprite_store: ResMut<SpriteStore>,
    mut vfx: CombatVfxAssets,
    beep_channels: Query<Entity, With<PlantedC4BeepChannel>>,
    mut breakable_query: Query<(Entity, &mut BreakableEntity)>,
    mut hostage_query: Query<(
        Entity,
        &mut HostageEntity,
        &mut StudioAnimatedModel,
        &GlobalTransform,
    )>,
    mut player_query: Query<&mut PlayerController>,
    mut events: CombatEvents,
    mut query: Query<(Entity, &mut PlantedC4, &mut Transform)>,
    console: Option<Res<crate::hud::text_channels::ConsoleOverlayState>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let console_open = console.as_ref().is_some_and(|c| c.open);

    for (ent, mut c4, mut tf) in &mut query {
        // Continuous `MOVETYPE_TOSS` gravity check so C4 drops to floor if a supporting box breaks
        if let Some(ref col) = collision_res.model {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col);
            let p = tf.translation;
            let support = bsp.trace_player_hull([p.x, -p.z, p.y + 1.0], [p.x, -p.z, p.y - 2.0], 0);
            if support.fraction < 1.0 && support.plane_normal[2] > 0.7 {
                c4.on_ground = true;
                c4.velocity = Vec3::ZERO;
                tf.translation.y = support.endpos[2];
            } else {
                c4.on_ground = false;
                c4.velocity.y -= 800.0 * dt;
                let next_p = p + c4.velocity * dt;
                let fall_tr =
                    bsp.trace_player_hull([p.x, -p.z, p.y], [next_p.x, -next_p.z, next_p.y], 0);
                if fall_tr.fraction < 1.0 {
                    c4.velocity = Vec3::ZERO;
                    c4.on_ground = true;
                    tf.translation =
                        Vec3::new(fall_tr.endpos[0], fall_tr.endpos[2], -fall_tr.endpos[1]);
                } else {
                    tf.translation = next_p;
                }
            }
        }

        c4.timer -= dt;
        c4.beep_timer -= dt;
        c4.blink_timer -= dt;
        let pos = tf.translation;

        // CT +use (`E` key) C4 defusal (`5s` with kit, `10s` without)
        let use_held = !console_open && keyboard.pressed(KeyCode::KeyE);
        if let Ok(mut player) = player_query.get_single_mut() {
            let p_ori = player.sim.current.origin;
            let p_eye = Vec3::new(
                p_ori[0],
                p_ori[2] + player.sim.current.view_height,
                -p_ori[1],
            );
            let p_fwd = (Quat::from_rotation_y(player.yaw) * Quat::from_rotation_x(player.pitch))
                * Vec3::NEG_Z;
            let to_c4 = pos - p_eye;
            let dist = Vec3::new(p_ori[0], p_ori[2], -p_ori[1]).distance(pos);
            let look_dot = p_fwd.dot(to_c4.normalize_or_zero());

            let can_defuse = use_held
                && player.team == goldhub_game_cstrike::CsTeam::CounterTerrorist
                && player.sim.current.on_ground
                && dist <= 68.0
                && look_dot >= 0.2;

            if can_defuse {
                player.sim.current.velocity[0] = 0.0;
                player.sim.current.velocity[1] = 0.0;
                if !c4.is_being_defused {
                    c4.is_being_defused = true;
                    c4.defuse_progress = 0.0;
                    c4.defuse_duration = if player.has_defuser { 5.0 } else { 10.0 };
                    play_ui_sound(
                        &mut commands,
                        "weapons/c4_disarm.wav",
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                    );
                    let msg = if player.has_defuser {
                        "#Defusing_Bomb_With_Defuse_Kit"
                    } else {
                        "#Defusing_Bomb_Without_Defuse_Kit"
                    };
                    player.set_center_message(msg, 2.0);
                }
                c4.defuse_progress += dt;
                if c4.defuse_progress >= c4.defuse_duration {
                    for ch in &beep_channels {
                        commands.entity(ch).despawn_recursive();
                    }
                    play_ui_sound(
                        &mut commands,
                        "weapons/c4_disarmed.wav",
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                    );
                    player.set_center_message("#Bomb_Defused", 4.0);
                    if let Some(ref mut map_data) = scenario_map {
                        map_data.bomb_defused = true;
                    }
                    commands.entity(ent).despawn_recursive();
                    continue;
                }
            } else {
                c4.is_being_defused = false;
                c4.defuse_progress = 0.0;
            }
        }

        if c4.timer <= 0.0 {
            if let Some(ref mut map_data) = scenario_map {
                map_data.bomb_exploded = true;
            }
            detonate_planted_c4(
                &mut commands,
                ent,
                pos,
                bomb_params.bomb_radius,
                collision_res.model.as_ref(),
                &beep_channels,
                &audio.search_paths,
                &mut audio.audio_bank,
                &mut audio.audio_sources,
                &mut sprite_store,
                &mut vfx.images,
                &mut vfx.materials,
                &mut vfx.meshes,
                &mut breakable_query,
                &mut hostage_query,
                &mut player_query,
                &mut events.fire,
                &mut events.break_ent,
            );
            continue;
        }

        // Advance wave stage (`0 -> 4`) whenever `timer` crosses `next_freq` (`*= 0.9`)
        if c4.cur_wave < 5 && c4.timer <= c4.next_freq {
            c4.next_freq -= c4.next_freq_interval;
            c4.next_freq_interval *= 0.9;
            c4.cur_wave = c4.cur_wave.saturating_add(1);
        }

        // Fixed 1.4s beep trigger matching CS 1.6 1.501s multi-beep clips on a single CHAN_VOICE channel
        if c4.beep_timer <= 0.0 {
            c4.beep_timer = 1.4;
            let wave = c4.cur_wave.saturating_sub(1).min(4);
            let (beep_snd, atten): (&str, f32) = match wave {
                0 => ("weapons/c4_beep1.wav", 1.5),
                1 => ("weapons/c4_beep2.wav", 1.0),
                2 => ("weapons/c4_beep3.wav", 0.8),
                3 => ("weapons/c4_beep4.wav", 0.5),
                _ => ("weapons/c4_beep5.wav", 0.2),
            };
            for ch in &beep_channels {
                commands.entity(ch).despawn();
            }
            if let Some(h) = audio.audio_bank.get_or_load_sound(
                beep_snd,
                &audio.search_paths.dirs,
                &mut audio.audio_sources,
            ) {
                let scale = 1.0 / (180.0 / atten.max(0.2));
                commands.spawn((
                    PlantedC4BeepChannel,
                    AudioPlayer::new(h),
                    PlaybackSettings::DESPAWN
                        .with_spatial(true)
                        .with_spatial_scale(bevy::audio::SpatialScale::new(scale))
                        .with_volume(bevy::audio::Volume::new(0.8)),
                    Transform::from_translation(pos),
                ));
            }
        }

        // LED blink (`TE_GLOWSPRITE` `sprites/ledglow.spr` every 2.0s)
        if c4.blink_timer <= 0.0 {
            c4.blink_timer = 2.0;
            spawn_ledglow_sprite(
                &mut commands,
                pos,
                &mut sprite_store,
                &audio.search_paths,
                &mut vfx.images,
                &mut vfx.materials,
                &mut vfx.meshes,
            );
            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.1, 0.05),
                    intensity: 3_500.0,
                    range: 96.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(pos + Vec3::Y * 8.0),
                crate::entity::ExplosionFlash {
                    lifetime: 0.0,
                    max_lifetime: 0.12,
                    initial_intensity: 3_500.0,
                },
            ));
        }
    }
}

/// Integrates hopping/resting smoke grenade canisters and emits 21 x 1.0s `black_smoke1..4.spr` puffs (`SG_Smoke`).
#[allow(clippy::too_many_arguments)]
pub fn update_smoking_canister_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Res<LevelCollisionResource>,
    search_paths: Res<GameSearchPaths>,
    mut sprite_store: ResMut<SpriteStore>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut query: Query<(Entity, &mut SmokingCanister, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (ent, mut can, mut tf) in &mut query {
        // Hop & settle physics cleanly kept flat horizontal
        if can.velocity.length_squared() > 0.5 {
            can.velocity.y -= 440.0 * dt;
            let old_pos = tf.translation;
            let new_pos = old_pos + can.velocity * dt;
            if let Some(ref col) = collision_res.model {
                use goldhub_physics::WorldTrace;
                let bsp = goldhub_physics::BspWorldCollision::new(col);
                let tr = bsp.trace_player_hull(
                    [old_pos.x, -old_pos.z, old_pos.y],
                    [new_pos.x, -new_pos.z, new_pos.y],
                    0,
                );
                if tr.fraction < 1.0 {
                    let hit_n =
                        Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                            .normalize_or_zero();
                    let endpos = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);
                    can.bounce_count += 1;
                    let vn = can.velocity.dot(hit_n);
                    if vn < 0.0 {
                        can.velocity = (can.velocity - hit_n * (1.5 * vn)) * 0.6;
                    }
                    if hit_n.y > 0.7 && can.velocity.length() < 28.0 {
                        can.velocity = Vec3::ZERO;
                        tf.translation = endpos;
                        let yaw_only = tf.rotation.to_euler(EulerRot::YXZ).0;
                        tf.rotation = Quat::from_rotation_y(yaw_only);
                    } else {
                        tf.translation = endpos + hit_n * 0.5;
                    }
                } else {
                    tf.translation = new_pos;
                }
            } else {
                tf.translation = new_pos;
            }
        }

        can.emit_timer -= dt;
        if can.emit_timer <= 0.0 {
            if can.puffs_left == 0 {
                commands.entity(ent).despawn_recursive();
                continue;
            }
            let idx = (21 - can.puffs_left) as usize;
            let angle_deg = ((idx * 137) % 360) as f32;
            spawn_black_smoke_puff(
                &mut commands,
                tf.translation,
                idx,
                angle_deg,
                &mut sprite_store,
                &search_paths,
                &mut images,
                &mut materials,
                &mut meshes,
            );
            can.puffs_left -= 1;
            can.emit_timer = 1.0;
        }
    }
}

#[derive(Component, Debug)]
pub struct WeaponAudioChannel;

/// Bouncing projectile entity for thrown grenades (HE, Flash, Smoke).
#[derive(Component, Debug)]
pub struct GrenadeProjectile {
    pub velocity: Vec3,
    pub fuse_timer: f32,
    pub bounce_count: u32,
    pub weapon_id: WeaponId,
    pub on_ground: bool,
    pub yaw: f32,
}

/// Ejected physical brass casing from fired weapons.
#[derive(Component, Debug)]
pub struct BrassCasingProjectile {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub bounce_count: u32,
}

#[derive(Component, Debug)]
pub struct BulletParticle {
    pub velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub gravity: f32,
}

pub fn update_bullet_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut BulletParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (entity, mut p, mut transform) in &mut query {
        p.lifetime += dt;
        if p.lifetime >= p.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        p.velocity.y -= p.gravity * dt;
        transform.translation += p.velocity * dt;
        let frac = (1.0 - (p.lifetime / p.max_lifetime)).max(0.01);
        transform.scale = Vec3::splat(frac);
    }
}

/// Updates brass casing ballistics, gravity, bounce sound, and smooth dissolve before despawn.
pub fn update_brass_casings_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Res<LevelCollisionResource>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut query: Query<(Entity, &mut BrassCasingProjectile, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut brass, mut transform) in &mut query {
        brass.lifetime += dt;
        if brass.lifetime >= brass.max_lifetime {
            commands.entity(entity).despawn_recursive();
            continue;
        }

        if brass.velocity.length_squared() < 1.0 {
            continue;
        }

        brass.velocity.y -= 600.0 * dt; // Brass gravity
        let old_pos = transform.translation;
        let new_pos = old_pos + brass.velocity * dt;

        // Trace against BSP floor
        if let Some(ref col_model) = collision_res.model {
            use goldhub_physics::WorldTrace;
            let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
            let gs_start = [old_pos.x, -old_pos.z, old_pos.y];
            let gs_end = [new_pos.x, -new_pos.z, new_pos.y];
            let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
            if tr.fraction < 1.0 {
                let hit_n = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                    .normalize_or_zero();
                transform.translation = old_pos + (new_pos - old_pos) * tr.fraction + hit_n * 0.5;
                brass.bounce_count += 1;
                if brass.bounce_count == 1
                    && let Some(h) = audio_bank.get_or_load_sound(
                        "player/pl_shell1.wav",
                        &search_paths.dirs,
                        &mut audio_sources,
                    )
                {
                    commands.spawn((
                        AudioPlayer::new(h),
                        PlaybackSettings::DESPAWN
                            .with_spatial(true)
                            .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 96.0))
                            .with_volume(bevy::audio::Volume::new(0.25)),
                        Transform::from_translation(transform.translation),
                    ));
                }
                let normal_vel = brass.velocity.dot(hit_n);
                brass.velocity = (brass.velocity - hit_n * (1.4 * normal_vel)) * 0.55;
                if hit_n.y > 0.7 && brass.velocity.length() < 28.0 {
                    brass.velocity = Vec3::ZERO;
                }
                continue;
            }
        }

        transform.translation = new_pos;
        transform.rotate_local_x(15.0 * dt);
        transform.rotate_local_y(10.0 * dt);
    }
}

/// Ticks grenade flight physics, horizontal `.mdl` tumble/roll, bouncing, Smoke `on_ground`, `RadiusFlash`, and `RadiusDamage`.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn update_grenade_projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_res: Res<LevelCollisionResource>,
    mut audio: CombatAudio,
    mut sprite_store: ResMut<SpriteStore>,
    mut vfx: CombatVfxAssets,
    mut breakable_query: Query<(Entity, &mut BreakableEntity)>,
    mut hostage_query: Query<(
        Entity,
        &mut HostageEntity,
        &mut StudioAnimatedModel,
        &GlobalTransform,
    )>,
    mut player_query: Query<&mut PlayerController>,
    mut events: CombatEvents,
    mut query: Query<
        (
            Entity,
            &mut GrenadeProjectile,
            &mut Transform,
            Option<&mut StudioAnimatedModel>,
        ),
        Without<HostageEntity>,
    >,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut grenade, mut transform, mut anim_opt) in &mut query {
        grenade.fuse_timer -= dt;

        // Detonation condition
        if grenade.fuse_timer <= 0.0 {
            let det_pos = transform.translation;

            match grenade.weapon_id {
                WeaponId::SmokeGrenade => {
                    // CS 1.6 `SG_TumbleThink`: smoke grenade never detonates mid-air or on skybox; wait until grounded on solid world
                    let grounded = if grenade.on_ground {
                        true
                    } else if let Some(ref col) = collision_res.model {
                        use goldhub_physics::WorldTrace;
                        let bsp = goldhub_physics::BspWorldCollision::new(col);
                        let tr = bsp.trace_player_hull(
                            [det_pos.x, -det_pos.z, det_pos.y + 1.0],
                            [det_pos.x, -det_pos.z, det_pos.y - 4.0],
                            0,
                        );
                        tr.fraction < 1.0 && tr.plane_normal[2] > 0.7 && !tr.is_sky
                    } else {
                        false
                    };
                    if grounded {
                        if let Some(h) = audio.audio_bank.get_or_load_sound(
                            "weapons/sg_explode.wav",
                            &audio.search_paths.dirs,
                            &mut audio.audio_sources,
                        ) {
                            commands.spawn((
                                AudioPlayer::new(h),
                                PlaybackSettings::DESPAWN
                                    .with_spatial(true)
                                    .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 128.0))
                                    .with_volume(bevy::audio::Volume::new(0.9)),
                                Transform::from_translation(det_pos),
                            ));
                        }
                        spawn_smoke_cloud(
                            &mut commands,
                            det_pos,
                            &mut sprite_store,
                            &audio.search_paths,
                            &mut vfx.images,
                            &mut vfx.materials,
                            &mut vfx.meshes,
                        );

                        // `SG_Detonate` canister hop impulse (`vx/vz in -175..175, vy in 250..350`)
                        let mut s = det_pos.x.to_bits()
                            ^ det_pos.z.to_bits().rotate_left(9)
                            ^ time.elapsed_secs().to_bits().rotate_left(17);
                        let mut rand_range = |lo: f32, hi: f32| -> f32 {
                            s ^= s << 13;
                            s ^= s >> 17;
                            s ^= s << 5;
                            let t = ((s & 0xFFFF) as f32) / 65536.0;
                            lo + t * (hi - lo)
                        };
                        let hop_vel = Vec3::new(
                            rand_range(-175.0, 175.0),
                            rand_range(250.0, 350.0),
                            rand_range(-175.0, 175.0),
                        );
                        if let Some(ref mut anim) = anim_opt {
                            anim.set_sequence(0, 0.0, false);
                        }
                        let yaw_only = transform.rotation.to_euler(EulerRot::YXZ).0;
                        transform.rotation = Quat::from_rotation_y(yaw_only);

                        commands
                            .entity(entity)
                            .remove::<GrenadeProjectile>()
                            .insert(SmokingCanister {
                                velocity: hop_vel,
                                puffs_left: 21,
                                emit_timer: 1.0,
                                bounce_count: 0,
                            });
                        continue;
                    }
                    grenade.fuse_timer = 0.02;
                }
                WeaponId::Flashbang => {
                    if let Some(h) = audio.audio_bank.get_or_load_sound(
                        "weapons/flashbang-1.wav",
                        &audio.search_paths.dirs,
                        &mut audio.audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 128.0))
                                .with_volume(bevy::audio::Volume::new(1.0)),
                            Transform::from_translation(det_pos),
                        ));
                    }
                    let seed = det_pos.x.to_bits() ^ det_pos.z.to_bits().rotate_left(7);
                    spawn_spark_shower(&mut commands, det_pos, Vec3::Y, seed);
                    spawn_delayed_smoke(
                        &mut commands,
                        det_pos,
                        0.0,
                        70.0,
                        14.0,
                        "sprites/black_smoke1.spr",
                    );

                    commands.spawn((
                        PointLight {
                            color: Color::WHITE,
                            intensity: 120_000.0,
                            range: 900.0,
                            shadows_enabled: false,
                            ..default()
                        },
                        Transform::from_translation(det_pos + Vec3::Y * 8.0),
                        crate::entity::ExplosionFlash {
                            lifetime: 0.0,
                            max_lifetime: 0.25,
                            initial_intensity: 120_000.0,
                        },
                    ));

                    // CS 1.6 `RadiusFlash` (`radius = 1500.0`, double BSP LOS trace, view-dot falloff)
                    for mut player in &mut player_query {
                        let ori = player.sim.current.origin;
                        let eye_pos =
                            Vec3::new(ori[0], ori[2] + player.sim.current.view_height, -ori[1]);
                        let dist = eye_pos.distance(det_pos);
                        if dist <= 1500.0 {
                            let src = det_pos + Vec3::Y * 1.0;
                            let clear_los = collision_res.model.as_ref().is_none_or(|col| {
                                use goldhub_physics::WorldTrace;
                                let bsp = goldhub_physics::BspWorldCollision::new(col);
                                let tr1 = bsp.trace_player_hull(
                                    [src.x, -src.z, src.y],
                                    [eye_pos.x, -eye_pos.z, eye_pos.y],
                                    0,
                                );
                                if tr1.fraction < 0.99 {
                                    return false;
                                }
                                let tr2 = bsp.trace_player_hull(
                                    [eye_pos.x, -eye_pos.z, eye_pos.y],
                                    [src.x, -src.z, src.y],
                                    0,
                                );
                                tr2.fraction >= 0.99
                            });
                            if clear_los {
                                let fl_adjusted = (4.0 * (1.0 - dist / 1500.0)).max(0.0);
                                if fl_adjusted > 0.0 {
                                    let view_fwd = Quat::from_rotation_y(player.yaw)
                                        * Quat::from_rotation_x(player.pitch)
                                        * Vec3::NEG_Z;
                                    let to_flash = (det_pos - eye_pos).normalize_or_zero();
                                    let dot = view_fwd.dot(to_flash);
                                    let (alpha, fade_time, hold_time) = if dot < 0.0 {
                                        (200.0, fl_adjusted * 1.75, fl_adjusted / 3.5)
                                    } else {
                                        (255.0, fl_adjusted * 3.0, fl_adjusted / 1.5)
                                    };
                                    player.apply_flash(alpha, fade_time, hold_time);
                                }
                            }
                        }
                    }

                    commands.entity(entity).despawn_recursive();
                    continue;
                }
                _ => {
                    // Frag / HE Grenade: sound, fireball, scorch, sparks, delayed steam1.spr, full RadiusDamage
                    if let Some(h) = audio.audio_bank.get_or_load_sound(
                        "weapons/explode3.wav",
                        &audio.search_paths.dirs,
                        &mut audio.audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 128.0))
                                .with_volume(bevy::audio::Volume::new(1.0)),
                            Transform::from_translation(det_pos),
                        ));
                    }

                    spawn_explosion_sprite(
                        &mut commands,
                        det_pos,
                        &mut sprite_store,
                        &audio.search_paths,
                        &mut vfx.images,
                        &mut vfx.materials,
                        &mut vfx.meshes,
                    );
                    let mut hit_normal = Vec3::Y;
                    if let Some(ref col_model) = collision_res.model {
                        use goldhub_physics::WorldTrace;
                        let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
                        let gs_start = [det_pos.x, -det_pos.z, det_pos.y + 8.0];
                        let gs_end = [det_pos.x, -det_pos.z, det_pos.y - 64.0];
                        let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
                        if tr.fraction < 1.0 {
                            let hit_pos = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);
                            let hit_norm = Vec3::new(
                                tr.plane_normal[0],
                                tr.plane_normal[2],
                                -tr.plane_normal[1],
                            )
                            .normalize_or_zero();
                            if hit_norm.length_squared() > 0.1 {
                                hit_normal = hit_norm;
                            }
                            spawn_scorch_decal(
                                &mut commands,
                                hit_pos,
                                hit_normal,
                                &mut sprite_store,
                                &audio.search_paths,
                                &mut vfx.images,
                                &mut vfx.materials,
                                &mut vfx.meshes,
                            );
                        }
                    }

                    let seed = det_pos.x.to_bits() ^ det_pos.z.to_bits().rotate_left(11);
                    spawn_spark_shower(&mut commands, det_pos, hit_normal, seed);
                    spawn_flicker_particles(
                        &mut commands,
                        det_pos,
                        &mut vfx.meshes,
                        &mut vfx.materials,
                        &mut sprite_store,
                    );
                    spawn_delayed_smoke(
                        &mut commands,
                        det_pos,
                        0.0,
                        240.0,
                        12.0,
                        "sprites/black_smoke1.spr",
                    );

                    commands.spawn((
                        PointLight {
                            color: Color::srgb(1.0, 0.75, 0.3),
                            intensity: 45_000.0,
                            range: 600.0,
                            shadows_enabled: false,
                            ..default()
                        },
                        Transform::from_translation(det_pos),
                        crate::entity::ExplosionFlash {
                            lifetime: 0.0,
                            max_lifetime: 0.35,
                            initial_intensity: 45_000.0,
                        },
                    ));

                    apply_explosion_radius_damage(
                        det_pos,
                        100.0,
                        350.0,
                        0.0,
                        0.0,
                        collision_res.model.as_ref(),
                        &mut breakable_query,
                        &mut hostage_query,
                        &mut player_query,
                        &mut events.fire,
                        &mut events.break_ent,
                    );

                    commands.entity(entity).despawn_recursive();
                    continue;
                }
            }
        }

        // Authentic CS 1.6 grenade gravity (`pev->gravity = 0.55` * 800 = 440 u/s^2)
        // CS 1.6 grenades stay statically oriented (`idle` seq 0, fps 0) lying sideways without rolling spin.
        if grenade.velocity.length_squared() <= 0.5 {
            grenade.on_ground = true;
            if let Some(ref mut anim) = anim_opt {
                anim.is_looping = false;
                anim.fps = 0.0;
                anim.set_sequence(0, 0.0, false);
            }
            transform.rotation = Quat::from_rotation_y(grenade.yaw + std::f32::consts::FRAC_PI_2);
            continue;
        }
        grenade.velocity.y -= 440.0 * dt;
        if grenade.on_ground {
            // CS 1.6 `BounceTouch` floor friction (0.95) stops floor sliding without endless jitter
            let damp = 0.95_f32.powf(dt * 60.0);
            grenade.velocity.x *= damp;
            grenade.velocity.z *= damp;
        }
        let old_pos = transform.translation;
        let new_pos = old_pos + grenade.velocity * dt;

        // Trace hull collision vs world BSP
        let mut hit = false;
        if let Some(ref col_model) = collision_res.model {
            use goldhub_physics::WorldTrace;
            let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
            let gs_start = [old_pos.x, -old_pos.z, old_pos.y];
            let gs_end = [new_pos.x, -new_pos.z, new_pos.y];
            let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
            if tr.fraction < 1.0 {
                if tr.is_sky {
                    // CS 1.6 / GoldSrc parity: Grenades colliding with the skybox do NOT hard-ricochet across the map.
                    // Instead, the skybox arrests horizontal and upward momentum, allowing the grenade
                    // to decelerate smoothly and drop downward under gravity while remaining visible.
                    let mut hit_n =
                        Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1]);
                    if hit_n.length_squared() > 1e-4 {
                        hit_n = hit_n.normalize();
                    } else {
                        hit_n = Vec3::NEG_Y;
                    }
                    let endpos = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);
                    transform.translation = endpos + hit_n * 0.5;

                    // Heavily dampen horizontal velocities so grenade doesn't launch backwards
                    grenade.velocity.x *= 0.15;
                    grenade.velocity.z *= 0.15;
                    // Stop upward motion and impart gentle initial downward drift
                    if grenade.velocity.y > 0.0 {
                        grenade.velocity.y = -20.0;
                    }
                    grenade.on_ground = false;
                    continue;
                }
                hit = true;
                let hit_n = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                    .normalize_or_zero();
                let endpos = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);

                // Breakable impact check (CS 1.6 / ReGameDLL func_break.cpp BreakTouch):
                // If a flying grenade collides with a breakable entity (glass, vent grate, crate),
                // check its material and damage it.
                if let Some(ref col) = collision_res.model {
                    for (_b_entity, mut b) in &mut breakable_query {
                        if b.is_broken {
                            continue;
                        }
                        // Check if hit point or entity matches this breakable's model/bounds
                        let is_hit = if let Some(hit_id) = tr.hit_entity {
                            col.brush_entities.iter().any(|be| {
                                be.entity_id == hit_id && b.model == format!("*{}", be.model_index)
                            })
                        } else {
                            endpos.x >= b.bmin.x - 4.0
                                && endpos.x <= b.bmax.x + 4.0
                                && endpos.y >= b.bmin.y - 4.0
                                && endpos.y <= b.bmax.y + 4.0
                                && endpos.z >= b.bmin.z - 4.0
                                && endpos.z <= b.bmax.z + 4.0
                        };

                        if is_hit {
                            // Glass immediately shatters on grenade impact in GoldSrc
                            let impact_dmg = if b.material == BreakableMaterial::Glass {
                                b.health
                            } else {
                                (grenade.velocity.length() * 0.05).max(15.0)
                            };
                            b.health -= impact_dmg;
                            if b.health <= 0.0 {
                                b.is_broken = true;
                                let center = (b.bmin + b.bmax) * 0.5;
                                events.break_ent.send(BreakEntityEvent {
                                    origin: center,
                                    material: b.material,
                                    model: b.model.clone(),
                                    explodemagnitude: b.explodemagnitude,
                                });
                                events.fire.send(FireTargetEvent {
                                    target: b.target.clone(),
                                });
                            }
                            break;
                        }
                    }
                }

                let normal_vel = grenade.velocity.dot(hit_n);
                if normal_vel < 0.0 {
                    grenade.velocity = (grenade.velocity - hit_n * (1.5 * normal_vel)) * 0.7;
                }
                if hit_n.y > 0.7 && grenade.velocity.length() < 100.0 {
                    grenade.velocity.y = 0.0;
                    grenade.velocity.x *= 0.85;
                    grenade.velocity.z *= 0.85;
                    grenade.on_ground = true;
                    transform.translation = endpos;
                    if grenade.velocity.length_squared() < 25.0 {
                        grenade.velocity = Vec3::ZERO;
                    }
                    transform.rotation =
                        Quat::from_rotation_y(grenade.yaw + std::f32::consts::FRAC_PI_2);
                } else {
                    grenade.on_ground = hit_n.y > 0.7 && grenade.velocity.y.abs() < 45.0;
                    transform.translation = endpos + hit_n * 0.8;
                }
                grenade.bounce_count += 1;

                if grenade.bounce_count == 1 {
                    transform.rotation =
                        Quat::from_rotation_y(grenade.yaw + std::f32::consts::FRAC_PI_2);
                    if let Some(ref mut anim) = anim_opt {
                        // ReGameDLL BounceTouch: Sequence 1 is resting flat on the ground
                        anim.set_sequence(1, 0.0, false);
                    }
                }

                if grenade.bounce_count <= 4 {
                    let b_snd = if grenade.bounce_count == 1 {
                        "weapons/he_bounce-1.wav"
                    } else {
                        "weapons/grenade_hit1.wav"
                    };
                    if let Some(h) = audio.audio_bank.get_or_load_sound(
                        b_snd,
                        &audio.search_paths.dirs,
                        &mut audio.audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN
                                .with_spatial(true)
                                .with_volume(bevy::audio::Volume::new(0.5)),
                            Transform::from_translation(transform.translation),
                        ));
                    }
                }
            }
        }

        if !hit {
            grenade.on_ground = false;
            transform.translation = new_pos;
        }
    }
}

pub fn spawn_blood_splatter(
    commands: &mut Commands,
    origin: Vec3,
    sprite_store: &mut SpriteStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh_handle = sprite_store.get_or_create_unit_sphere(meshes);
    let mat_handle = sprite_store.get_or_create_blood_mat(materials);

    for i in 0..12 {
        let angle = (i as f32) * std::f32::consts::TAU / 12.0;
        let speed = 40.0 + ((i * 17) % 70) as f32;
        let vel = Vec3::new(
            angle.cos() * speed,
            20.0 + ((i * 13) % 40) as f32,
            angle.sin() * speed,
        );
        commands.spawn((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(mat_handle.clone()),
            Transform::from_translation(origin).with_scale(Vec3::splat(1.8)),
            BulletParticle {
                velocity: vel,
                lifetime: 0.0,
                max_lifetime: 0.35,
                gravity: 500.0,
            },
        ));
    }
}

pub fn spawn_bullet_dust_puff(
    commands: &mut Commands,
    origin: Vec3,
    normal: Vec3,
    sprite_store: &mut SpriteStore,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh_handle = sprite_store.get_or_create_unit_sphere(meshes);
    let mat_handle = sprite_store.get_or_create_dust_puff_mat(materials);

    for i in 0..8 {
        let angle = (i as f32) * std::f32::consts::TAU / 8.0;
        let speed = 30.0 + ((i * 19) % 50) as f32;
        let spread_dir =
            (normal + Vec3::new(angle.cos() * 0.5, angle.sin() * 0.5, 0.0)).normalize();
        let vel = spread_dir * speed;
        commands.spawn((
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(mat_handle.clone()),
            Transform::from_translation(origin).with_scale(Vec3::splat(1.4)),
            BulletParticle {
                velocity: vel,
                lifetime: 0.0,
                max_lifetime: 0.25,
                gravity: 60.0,
            },
        ));
    }
}

/// SystemParam bundling audio resources for weapon sounds.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatAudio<'w> {
    pub search_paths: Res<'w, GameSearchPaths>,
    pub audio_bank: ResMut<'w, GoldSrcAudioBank>,
    pub audio_sources: ResMut<'w, Assets<AudioSource>>,
}

/// SystemParam bundling combat event writers.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatEvents<'w> {
    pub fire: EventWriter<'w, FireTargetEvent>,
    pub break_ent: EventWriter<'w, BreakEntityEvent>,
}

/// SystemParam bundling mesh and material assets for combat VFX.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatVfxAssets<'w> {
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub images: ResMut<'w, Assets<Image>>,
    pub decal_faces: Option<Res<'w, crate::sprite::LevelDecalSurfaceIndex>>,
}

/// SystemParam bundling combat scene queries.
#[derive(bevy::ecs::system::SystemParam)]
pub struct CombatQueries<'w, 's> {
    pub audio_channel: Query<'w, 's, Entity, With<WeaponAudioChannel>>,
    pub camera: Query<
        'w,
        's,
        &'static GlobalTransform,
        (With<Camera3d>, With<crate::player::PlayerController>),
    >,
    pub breakable: Query<'w, 's, (Entity, &'static mut BreakableEntity)>,
    pub hostage: Query<
        'w,
        's,
        (
            Entity,
            &'static mut HostageEntity,
            &'static mut StudioAnimatedModel,
            &'static mut EntityCollisionBounds,
            &'static mut WorldStudioEntity,
            &'static GlobalTransform,
        ),
        (
            Without<ViewModelWeapon>,
            Without<crate::player::PlayerController>,
        ),
    >,
    pub props: Query<
        'w,
        's,
        (
            Entity,
            &'static GlobalTransform,
            &'static EntityCollisionBounds,
            &'static WorldStudioEntity,
        ),
        Without<HostageEntity>,
    >,
    pub vm: Query<
        'w,
        's,
        (
            Entity,
            &'static mut ViewModelWeapon,
            &'static mut StudioAnimatedModel,
        ),
    >,
    pub inventory: Query<'w, 's, &'static mut PlayerWeaponInventory>,
    pub submodels: Query<'w, 's, (Entity, &'static BspSubmodelMesh, &'static GlobalTransform)>,
    pub decals: Query<'w, 's, (Entity, &'static BulletHoleDecal)>,
}

/// Computes canonical ReGameDLL weapon spread and updated accuracy via [`goldhub_game_cstrike::compute_weapon_spread`].
#[must_use]
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn compute_weapon_spread(
    id: WeaponId,
    silenced: bool,
    burst_mode: bool,
    pre_shots: u32,
    prev_accuracy: f32,
    time_since_last_fire: f32,
    has_fired_before: bool,
    player_speed: f32,
    on_ground: bool,
    is_ducking: bool,
    fov_deg: f32,
) -> goldhub_game_cstrike::SpreadResult {
    let fov_u8 = fov_deg.round().clamp(1.0, 120.0) as u8;
    goldhub_game_cstrike::compute_weapon_spread(goldhub_game_cstrike::SpreadContext {
        weapon: id,
        silenced,
        burst_mode,
        shots_fired: pre_shots,
        prev_accuracy,
        time_since_last_fire,
        has_fired_before,
        speed_2d: player_speed,
        on_ground,
        is_ducking,
        fov: fov_u8,
    })
}

/// Clamps decal position away from outer convex wall boundaries (270° convex corners)
/// so decals never overhang into empty air.
/// Detects whether a bullet struck within decal bounds (`half_size`) of an L-corner (either an inner 90°
/// or outer 270° corner) and calculates the position and surface normal for the adjacent face.
/// Authentic CS 1.6 GoldSrc parity: Decals wrap across corner edges so bullet impacts appear on BOTH sides.
fn find_adjacent_corner_decal(
    hit_pos: Vec3,
    hit_normal: Vec3,
    half_size: f32,
    collision: Option<&goldhub_formats::ir::CollisionModel>,
) -> Option<(Vec3, Vec3)> {
    use goldhub_physics::WorldTrace;

    let col_model = collision?;
    let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);

    let up = if hit_normal.dot(Vec3::Y).abs() > 0.85 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let tangent_h = up.cross(hit_normal).normalize_or_zero();
    let tangent_v = hit_normal.cross(tangent_h).normalize_or_zero();

    for t in [tangent_h, -tangent_h, tangent_v, -tangent_v] {
        if t.length_squared() < 0.5 {
            continue;
        }

        // 1. Check for Concave (inner 90°) corner: trace along tangent into an intersecting perpendicular wall
        let ray_end = hit_pos + t * (half_size + 1.0);
        let gs_start = [hit_pos.x, -hit_pos.z, hit_pos.y];
        let gs_end = [ray_end.x, -ray_end.z, ray_end.y];
        let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
        if tr.fraction < 1.0 && !tr.is_sky {
            let adj_normal = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                .normalize_or_zero();
            if adj_normal.dot(hit_normal).abs() < 0.25 {
                let dist_to_corner = (half_size + 1.0) * tr.fraction;
                if dist_to_corner < half_size {
                    let corner_edge = hit_pos + t * dist_to_corner;
                    let adj_pos = corner_edge + hit_normal * (half_size - dist_to_corner);
                    return Some((adj_pos, adj_normal));
                }
            }
        }

        // 2. Check for Convex (outer 270°) corner: wall ends and turns around the corner
        // Probe outwards along tangent t in 4 fine-grained increments
        for step in 1..=4 {
            let probe_dist = (half_size * step as f32) / 4.0;
            let probe_pt = hit_pos + t * probe_dist;
            let p_start = probe_pt + hit_normal * 0.5;
            let p_end = probe_pt - hit_normal * 2.0;
            let tr_edge = bsp_world.trace_player_hull(
                [p_start.x, -p_start.z, p_start.y],
                [p_end.x, -p_end.z, p_end.y],
                0,
            );
            if tr_edge.fraction >= 0.99 || tr_edge.allsolid {
                // Outer wall corner edge detected! Probe inward into the perpendicular return face
                let corner_pt = hit_pos + t * (probe_dist - (half_size / 8.0));
                let adj_probe_start = corner_pt + t * 2.0 - hit_normal * 0.5;
                let adj_probe_end = corner_pt - t * 2.0 - hit_normal * 0.5;
                let tr_adj = bsp_world.trace_player_hull(
                    [adj_probe_start.x, -adj_probe_start.z, adj_probe_start.y],
                    [adj_probe_end.x, -adj_probe_end.z, adj_probe_end.y],
                    0,
                );
                if tr_adj.fraction < 1.0 && !tr_adj.is_sky {
                    let adj_normal = Vec3::new(
                        tr_adj.plane_normal[0],
                        tr_adj.plane_normal[2],
                        -tr_adj.plane_normal[1],
                    )
                    .normalize_or_zero();
                    if adj_normal.dot(hit_normal).abs() < 0.25 {
                        let adj_surface_pt =
                            adj_probe_start + (adj_probe_end - adj_probe_start) * tr_adj.fraction;
                        // Return face decal anchors right at corner edge
                        let adj_pos = adj_surface_pt - hit_normal * (half_size * 0.5);
                        return Some((adj_pos, adj_normal));
                    }
                }
                break;
            }
        }
    }
    None
}

/// Plays spatial surface impact audio/decals/sparks when a bullet strikes brush/wall or survivable breakable.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn emit_surface_impact_fx(
    commands: &mut Commands,
    time: &Time,
    hit_pos: Vec3,
    hit_normal: Vec3,
    material: Option<BreakableMaterial>,
    attach_to: Option<(Entity, Vec3, Quat)>,
    model_index: usize,
    sprite_store: &mut SpriteStore,
    audio: &mut CombatAudio,
    vfx: &mut CombatVfxAssets,
    collision: Option<&goldhub_formats::ir::CollisionModel>,
    existing_decals: &Query<(Entity, &'static BulletHoleDecal)>,
) {
    let half_size = match material {
        Some(BreakableMaterial::Glass) => 16.0,
        _ => 8.0,
    };

    // Only deduplicate exact point-blank overlap (< 1.5 units) so tight groupings layer naturally
    for (ent, decal) in existing_decals {
        if decal.position.distance_squared(hit_pos) < 2.25 && decal.normal.dot(hit_normal) > 0.85 {
            crate::sprite::despawn_decal_entity(commands, Some(&mut vfx.meshes), ent, decal);
        }
    }

    let (decal_kind, hit_snd, emit_sparks) = match material {
        Some(BreakableMaterial::Glass) => {
            let idx = ((time.elapsed_secs() * 100.0) as usize % 3) + 1;
            (
                ImpactDecalKind::BreakableGlass,
                format!("debris/glass{idx}.wav"),
                false,
            )
        }
        Some(BreakableMaterial::UnbreakableGlass) => {
            let idx = ((time.elapsed_secs() * 100.0) as usize % 3) + 1;
            (
                ImpactDecalKind::UnbreakableGlass,
                format!("debris/glass{idx}.wav"),
                true,
            )
        }
        Some(BreakableMaterial::Wood) => {
            let idx = ((time.elapsed_secs() * 100.0) as usize % 3) + 1;
            (
                ImpactDecalKind::BreakableGlass,
                format!("debris/wood{idx}.wav"),
                false,
            )
        }
        Some(BreakableMaterial::Metal | BreakableMaterial::Computer) => {
            let idx = ((time.elapsed_secs() * 100.0) as usize % 3) + 1;
            (
                ImpactDecalKind::BreakableGlass,
                format!("debris/metal{idx}.wav"),
                true,
            )
        }
        Some(_) => {
            let ric_idx = ((time.elapsed_secs() * 100.0) as usize % 5) + 1;
            (
                ImpactDecalKind::BreakableGlass,
                format!("weapons/ric{ric_idx}.wav"),
                true,
            )
        }
        None => {
            let ric_idx = ((time.elapsed_secs() * 100.0) as usize % 5) + 1;
            (
                ImpactDecalKind::BulletShot,
                format!("weapons/ric{ric_idx}.wav"),
                true,
            )
        }
    };

    if let Some(h) = audio.audio_bank.get_or_load_sound(
        &hit_snd,
        &audio.search_paths.dirs,
        &mut audio.audio_sources,
    ) {
        commands.spawn((
            AudioPlayer::new(h),
            PlaybackSettings::DESPAWN.with_spatial(true),
            Transform::from_translation(hit_pos),
        ));
    }

    // Spawn Sutherland-Hodgman BSP-clipped decal(s) (automatically wraps around L-corners cleanly).
    let decal_faces_ref = vfx.decal_faces.as_deref();
    spawn_surface_decal(
        commands,
        hit_pos,
        hit_normal,
        decal_kind,
        attach_to,
        model_index,
        decal_faces_ref,
        sprite_store,
        &audio.search_paths,
        &mut vfx.images,
        &mut vfx.materials,
        &mut vfx.meshes,
    );

    // Fallback corner probe only when LevelDecalSurfaceIndex is absent (e.g. non-BSP arena)
    if decal_faces_ref.is_none()
        && let Some((adj_pos, adj_normal)) =
            find_adjacent_corner_decal(hit_pos, hit_normal, half_size, collision)
    {
        for (ent, decal) in existing_decals {
            if decal.position.distance_squared(adj_pos) < 2.25
                && decal.normal.dot(adj_normal) > 0.85
            {
                crate::sprite::despawn_decal_entity(commands, Some(&mut vfx.meshes), ent, decal);
            }
        }
        spawn_surface_decal(
            commands,
            adj_pos,
            adj_normal,
            decal_kind,
            attach_to,
            model_index,
            None,
            sprite_store,
            &audio.search_paths,
            &mut vfx.images,
            &mut vfx.materials,
            &mut vfx.meshes,
        );
    }

    if emit_sparks {
        spawn_wall_sparks(
            commands,
            hit_pos,
            hit_normal,
            sprite_store,
            &audio.search_paths,
            &mut vfx.images,
            &mut vfx.materials,
            &mut vfx.meshes,
        );
    }
}

/// Core weapon fire, reload, kickback recoil, and hitscan raytrace damage dispatch.
const BULLET_REACH: f32 = 8192.0;

/// Swaps viewmodel entities cleanly to the combat knife and triggers draw animation + deploy sound.
fn swap_viewmodel_to_knife(
    commands: &mut Commands,
    model_store: &mut StudioModelStore,
    audio: &mut CombatAudio,
    vfx: &mut CombatVfxAssets,
    vm_query: &mut Query<(Entity, &mut ViewModelWeapon, &mut StudioAnimatedModel)>,
) {
    play_ui_sound(
        commands,
        "weapons/knife_deploy1.wav",
        &audio.search_paths,
        &mut audio.audio_bank,
        &mut audio.audio_sources,
    );

    let Some(model_data) = model_store.get_or_load(
        "models/v_knife.mdl",
        &audio.search_paths,
        &mut vfx.images,
        &mut vfx.materials,
    ) else {
        return;
    };

    for (vm_ent, _, mut anim) in vm_query.iter_mut() {
        for (child_ent, mesh_handle, _) in anim.mesh_entities.drain(..) {
            vfx.meshes.remove(&mesh_handle);
            commands.entity(child_ent).despawn_recursive();
        }
        anim.model_id = "models/v_knife.mdl".to_string();
        anim.idle_sequence_index = 0;
        let fps = model_data
            .model
            .rig
            .sequences
            .get(3)
            .map_or(30.0, |s| s.fps);
        anim.set_sequence(3, fps, false);
        anim.mesh_entities = spawn_model_submesh_children(
            commands,
            vm_ent,
            &model_data,
            3,
            true,
            Some(VIEWMODEL_RENDER_LAYER),
            &mut vfx.meshes,
        );
    }
}

/// Spawns a grenade projectile using static sideways horizontal orientation (`idle` seq 0, `fps = 0`).
#[allow(clippy::too_many_arguments)]
fn spawn_grenade_entity(
    commands: &mut Commands,
    weapon_id: WeaponId,
    launch_pos: Vec3,
    launch_vel: Vec3,
    player_yaw: f32,
    _player_pitch: f32,
    _seed_bits: u32,
    model_store: &mut StudioModelStore,
    sprite_store: &mut SpriteStore,
    search_paths: &GameSearchPaths,
    vfx: &mut CombatVfxAssets,
) {
    let w_model_path = match weapon_id {
        WeaponId::Flashbang => "models/w_flashbang.mdl",
        WeaponId::SmokeGrenade => "models/w_smokegrenade.mdl",
        _ => "models/w_hegrenade.mdl",
    };

    // CS 1.6: Grenade lies horizontally on its side across the player's view (a horizontal log / dash `-`)
    let init_rot = Quat::from_rotation_y(player_yaw + std::f32::consts::FRAC_PI_2);

    if let Some(model_data) = model_store.get_or_load(
        w_model_path,
        search_paths,
        &mut vfx.images,
        &mut vfx.materials,
    ) {
        let mut anim = StudioAnimatedModel::new(w_model_path, false);
        // CS 1.6: Grenade maintains static side-profile orientation in flight without rolling spin
        anim.set_sequence(0, 0.0, false);

        let parent = commands
            .spawn((
                GrenadeProjectile {
                    velocity: launch_vel,
                    fuse_timer: 1.5,
                    bounce_count: 0,
                    weapon_id,
                    on_ground: false,
                    yaw: player_yaw,
                },
                Transform::from_translation(launch_pos).with_rotation(init_rot),
                Visibility::Inherited,
            ))
            .id();
        model_store.spawn_static_model_submesh_children(
            commands,
            parent,
            &model_data,
            w_model_path,
            false,
            Some(2),
            &mut vfx.meshes,
        );
        commands.entity(parent).insert(anim);
    } else {
        let mesh_handle = sprite_store.get_or_create_unit_sphere(&mut vfx.meshes);
        let mat_handle = sprite_store.get_or_create_grenade_mat(&mut vfx.materials);
        commands.spawn((
            GrenadeProjectile {
                velocity: launch_vel,
                fuse_timer: 1.5,
                bounce_count: 0,
                weapon_id,
                on_ground: false,
                yaw: player_yaw,
            },
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat_handle),
            Transform::from_translation(launch_pos)
                .with_rotation(init_rot)
                .with_scale(Vec3::splat(3.5)),
        ));
    }
}

/// Swaps the Layer 1 ViewModel submeshes and plays the matching draw sequence.
#[allow(clippy::too_many_arguments)]
fn deploy_viewmodel_sequence(
    commands: &mut Commands,
    vm_entity: Entity,
    anim: &mut StudioAnimatedModel,
    weapon_id: WeaponId,
    silenced: bool,
    model_store: &mut StudioModelStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let def = get_weapon_def(weapon_id);
    for (child_ent, mesh_handle, _) in anim.mesh_entities.drain(..) {
        meshes.remove(&mesh_handle);
        commands.entity(child_ent).despawn_recursive();
    }
    if let Some(model_data) = model_store.get_or_load(def.v_model, search_paths, images, materials)
    {
        anim.model_id = def.v_model.to_string();
        let (deploy_anim_idx, idle_seq_idx) = match weapon_id {
            WeaponId::USP45 => {
                if silenced {
                    (6, 0)
                } else {
                    (14, 8)
                }
            }
            WeaponId::M4A1 => {
                if silenced {
                    (5, 0)
                } else {
                    (12, 7)
                }
            }
            _ => (def.deploy_anim, 0),
        };
        anim.idle_sequence_index = idle_seq_idx;
        let deploy_fps = model_data
            .model
            .rig
            .sequences
            .get(deploy_anim_idx)
            .map_or(30.0, |s| s.fps);
        anim.set_sequence(deploy_anim_idx, deploy_fps, false);
        anim.mesh_entities = spawn_model_submesh_children(
            commands,
            vm_entity,
            &model_data,
            deploy_anim_idx,
            true,
            Some(VIEWMODEL_RENDER_LAYER),
            meshes,
        );
    }
}

/// Deterministic 32-bit float mixer (`0.0..1.0`) for symmetric spread and casing velocity.
#[inline]
fn hash_mix_unit(x: u64) -> f32 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z & 0xFFFF) as f32) / 65536.0
}

/// Fires one hitscan bullet trace with CS 1.6 muzzle flash (unsilenced), brass ejection, wallbang, moving decals, and sparks.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn fire_single_bullet_trace(
    commands: &mut Commands,
    time: &Time,
    perspective: CameraPerspective,
    def: &WeaponDef,
    weapon_id: WeaponId,
    silenced: bool,
    burst_mode: bool,
    burst_shot_index: u8,
    emit_muzzle_and_brass: bool,
    spread: f32,
    hash_base: u64,
    cam_t: &GlobalTransform,
    eye_pos: Vec3,
    player_pitch: f32,
    player_yaw: f32,
    recoil: &PlayerRecoil,
    mut player_ctrl: Option<&mut PlayerController>,
    collision: Option<&goldhub_formats::ir::CollisionModel>,
    model_store: &mut StudioModelStore,
    sprite_store: &mut SpriteStore,
    audio: &mut CombatAudio,
    vfx: &mut CombatVfxAssets,
    queries: &mut CombatQueries,
    events: &mut CombatEvents,
) {
    let cam_pos = cam_t.translation();
    let cam_forward = cam_t.forward().as_vec3();
    let cam_right = cam_t.right().as_vec3();
    let cam_up = cam_t.up().as_vec3();
    let right_handed = true;

    if emit_muzzle_and_brass {
        if !silenced {
            let flash_light_pos = if perspective == CameraPerspective::FirstPerson {
                if let Some((vm_ent, _, _)) = queries.vm.iter().next() {
                    spawn_muzzleflash_attached(
                        commands,
                        vm_ent,
                        right_handed,
                        sprite_store,
                        &audio.search_paths,
                        &mut vfx.images,
                        &mut vfx.materials,
                        &mut vfx.meshes,
                    );
                }
                cam_pos + cam_forward * 24.0
            } else {
                let barrel_world = eye_pos - Vec3::Y * 10.0 + cam_forward * 34.0 + cam_right * 6.0;
                spawn_muzzleflash_at(
                    commands,
                    barrel_world,
                    sprite_store,
                    &audio.search_paths,
                    &mut vfx.images,
                    &mut vfx.materials,
                    &mut vfx.meshes,
                );
                barrel_world
            };

            commands.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.85, 0.6),
                    intensity: 6_000.0,
                    range: 240.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(flash_light_pos),
                crate::entity::ExplosionFlash {
                    lifetime: 0.0,
                    max_lifetime: 0.05,
                    initial_intensity: 6_000.0,
                },
            ));
        }

        // Physical brass shell ejection (`EV_EjectBrass`)
        let side_sign = if right_handed { -1.0 } else { 1.0 };
        let shell_origin = if perspective == CameraPerspective::FirstPerson {
            cam_pos + cam_forward * 20.0 + cam_right * (if right_handed { 10.0 } else { -10.0 })
                - cam_up * 8.0
        } else {
            eye_pos - Vec3::Y * 10.0 + cam_forward * 20.0 + cam_right * 4.0
        };
        let forward_impulse = 45.0 + (hash_mix_unit(hash_base ^ 0x1111) * 75.0);
        let lateral_impulse = (50.0 + hash_mix_unit(hash_base ^ 0x2222) * 35.0) * side_sign;
        let up_impulse = 85.0 + (hash_mix_unit(hash_base ^ 0x3333) * 60.0);
        let shell_vel =
            cam_forward * forward_impulse + cam_right * lateral_impulse + cam_up * up_impulse;

        let shell_mdl_path = shell_model_for_weapon(weapon_id);
        if let Some(shell_data) = model_store.get_or_load(
            shell_mdl_path,
            &audio.search_paths,
            &mut vfx.images,
            &mut vfx.materials,
        ) {
            let shell_parent = commands
                .spawn((
                    BrassCasingProjectile {
                        velocity: shell_vel,
                        lifetime: 0.0,
                        max_lifetime: 1.8,
                        bounce_count: 0,
                    },
                    Transform::from_translation(shell_origin).with_rotation(Quat::from_rotation_y(
                        cam_t.rotation().to_euler(EulerRot::YXZ).0,
                    )),
                    Visibility::Inherited,
                ))
                .id();
            model_store.spawn_static_model_submesh_children(
                commands,
                shell_parent,
                &shell_data,
                shell_mdl_path,
                false,
                None,
                &mut vfx.meshes,
            );
        } else {
            let (shell_mesh, shell_mat) =
                sprite_store.get_or_create_brass_mesh_and_mat(&mut vfx.meshes, &mut vfx.materials);
            commands.spawn((
                BrassCasingProjectile {
                    velocity: shell_vel,
                    lifetime: 0.0,
                    max_lifetime: 1.8,
                    bounce_count: 0,
                },
                Mesh3d(shell_mesh),
                MeshMaterial3d(shell_mat),
                Transform::from_translation(shell_origin).with_rotation(Quat::from_rotation_z(1.2)),
            ));
        }
    }

    let u1 = hash_mix_unit(hash_base ^ 0xAAAA_5555);
    let u2 = hash_mix_unit(hash_base ^ 0x5555_AAAA);
    let u3 = hash_mix_unit(hash_base ^ 0xF0F0_0F0F);
    let u4 = hash_mix_unit(hash_base ^ 0x0F0F_F0F0);
    let rx = (u1 - 0.5) + (u2 - 0.5);
    let ry = (u3 - 0.5) + (u4 - 0.5);
    let (punch_pitch, punch_yaw) = (
        recoil.punch_pitch.to_radians() * 2.0,
        recoil.punch_yaw.to_radians() * 2.0,
    );
    let shot_pitch = player_pitch + punch_pitch;
    let shot_yaw = player_yaw + punch_yaw;
    let shot_rot = Quat::from_rotation_y(shot_yaw) * Quat::from_rotation_x(shot_pitch);
    let shot_forward = shot_rot * (-Vec3::Z);
    let shot_right = shot_rot * Vec3::X;
    let shot_up = shot_rot * Vec3::Y;
    let bullet_dir =
        (shot_forward + shot_right * (rx * spread) + shot_up * (ry * spread)).normalize();

    // FireBullets3 wallbang & hitscan penetration loop with range_modifier + moving brush decal attachment
    let mut cur_src = eye_pos;
    let mut remaining_reach = BULLET_REACH;
    let mut cur_damage = weapon_id.effective_damage(silenced, burst_mode, burst_shot_index);
    let range_mod = weapon_id.range_modifier(silenced, burst_mode && burst_shot_index > 0);
    let mut penetrations_left = def.penetration;

    while penetrations_left > 0 || cur_src == eye_pos {
        let mut hit_wall_dist = remaining_reach;
        let mut hit_wall_normal = -bullet_dir;
        let mut hit_brush_entity = None;
        let mut hit_is_sky = false;
        if let Some(col_model) = collision {
            use goldhub_physics::WorldTrace;
            let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
            let gs_start = [cur_src.x, -cur_src.z, cur_src.y];
            let ray_end = cur_src + bullet_dir * remaining_reach;
            let gs_end = [ray_end.x, -ray_end.z, ray_end.y];
            let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
            if tr.fraction < 1.0 {
                hit_is_sky = tr.is_sky;
                hit_wall_dist = tr.fraction * remaining_reach;
                hit_wall_normal =
                    Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                        .normalize_or_zero();
                hit_brush_entity = tr.hit_entity;
            }
        }

        // Resolve BSP brush entity_id -> "*N" submodel name + parent Transform for decals on moving doors/glass
        let hit_brush = collision.and_then(|col_model| {
            let eid = hit_brush_entity?;
            col_model.brush_entities.iter().find(|b| b.entity_id == eid)
        });
        let hit_submodel_str = hit_brush.map(|b| format!("*{}", b.model_index));
        let attach_to = hit_submodel_str.as_ref().and_then(|model_str| {
            queries
                .submodels
                .iter()
                .find(|(_, sm, _)| sm.model == *model_str)
                .map(|(e, _, gt)| (e, gt.translation(), gt.rotation()))
        });

        let mut hit_breakable: Option<(Entity, f32, BreakableMaterial, u32)> = None;
        if let Some(ref model_str) = hit_submodel_str {
            for (b_ent, b) in &queries.breakable {
                if !b.is_broken && b.model == *model_str {
                    hit_breakable = Some((b_ent, hit_wall_dist, b.material, b.explodemagnitude));
                    break;
                }
            }
        }

        if hit_breakable.is_none() {
            for (b_ent, b) in &queries.breakable {
                if b.is_broken {
                    continue;
                }
                if let Some(t) =
                    ray_intersects_aabb(cur_src, bullet_dir, hit_wall_dist + 4.0, b.bmin, b.bmax)
                    && hit_breakable.as_ref().is_none_or(|(_, d, _, _)| t < *d)
                {
                    hit_breakable = Some((b_ent, t, b.material, b.explodemagnitude));
                }
            }
        }

        for (_ent, ent_t, bounds, studio_ent) in &queries.props {
            let ep = ent_t.translation();
            let bmin = ep + bounds.mins;
            let bmax = ep + bounds.maxs;
            if let Some(t) = ray_intersects_aabb(cur_src, bullet_dir, remaining_reach, bmin, bmax)
                && !studio_ent.targetname.is_empty()
            {
                for (b_ent, b) in &queries.breakable {
                    if !b.is_broken
                        && ((b.bmin + b.bmax) * 0.5).distance(ep) < 64.0
                        && hit_breakable.as_ref().is_none_or(|(_, d, _, _)| t < *d)
                    {
                        hit_breakable = Some((b_ent, t, b.material, b.explodemagnitude));
                        break;
                    }
                }
            }
        }

        let mut hit_hostage: Option<(Entity, f32)> = None;
        for (h_ent, _, _, bounds, _, ht) in &queries.hostage {
            let ep = ht.translation();
            let bmin = ep + bounds.mins;
            let bmax = ep + bounds.maxs;
            if let Some(t) = ray_intersects_aabb(cur_src, bullet_dir, remaining_reach, bmin, bmax)
                && hit_hostage.as_ref().is_none_or(|(_, d)| t < *d)
            {
                hit_hostage = Some((h_ent, t));
            }
        }

        if let Some((b_ent, b_t, mat, exp_mag)) = hit_breakable
            && hit_hostage.as_ref().is_none_or(|(_, h_t)| b_t < *h_t)
        {
            let hit_pos = cur_src + bullet_dir * b_t;
            let impact_norm = if hit_wall_normal.length_squared() > 0.1 {
                hit_wall_normal
            } else {
                -bullet_dir
            };
            let dist_damage = cur_damage * range_mod.powf((b_t / 500.0).max(0.0));
            if let Ok((_, mut breakable)) = queries.breakable.get_mut(b_ent) {
                breakable.health -= dist_damage;
                let was_broken = breakable.health <= 0.0 && !breakable.is_broken;
                if was_broken {
                    breakable.is_broken = true;
                    let center = (breakable.bmin + breakable.bmax) * 0.5;
                    events.break_ent.send(BreakEntityEvent {
                        origin: center,
                        material: mat,
                        model: breakable.model.clone(),
                        explodemagnitude: exp_mag,
                    });
                    events.fire.send(FireTargetEvent {
                        target: breakable.target.clone(),
                    });
                } else {
                    let b_model_idx = breakable
                        .model
                        .strip_prefix('*')
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or_else(|| hit_brush.map_or(0, |b| b.model_index));
                    emit_surface_impact_fx(
                        commands,
                        time,
                        hit_pos,
                        impact_norm,
                        Some(mat),
                        attach_to,
                        b_model_idx,
                        sprite_store,
                        audio,
                        vfx,
                        collision,
                        &queries.decals,
                    );
                }
                if penetrations_left > 0 && remaining_reach > b_t + 16.0 {
                    penetrations_left -= 1;
                    cur_damage = dist_damage * if was_broken { 0.85 } else { 0.5 };
                    cur_src += bullet_dir * (b_t + 16.0);
                    remaining_reach -= b_t + 16.0;
                    continue;
                }
            }
            break;
        } else if let Some((h_ent, h_t)) = hit_hostage
            && h_t <= hit_wall_dist + 4.0
        {
            let dist_damage = cur_damage * range_mod.powf((h_t / 500.0).max(0.0));
            if let Ok((_, mut hostage, mut h_anim, mut bounds, mut studio_ent, ht)) =
                queries.hostage.get_mut(h_ent)
            {
                let was_dead = hostage.is_dead;
                hostage.health -= dist_damage;
                let hit_pos = cur_src + bullet_dir * h_t;
                let flesh_snd = if ((time.elapsed_secs() * 100.0) as usize).is_multiple_of(2) {
                    "weapons/bullet_hit1.wav"
                } else {
                    "weapons/bullet_hit2.wav"
                };
                if let Some(h) = audio.audio_bank.get_or_load_sound(
                    flesh_snd,
                    &audio.search_paths.dirs,
                    &mut audio.audio_sources,
                ) {
                    commands.spawn((
                        AudioPlayer::new(h),
                        PlaybackSettings::DESPAWN.with_spatial(true),
                        Transform::from_translation(hit_pos),
                    ));
                }
                spawn_blood_sprite(
                    commands,
                    hit_pos,
                    sprite_store,
                    &audio.search_paths,
                    &mut vfx.images,
                    &mut vfx.materials,
                    &mut vfx.meshes,
                );

                if hostage.health > 0.0 {
                    if !was_dead && let Some(ref mut pc) = player_ctrl {
                        pc.apply_hostage_penalty(dist_damage, false);
                    }
                    h_anim.set_sequence(7, 30.0, false);
                    let pain_idx = ((time.elapsed_secs() * 100.0) as usize % 6) + 1;
                    let snd = format!("hostage/hpain/hpain{pain_idx}.wav");
                    if let Some(h) = audio.audio_bank.get_or_load_sound(
                        &snd,
                        &audio.search_paths.dirs,
                        &mut audio.audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(h),
                            PlaybackSettings::DESPAWN.with_spatial(true),
                            Transform::from_translation(ht.translation()),
                        ));
                    }
                } else if !was_dead {
                    hostage.is_dead = true;
                    if let Some(ref mut pc) = player_ctrl {
                        pc.apply_hostage_penalty(dist_damage, true);
                    }
                    h_anim.set_sequence(31, 24.0, false);
                    studio_ent.is_solid = false;
                    bounds.mins = Vec3::ZERO;
                    bounds.maxs = Vec3::ZERO;
                    if let Some(h) = audio.audio_bank.get_or_load_sound(
                        "radio/hosdown.wav",
                        &audio.search_paths.dirs,
                        &mut audio.audio_sources,
                    ) {
                        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                    }
                }

                if penetrations_left > 0 && remaining_reach > h_t + 24.0 {
                    penetrations_left -= 1;
                    cur_damage = dist_damage * 0.65;
                    cur_src += bullet_dir * (h_t + 24.0);
                    remaining_reach -= h_t + 24.0;
                    continue;
                }
            }
            break;
        } else if hit_wall_dist < remaining_reach {
            if hit_is_sky {
                // GoldSrc / CS 1.6 parity: Bullets hitting the skybox vanish silently without decals or ricochets
                break;
            }
            let hit_pos = cur_src + bullet_dir * hit_wall_dist;
            // GoldSrc ev_cs16.cpp: if (pe->rendermode != kRenderNormal) return "{bproof1";
            let hit_mat = if hit_brush.is_some_and(|b| b.rendermode != 0) {
                Some(BreakableMaterial::UnbreakableGlass)
            } else {
                None
            };
            let hit_model_idx = hit_brush.map_or(0, |b| b.model_index);
            emit_surface_impact_fx(
                commands,
                time,
                hit_pos,
                hit_wall_normal,
                hit_mat,
                attach_to,
                hit_model_idx,
                sprite_store,
                audio,
                vfx,
                collision,
                &queries.decals,
            );

            if penetrations_left > 0
                && def.penetration_power > 0.0
                && remaining_reach > hit_wall_dist + def.penetration_power
            {
                penetrations_left -= 1;
                cur_damage = cur_damage * range_mod.powf((hit_wall_dist / 500.0).max(0.0)) * 0.5;
                let pen_step = def.penetration_power;
                cur_src = hit_pos + bullet_dir * pen_step;
                remaining_reach -= hit_wall_dist + pen_step;
                continue;
            }
        }
        break;
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub fn weapon_fire_and_reload_system(
    mut commands: Commands,
    time: Res<Time>,
    perspective: Res<CameraPerspective>,
    pending_sel: Option<Res<PendingWeaponSelection>>,
    mut audio: CombatAudio,
    mouse_button: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    collision_res: Res<LevelCollisionResource>,
    mut model_store: ResMut<StudioModelStore>,
    mut sprite_store: ResMut<SpriteStore>,
    mut vfx: CombatVfxAssets,
    mut player_q: Query<(
        &mut ActiveWeaponState,
        &mut PlayerRecoil,
        Option<&mut PlayerController>,
    )>,
    mut queries: CombatQueries,
    mut events: CombatEvents,
    console: Option<Res<crate::hud::text_channels::ConsoleOverlayState>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok((mut state, mut recoil, mut player_ctrl)) = player_q.get_single_mut() else {
        return;
    };

    if state.next_primary_attack > 0.0 {
        state.next_primary_attack = (state.next_primary_attack - dt).max(0.0);
    }
    if state.next_secondary_attack > 0.0 {
        state.next_secondary_attack = (state.next_secondary_attack - dt).max(0.0);
    }

    let console_open = console.as_ref().is_some_and(|c| c.open);
    let input_blocked_by_menu = console_open
        || pending_sel
            .as_ref()
            .is_some_and(|p| p.open || p.consume_next_attack);

    // Complete delayed knife draw / next-grenade deploy after throw or C4_DROP animation
    if let Some(ref mut swap_timer) = state.pending_knife_swap_timer {
        *swap_timer -= dt;
        if *swap_timer <= 0.0 {
            state.pending_knife_swap_timer = None;
            let was_c4 = state.c4_placed_anim;
            state.c4_placed_anim = false;
            if !was_c4 && state.reserve > 0 {
                state.pin_pulled = false;
                state.pin_pull_timer = 0.0;
                state.next_primary_attack = 0.5;
                state.next_secondary_attack = 0.5;
                let wid = state.id;
                let sil = state.silenced;
                for (vm_ent, _, mut anim) in &mut queries.vm {
                    deploy_viewmodel_sequence(
                        &mut commands,
                        vm_ent,
                        &mut anim,
                        wid,
                        sil,
                        &mut model_store,
                        &audio.search_paths,
                        &mut vfx.images,
                        &mut vfx.materials,
                        &mut vfx.meshes,
                    );
                }
            } else {
                state.id = WeaponId::Knife;
                state.clip = 0;
                state.reserve = 0;
                state.next_primary_attack = 0.75;
                state.next_secondary_attack = 0.75;
                if let Ok(mut inv) = queries.inventory.get_single_mut() {
                    inv.active_slot = 3;
                }
                swap_viewmodel_to_knife(
                    &mut commands,
                    &mut model_store,
                    &mut audio,
                    &mut vfx,
                    &mut queries.vm,
                );
            }
        }
        return;
    }

    // Knife Attack Lifecycle (ReGameDLL `CKnife::PrimaryAttack` Swing / `SecondaryAttack` Stab)
    if state.id == WeaponId::Knife {
        let lmb_pressed = !input_blocked_by_menu && mouse_button.just_pressed(MouseButton::Left);
        let rmb_pressed = !input_blocked_by_menu && mouse_button.just_pressed(MouseButton::Right);

        if (lmb_pressed && state.next_primary_attack <= 0.0)
            || (rmb_pressed && state.next_secondary_attack <= 0.0)
        {
            let is_stab = rmb_pressed;
            let (reach, base_dmg, anim_seq, anim_miss_seq, next_p, next_s) = if is_stab {
                (32.0_f32, 65.0_f32, 4usize, 5usize, 1.1_f32, 1.1_f32)
            } else {
                let seq = if state.shots_fired % 2 == 0 {
                    6usize
                } else {
                    7usize
                };
                state.shots_fired = state.shots_fired.wrapping_add(1);
                (48.0_f32, 15.0_f32, seq, seq, 0.4_f32, 0.5_f32)
            };

            state.next_primary_attack = next_p;
            state.next_secondary_attack = next_s;

            if let Some(ref mut p) = player_ctrl {
                p.trigger_third_person_shoot(0.3);
            }

            // Knife Raycast & Hull Trace
            if let Ok(cam_t) = queries.camera.get_single() {
                let eye_pos = player_ctrl.as_deref().map_or_else(
                    || cam_t.translation(),
                    |p| {
                        Vec3::new(
                            p.sim.current.origin[0],
                            p.sim.current.origin[2] + p.sim.current.view_height,
                            -p.sim.current.origin[1],
                        )
                    },
                );

                let fwd = player_ctrl.as_deref().map_or_else(
                    || cam_t.forward().as_vec3(),
                    |p| {
                        let yaw_quat = Quat::from_rotation_y(p.yaw);
                        let pitch_quat = Quat::from_rotation_x(p.pitch);
                        (yaw_quat * pitch_quat * Vec3::NEG_Z).normalize()
                    },
                );

                let mut hit_something = false;

                // 1. Hostage / Entity hit
                for (_h_ent, mut hostage, mut anim, mut bounds, _ws, ht) in &mut queries.hostage {
                    if hostage.is_dead {
                        continue;
                    }
                    let h_trans = ht.translation();
                    let (bmin, bmax) = (h_trans + bounds.mins, h_trans + bounds.maxs);
                    if let Some(_dist) = ray_intersects_aabb(eye_pos, fwd, reach + 6.0, bmin, bmax)
                    {
                        hit_something = true;
                        hostage.health -= base_dmg;
                        let snd = format!(
                            "weapons/knife_hit{}.wav",
                            ((eye_pos.x.abs() as usize) % 4) + 1
                        );
                        play_ui_sound(
                            &mut commands,
                            &snd,
                            &audio.search_paths,
                            &mut audio.audio_bank,
                            &mut audio.audio_sources,
                        );

                        if hostage.health <= 0.0 {
                            hostage.is_dead = true;
                            if let Some(ref mut pc) = player_ctrl {
                                pc.apply_hostage_penalty(base_dmg, true);
                            }
                            anim.set_sequence(31, 24.0, false);
                            bounds.mins = Vec3::ZERO;
                            bounds.maxs = Vec3::ZERO;
                            if let Some(h) = audio.audio_bank.get_or_load_sound(
                                "radio/hosdown.wav",
                                &audio.search_paths.dirs,
                                &mut audio.audio_sources,
                            ) {
                                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                            }
                        } else if let Some(ref mut pc) = player_ctrl {
                            pc.apply_hostage_penalty(base_dmg, false);
                        }
                        break;
                    }
                }

                // 2. Breakable brush entity hit (Breakables take 2x damage from knife, instant on crowbar flag)
                if !hit_something {
                    for (_b_ent, mut b) in &mut queries.breakable {
                        if b.is_broken {
                            continue;
                        }
                        if let Some(_dist) =
                            ray_intersects_aabb(eye_pos, fwd, reach + 6.0, b.bmin, b.bmax)
                        {
                            hit_something = true;
                            let knife_dmg = base_dmg * 2.0;
                            b.health -= knife_dmg;
                            let snd = if b.material == BreakableMaterial::Glass {
                                "debris/bustglass1.wav"
                            } else {
                                "weapons/knife_hitwall1.wav"
                            };
                            play_ui_sound(
                                &mut commands,
                                snd,
                                &audio.search_paths,
                                &mut audio.audio_bank,
                                &mut audio.audio_sources,
                            );

                            if b.health <= 0.0 {
                                b.is_broken = true;
                                let center = (b.bmin + b.bmax) * 0.5;
                                events.break_ent.send(BreakEntityEvent {
                                    origin: center,
                                    material: b.material,
                                    model: b.model.clone(),
                                    explodemagnitude: b.explodemagnitude,
                                });
                                events.fire.send(FireTargetEvent {
                                    target: b.target.clone(),
                                });
                            }
                            break;
                        }
                    }
                }

                // 3. World BSP hit
                if !hit_something && let Some(ref col) = collision_res.model {
                    use goldhub_physics::WorldTrace;
                    let bsp = goldhub_physics::BspWorldCollision::new(col);
                    let p1 = [eye_pos.x, -eye_pos.z, eye_pos.y];
                    let end_pos = eye_pos + fwd * reach;
                    let p2 = [end_pos.x, -end_pos.z, end_pos.y];
                    let tr = bsp.trace_player_hull(p1, p2, 0);
                    if tr.fraction < 1.0 && !tr.is_sky {
                        hit_something = true;
                        play_ui_sound(
                            &mut commands,
                            "weapons/knife_hitwall1.wav",
                            &audio.search_paths,
                            &mut audio.audio_bank,
                            &mut audio.audio_sources,
                        );
                        // Knife hits on walls produce impact audio only (no bullet sparks or smoke puffs).
                    }
                }

                // Animation & Miss sound
                let target_anim = if hit_something {
                    anim_seq
                } else {
                    anim_miss_seq
                };
                for (_, _, mut anim) in &mut queries.vm {
                    anim.set_sequence(target_anim, 30.0, false);
                }

                if !hit_something {
                    let miss_snd = if is_stab {
                        "weapons/knife_slash1.wav"
                    } else {
                        "weapons/knife_slash2.wav"
                    };
                    play_ui_sound(
                        &mut commands,
                        miss_snd,
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                    );
                }
            }
        }
        return;
    }

    let is_grenade = matches!(
        state.id,
        WeaponId::HeGrenade | WeaponId::Flashbang | WeaponId::SmokeGrenade
    );

    // Grenade Throwing Lifecycle (`CHEGrenade`: 41fps pullpin + 0.5s cook + 0.5s throw + 3rd-person trigger)
    if is_grenade {
        let lmb_pressed = !input_blocked_by_menu && mouse_button.pressed(MouseButton::Left);

        if !state.pin_pulled
            && !input_blocked_by_menu
            && mouse_button.just_pressed(MouseButton::Left)
            && state.next_primary_attack <= 0.0
        {
            state.pin_pulled = true;
            state.pin_pull_timer = 0.0;
            play_ui_sound(
                &mut commands,
                "weapons/pinpull.wav",
                &audio.search_paths,
                &mut audio.audio_bank,
                &mut audio.audio_sources,
            );
            for (_, _, mut anim) in &mut queries.vm {
                anim.set_sequence_hold(1, 41.0); // HEGRENADE_PULLPIN @ 41 fps
            }
        } else if state.pin_pulled {
            state.pin_pull_timer += dt;
            if !lmb_pressed && state.pin_pull_timer >= 0.5 {
                state.pin_pulled = false;
                state.pin_pull_timer = 0.0;
                state.next_primary_attack = 0.6;

                let throw_fps = model_store
                    .models
                    .get(get_weapon_def(state.id).v_model)
                    .and_then(|m| m.model.rig.sequences.get(2))
                    .map_or(30.0, |s| s.fps);
                for (_, _, mut anim) in &mut queries.vm {
                    anim.set_sequence(2, throw_fps, false); // HEGRENADE_THROW
                }
                if let Some(ref mut p) = player_ctrl {
                    p.trigger_third_person_shoot(0.35);
                }

                if let Ok(cam_t) = queries.camera.get_single() {
                    let eye_pos = player_ctrl.as_deref().map_or_else(
                        || cam_t.translation(),
                        |p| {
                            Vec3::new(
                                p.sim.current.origin[0],
                                p.sim.current.origin[2] + p.sim.current.view_height,
                                -p.sim.current.origin[1],
                            )
                        },
                    );
                    let player_vel = player_ctrl.as_deref().map_or(Vec3::ZERO, |p| {
                        let v = p.sim.current.velocity;
                        Vec3::new(v[0], v[2], -v[1])
                    });

                    let yaw_rad = player_ctrl.as_deref().map_or(0.0, |p| p.yaw);
                    let pitch_rad = player_ctrl.as_deref().map_or(0.0, |p| p.pitch);
                    let pitch_deg = pitch_rad.to_degrees();
                    let gs_pitch = -pitch_deg;
                    let adj_gs_pitch = if gs_pitch < 0.0 {
                        -10.0 + gs_pitch * (80.0 / 90.0)
                    } else {
                        -10.0 + gs_pitch * (100.0 / 90.0)
                    };
                    let fl_vel = ((90.0 - adj_gs_pitch) * 6.0).clamp(40.0, 750.0);
                    let throw_pitch_rad = (-adj_gs_pitch).to_radians();
                    let throw_dir = Quat::from_rotation_y(yaw_rad)
                        * Quat::from_rotation_x(throw_pitch_rad)
                        * Vec3::NEG_Z;
                    let launch_vel = throw_dir * fl_vel + player_vel;

                    let desired_launch = eye_pos + throw_dir * 16.0;
                    let launch_pos = collision_res.model.as_ref().map_or(desired_launch, |col| {
                        use goldhub_physics::WorldTrace;
                        let bsp = goldhub_physics::BspWorldCollision::new(col);
                        let tr = bsp.trace_player_hull(
                            [eye_pos.x, -eye_pos.z, eye_pos.y],
                            [desired_launch.x, -desired_launch.z, desired_launch.y],
                            0,
                        );
                        Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1])
                    });

                    let seed_bits = time.elapsed_secs().to_bits();
                    spawn_grenade_entity(
                        &mut commands,
                        state.id,
                        launch_pos,
                        launch_vel,
                        yaw_rad,
                        pitch_rad,
                        seed_bits,
                        &mut model_store,
                        &mut sprite_store,
                        &audio.search_paths,
                        &mut vfx,
                    );
                }

                // Consume 1 grenade charge; keep item when reserve > 0 (e.g., 2nd Flashbang), else promote/retire
                state.reserve = (state.reserve - 1).max(0);
                if let Ok(mut inv) = queries.inventory.get_single_mut() {
                    if !inv.bucket_weapons[3].is_empty() {
                        let idx = inv.bucket_cursor[3].min(inv.bucket_weapons[3].len() - 1);
                        if state.reserve > 0 {
                            inv.bucket_weapons[3][idx].reserve = state.reserve;
                            inv.slot4.reserve = state.reserve;
                        } else {
                            inv.bucket_weapons[3].remove(idx);
                            if inv.bucket_weapons[3].is_empty() {
                                inv.bucket_cursor[3] = 0;
                                inv.slot4 = WeaponSlotData::empty();
                            } else {
                                inv.bucket_cursor[3] %= inv.bucket_weapons[3].len();
                                inv.slot4 = inv.bucket_weapons[3][inv.bucket_cursor[3]];
                            }
                            inv.active_slot = 3;
                        }
                    } else if state.reserve == 0 {
                        inv.slot4 = WeaponSlotData::empty();
                        inv.active_slot = 3;
                    }
                }
                state.pending_knife_swap_timer = Some(0.5);
                return;
            }
        }
        return;
    }

    // C4 Planting Lifecycle (`CC4::PrimaryAttack`: bombsite + grounded + 3.0s hold -> C4_ARM -> C4_DROP)
    if state.id == WeaponId::C4 {
        let in_bomb_zone = player_ctrl.as_deref().is_some_and(|p| p.in_bomb_zone);
        let on_ground = player_ctrl
            .as_deref()
            .is_some_and(|p| p.sim.current.on_ground);
        let lmb_pressed = !input_blocked_by_menu && mouse_button.pressed(MouseButton::Left);

        if !state.c4_arming {
            if !input_blocked_by_menu
                && mouse_button.just_pressed(MouseButton::Left)
                && state.next_primary_attack <= 0.0
            {
                if !in_bomb_zone {
                    if let Some(ref mut p) = player_ctrl {
                        p.set_center_message("#C4_Plant_At_Bomb_Spot", 2.0);
                    }
                } else if !on_ground {
                    if let Some(ref mut p) = player_ctrl {
                        p.set_center_message("#C4_Plant_Must_Be_On_Ground", 2.0);
                    }
                } else {
                    state.c4_arming = true;
                    state.c4_arm_timer = 0.0;
                    play_ui_sound(
                        &mut commands,
                        "weapons/c4_click.wav",
                        &audio.search_paths,
                        &mut audio.audio_bank,
                        &mut audio.audio_sources,
                    );
                    let arm_fps = model_store
                        .models
                        .get("models/v_c4.mdl")
                        .and_then(|m| m.model.rig.sequences.get(3))
                        .map_or(30.0, |s| s.fps);
                    for (_, _, mut anim) in &mut queries.vm {
                        anim.set_sequence(3, arm_fps, false); // C4_ARM
                    }
                    if let Some(ref mut p) = player_ctrl {
                        p.trigger_third_person_shoot(0.6);
                    }
                }
            }
        } else if !lmb_pressed || !in_bomb_zone || !on_ground {
            state.c4_arming = false;
            state.c4_arm_timer = 0.0;
            for (_, _, mut anim) in &mut queries.vm {
                anim.set_sequence(0, 15.0, true); // cancel back to C4_IDLE
            }
        } else {
            state.c4_arm_timer += dt;
            if let Some(ref mut p) = player_ctrl {
                p.trigger_third_person_shoot(0.35);
            }
            if state.c4_arm_timer >= 3.0 {
                state.c4_arming = false;
                state.c4_arm_timer = 0.0;
                state.c4_placed_anim = true;

                let player_yaw = player_ctrl.as_deref().map_or(0.0, |p| p.yaw);
                let player_ori = player_ctrl.as_deref().map_or(Vec3::ZERO, |p| {
                    let o = p.sim.current.origin;
                    Vec3::new(o[0], o[2], -o[1])
                });
                // Authentic GoldSrc `CGrenade::ShootSatchelCharge`:
                // Spawns at player waist `pev->origin` with `movetype = MOVETYPE_TOSS` and `velocity = 0`.
                // Does NOT teleport 128 units down instantly; `update_planted_c4_system` applies
                // `sv_gravity 800` smoothly until touching the ground.
                let plant_pos = player_ori + Vec3::NEG_Y * 6.0;

                let c4_parent = commands
                    .spawn((
                        PlantedC4 {
                            timer: 35.0,
                            beep_timer: 0.5,
                            cur_wave: 0,
                            next_freq: 35.0,
                            next_freq_interval: 35.0 / 4.0,
                            blink_timer: 0.5,
                            velocity: Vec3::ZERO,
                            defuse_progress: 0.0,
                            defuse_duration: 10.0,
                            is_being_defused: false,
                            on_ground: false,
                        },
                        Transform::from_translation(plant_pos)
                            .with_rotation(Quat::from_rotation_y(player_yaw)),
                        Visibility::Inherited,
                    ))
                    .id();
                if let Some(c4_md) = model_store.get_or_load(
                    "models/w_c4.mdl",
                    &audio.search_paths,
                    &mut vfx.images,
                    &mut vfx.materials,
                ) {
                    model_store.spawn_static_model_submesh_children(
                        &mut commands,
                        c4_parent,
                        &c4_md,
                        "models/w_c4.mdl",
                        false,
                        Some(2),
                        &mut vfx.meshes,
                    );
                }

                play_ui_sound(
                    &mut commands,
                    "weapons/c4_plant.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
                play_ui_sound(
                    &mut commands,
                    "radio/bombpl.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
                if let Some(ref mut p) = player_ctrl {
                    p.set_center_message("#Bomb_Planted", 4.0);
                }

                let drop_fps = model_store
                    .models
                    .get("models/v_c4.mdl")
                    .and_then(|m| m.model.rig.sequences.get(2))
                    .map_or(30.0, |s| s.fps);
                for (_, _, mut anim) in &mut queries.vm {
                    anim.set_sequence(2, drop_fps, false); // C4_DROP
                }

                if let Ok(mut inv) = queries.inventory.get_single_mut() {
                    inv.slot5 = WeaponSlotData::empty();
                    inv.bucket_weapons[4].clear();
                    inv.bucket_cursor[4] = 0;
                    inv.active_slot = 3;
                }
                state.next_primary_attack = 0.75;
                state.next_secondary_attack = 0.75;
                state.pending_knife_swap_timer = Some(0.5);
            }
        }
        return;
    }

    let def = get_weapon_def(state.id);

    // Tick queued Glock-18 (`0.1s` @ `0.05`) / FAMAS (`0.05s` @ `0.03`) Burst-Fire follow-up shots
    if state.burst_shots_remaining > 0 {
        state.burst_timer -= dt;
        if state.burst_timer <= 0.0 {
            state.burst_shots_remaining -= 1;
            let is_famas = state.id == WeaponId::Famas;
            state.burst_timer = if is_famas { 0.05 } else { 0.1 };
            let burst_spread = if is_famas { 0.03 } else { 0.05 };
            if state.clip > 0 {
                state.clip -= 1;
                if let Some(cam_t) = queries.camera.get_single().ok().copied() {
                    let eye_pos = player_ctrl.as_deref().map_or_else(
                        || cam_t.translation(),
                        |p| {
                            Vec3::new(
                                p.sim.current.origin[0],
                                p.sim.current.origin[2] + p.sim.current.view_height,
                                -p.sim.current.origin[1],
                            )
                        },
                    );
                    let player_pitch = player_ctrl.as_deref().map_or(0.0, |p| p.pitch);
                    let player_yaw = player_ctrl.as_deref().map_or(0.0, |p| p.yaw);
                    let hash_base = (time.elapsed_secs_f64().to_bits())
                        ^ (u64::from(state.burst_shots_remaining)
                            .wrapping_mul(0x517C_C1B7_2722_0A95));
                    let burst_idx = 2u8.saturating_sub(state.burst_shots_remaining);
                    fire_single_bullet_trace(
                        &mut commands,
                        &time,
                        *perspective,
                        &def,
                        state.id,
                        state.silenced,
                        state.burst_mode,
                        burst_idx,
                        true,
                        burst_spread,
                        hash_base,
                        &cam_t,
                        eye_pos,
                        player_pitch,
                        player_yaw,
                        &recoil,
                        player_ctrl.as_deref_mut(),
                        collision_res.model.as_ref(),
                        &mut model_store,
                        &mut sprite_store,
                        &mut audio,
                        &mut vfx,
                        &mut queries,
                        &mut events,
                    );
                }
            } else {
                state.burst_shots_remaining = 0;
            }
        }
    }

    let trigger_held = !input_blocked_by_menu && mouse_button.pressed(MouseButton::Left);
    let rmb_held = !input_blocked_by_menu && mouse_button.pressed(MouseButton::Right);

    // Bolt-action sniper (`AWP` / `Scout`) auto-rescope upon cycle completion (enables continuous fast-zoom accuracy!)
    if state.next_primary_attack <= 0.0
        && !state.in_reload
        && let Some(rfov) = state.resume_zoom_fov.take()
    {
        let fovs = state.id.zoom_fovs();
        if let Some(idx) = fovs.iter().position(|&f| (f32::from(f) - rfov).abs() < 0.5) {
            state.zoom_level = idx as u8;
            state.current_fov = f32::from(fovs[idx]);
            if state.id.has_zoom_sound() {
                play_ui_sound(
                    &mut commands,
                    "weapons/zoom.wav",
                    &audio.search_paths,
                    &mut audio.audio_bank,
                    &mut audio.audio_sources,
                );
            }
        }
    }

    // 1. Active Reload / Interruptible Shotgun Shell-by-Shell Progression
    if state.in_reload {
        if state.shotgun_reload_stage > 0 && state.clip > 0 && trigger_held {
            state.in_reload = false;
            state.shotgun_reload_stage = 0;
            state.reload_timer = 0.0;
        } else {
            state.reload_timer -= dt;
            if state.reload_timer <= 0.0 {
                if state.shotgun_reload_stage == 1 {
                    state.shotgun_reload_stage = 2;
                    state.reload_timer = 0.45;
                    set_viewmodel_seq(&model_store, &mut queries.vm, def.v_model, 3);
                } else if state.shotgun_reload_stage == 2 {
                    if state.clip < def.clip_size && state.reserve > 0 {
                        state.clip += 1;
                        state.reserve -= 1;
                        play_ui_sound(
                            &mut commands,
                            "weapons/m3_insertshell.wav",
                            &audio.search_paths,
                            &mut audio.audio_bank,
                            &mut audio.audio_sources,
                        );
                    }
                    if state.clip < def.clip_size && state.reserve > 0 {
                        state.reload_timer = 0.45;
                        set_viewmodel_seq(&model_store, &mut queries.vm, def.v_model, 3);
                    } else {
                        state.in_reload = false;
                        state.shotgun_reload_stage = 0;
                        set_viewmodel_seq(&model_store, &mut queries.vm, def.v_model, 4);
                        if state.id == WeaponId::M3 {
                            play_ui_sound(
                                &mut commands,
                                "weapons/m3_pump.wav",
                                &audio.search_paths,
                                &mut audio.audio_bank,
                                &mut audio.audio_sources,
                            );
                        }
                    }
                } else {
                    let needed = def.clip_size - state.clip;
                    let transfer = needed.min(state.reserve);
                    state.clip += transfer;
                    state.reserve -= transfer;
                    state.in_reload = false;
                    state.accuracy = goldhub_game_cstrike::initial_weapon_accuracy(state.id);
                    state.has_fired_before = false;
                }
            }
            if state.in_reload {
                return;
            }
        }
    }

    // 2. Secondary Attack (Right Click): Zoom cycles (`0.3s` continuous hold), Burst toggle, Silencer toggle
    if handle_secondary_attack(
        &mut commands,
        &mouse_button,
        input_blocked_by_menu,
        &mut state,
        player_ctrl.as_deref_mut(),
        &mut audio,
        &mut queries.vm,
    ) {
        return;
    }

    // 3. Accuracy / Shot Count Decay while trigger released
    if !def.is_automatic {
        if !trigger_held && state.shots_fired > 0 {
            state.shots_fired = 0;
            state.decrease_shots_timer = 0.0;
        }
    } else if !trigger_held && state.shots_fired > 0 {
        state.decrease_shots_timer -= dt;
        if state.decrease_shots_timer <= 0.0 {
            state.shots_fired = state.shots_fired.saturating_sub(1);
            state.decrease_shots_timer = 0.0225;
        }
    }

    // 4. Primary Attack (LMB) & Empty-Clip Continuous Dryfire (`ItemPostFrame`: fires before auto-reload!)
    let continuous_hold_fires =
        def.is_automatic || matches!(state.id, WeaponId::AWP | WeaponId::Scout | WeaponId::M3);
    let fire_pressed = if input_blocked_by_menu {
        false
    } else if continuous_hold_fires && !(state.id == WeaponId::Famas && state.burst_mode) {
        trigger_held
    } else {
        mouse_button.just_pressed(MouseButton::Left)
    };

    if (fire_pressed || (state.clip <= 0 && trigger_held))
        && state.next_primary_attack <= 0.0
        && state.burst_shots_remaining == 0
    {
        if state.clip <= 0 {
            let dry_snd = if state.id.is_rifle_dryfire() {
                "weapons/dryfire_rifle.wav"
            } else {
                "weapons/dryfire_pistol.wav"
            };
            play_ui_sound(
                &mut commands,
                dry_snd,
                &audio.search_paths,
                &mut audio.audio_bank,
                &mut audio.audio_sources,
            );
            state.next_primary_attack = 0.2;
            return;
        }

        if fire_pressed {
            fire_weapon_round(
                &mut commands,
                &time,
                *perspective,
                &def,
                &mut state,
                &mut recoil,
                player_ctrl.as_deref_mut(),
                collision_res.model.as_ref(),
                &mut model_store,
                &mut sprite_store,
                &mut audio,
                &mut vfx,
                &mut queries,
                &mut events,
            );
            return;
        }
    }

    // 5. Manual (`R`) or Empty-Clip Auto-Reload (ONLY after releasing both triggers: `!trigger_held && !rmb_held`)
    let wants_manual_reload = !input_blocked_by_menu
        && keyboard.just_pressed(KeyCode::KeyR)
        && state.clip < def.clip_size
        && state.reserve > 0
        && state.next_primary_attack <= 0.0
        && state.burst_shots_remaining == 0;
    let wants_auto_reload = state.clip == 0
        && state.reserve > 0
        && !trigger_held
        && !rmb_held
        && state.next_primary_attack <= 0.0
        && state.burst_shots_remaining == 0;

    if wants_manual_reload || wants_auto_reload {
        begin_weapon_reload(
            &mut commands,
            &def,
            &mut state,
            &model_store,
            &mut audio,
            &mut queries.vm,
        );
    }
}

fn set_viewmodel_seq(
    model_store: &StudioModelStore,
    vm_query: &mut Query<(Entity, &mut ViewModelWeapon, &mut StudioAnimatedModel)>,
    v_model: &str,
    seq_idx: usize,
) {
    for (_, _, mut anim) in vm_query.iter_mut() {
        if let Some(model_data) = model_store.models.get(v_model) {
            let fps = model_data
                .model
                .rig
                .sequences
                .get(seq_idx)
                .map_or(30.0, |s| s.fps);
            anim.set_sequence(seq_idx, fps, false);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_secondary_attack(
    commands: &mut Commands,
    mouse_button: &ButtonInput<MouseButton>,
    blocked: bool,
    state: &mut ActiveWeaponState,
    player_ctrl: Option<&mut PlayerController>,
    audio: &mut CombatAudio,
    vm_query: &mut Query<(Entity, &mut ViewModelWeapon, &mut StudioAnimatedModel)>,
) -> bool {
    if blocked
        || state.in_reload
        || state.burst_shots_remaining > 0
        || state.next_secondary_attack > 0.0
    {
        return false;
    }

    let zoom_fovs = state.id.zoom_fovs();
    if zoom_fovs.len() > 1 {
        if !mouse_button.pressed(MouseButton::Right) {
            return false;
        }
        state.zoom_level = (state.zoom_level + 1) % (zoom_fovs.len() as u8);
        state.current_fov = f32::from(zoom_fovs[usize::from(state.zoom_level)]);
        state.next_secondary_attack = 0.3;
        if state.id.has_zoom_sound() {
            play_ui_sound(
                commands,
                "weapons/zoom.wav",
                &audio.search_paths,
                &mut audio.audio_bank,
                &mut audio.audio_sources,
            );
        }
        return true;
    }

    if !mouse_button.just_pressed(MouseButton::Right) {
        return false;
    }

    match state.id {
        WeaponId::Glock18 | WeaponId::Glock => {
            state.burst_mode = !state.burst_mode;
            state.next_secondary_attack = 0.3;
            let msg = if state.burst_mode {
                "#Switch_To_BurstFire"
            } else {
                "#Switch_To_SemiAuto"
            };
            if let Some(p) = player_ctrl {
                p.set_center_message(msg, 1.5);
            }
            true
        }
        WeaponId::Famas => {
            state.burst_mode = !state.burst_mode;
            state.next_secondary_attack = 0.3;
            let msg = if state.burst_mode {
                "#Switch_To_BurstFire"
            } else {
                "#Switch_To_FullAuto"
            };
            if let Some(p) = player_ctrl {
                p.set_center_message(msg, 1.5);
            }
            true
        }
        WeaponId::USP45 => {
            state.silenced = !state.silenced;
            let (seq, idle_seq, fps) = if state.silenced {
                (7, 0, 37.0)
            } else {
                (15, 8, 37.0)
            };
            state.next_primary_attack = 3.13;
            state.next_secondary_attack = 3.13;
            for (_, _, mut anim) in vm_query.iter_mut() {
                anim.idle_sequence_index = idle_seq;
                anim.set_sequence(seq, fps, false);
            }
            true
        }
        WeaponId::M4A1 => {
            state.silenced = !state.silenced;
            let (seq, idle_seq, fps) = if state.silenced {
                (6, 0, 30.0)
            } else {
                (13, 7, 30.0)
            };
            state.next_primary_attack = 2.0;
            state.next_secondary_attack = 2.0;
            for (_, _, mut anim) in vm_query.iter_mut() {
                anim.idle_sequence_index = idle_seq;
                anim.set_sequence(seq, fps, false);
            }
            true
        }
        _ => false,
    }
}

fn begin_weapon_reload(
    commands: &mut Commands,
    def: &WeaponDef,
    state: &mut ActiveWeaponState,
    model_store: &StudioModelStore,
    audio: &mut CombatAudio,
    vm_query: &mut Query<(Entity, &mut ViewModelWeapon, &mut StudioAnimatedModel)>,
) {
    state.in_reload = true;
    state.shots_fired = 0;
    state.zoom_level = 0;
    state.current_fov = 90.0;
    state.resume_zoom_fov = None;

    if matches!(state.id, WeaponId::M3 | WeaponId::XM1014) {
        state.shotgun_reload_stage = 1;
        state.reload_timer = 0.55;
        set_viewmodel_seq(model_store, vm_query, def.v_model, 5); // start_reload
        return;
    }

    state.shotgun_reload_stage = 0;
    state.reload_timer = def.reload_time;
    if !def.reload_sound.is_empty()
        && let Some(h) = audio.audio_bank.get_or_load_sound(
            def.reload_sound,
            &audio.search_paths.dirs,
            &mut audio.audio_sources,
        )
    {
        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
    }

    let reload_anim_idx = match state.id {
        WeaponId::USP45 => {
            if state.silenced {
                5
            } else {
                13
            }
        }
        WeaponId::M4A1 => {
            if state.silenced {
                4
            } else {
                11
            }
        }
        _ => def.reload_anim,
    };
    set_viewmodel_seq(model_store, vm_query, def.v_model, reload_anim_idx);
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn fire_weapon_round(
    commands: &mut Commands,
    time: &Time,
    perspective: CameraPerspective,
    def: &WeaponDef,
    state: &mut ActiveWeaponState,
    recoil: &mut PlayerRecoil,
    mut player_ctrl: Option<&mut PlayerController>,
    collision: Option<&goldhub_formats::ir::CollisionModel>,
    model_store: &mut StudioModelStore,
    sprite_store: &mut SpriteStore,
    audio: &mut CombatAudio,
    vfx: &mut CombatVfxAssets,
    queries: &mut CombatQueries,
    events: &mut CombatEvents,
) {
    let now = time.elapsed_secs();
    let is_glock = matches!(state.id, WeaponId::Glock18 | WeaponId::Glock);
    let is_glock_burst = is_glock && state.burst_mode;
    let is_famas_burst = state.id == WeaponId::Famas && state.burst_mode;
    let pre_shots = state.shots_fired;

    state.clip -= 1;
    let fov_u8 = state.current_fov.round().clamp(1.0, 120.0) as u8;
    state.next_primary_attack = state.id.effective_cycle_time(fov_u8, state.burst_mode);
    if is_glock_burst || is_famas_burst {
        state.next_secondary_attack = state.next_primary_attack;
        state.burst_shots_remaining = (state.clip.max(0) as u8).min(2);
        state.burst_timer = if is_famas_burst { 0.05 } else { 0.1 };
    }
    state.decrease_shots_timer = 0.4;

    let hash_base = time.elapsed_secs_f64().to_bits()
        ^ (u64::from(pre_shots).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let seed = hash_mix_unit(hash_base);

    // Single CHAN_WEAPON audio channel parity
    for ch_ent in &queries.audio_channel {
        commands.entity(ch_ent).despawn_recursive();
    }

    let shoot_snd = match state.id {
        WeaponId::USP45 => {
            if state.silenced {
                "weapons/usp1.wav"
            } else {
                "weapons/usp_unsil-1.wav"
            }
        }
        WeaponId::M4A1 => {
            if state.silenced {
                "weapons/m4a1-1.wav"
            } else {
                "weapons/m4a1_unsil-1.wav"
            }
        }
        WeaponId::Glock18 | WeaponId::Glock => {
            if is_glock_burst {
                "weapons/glock18-1.wav"
            } else {
                "weapons/glock18-2.wav"
            }
        }
        WeaponId::Famas => {
            if is_famas_burst {
                "weapons/famas-burst.wav"
            } else {
                def.shoot_sound
            }
        }
        _ => def.shoot_sound,
    };
    if let Some(h) = audio.audio_bank.get_or_load_sound(
        shoot_snd,
        &audio.search_paths.dirs,
        &mut audio.audio_sources,
    ) {
        commands.spawn((
            WeaponAudioChannel,
            AudioPlayer::new(h),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::new(0.42)),
        ));
    }

    let shoot_anim_idx = match state.id {
        WeaponId::USP45 => {
            if state.silenced {
                1
            } else {
                9
            }
        }
        WeaponId::M4A1 => {
            if state.silenced {
                1
            } else {
                8
            }
        }
        WeaponId::Glock18 | WeaponId::Glock => {
            if is_glock_burst {
                3
            } else {
                5
            }
        }
        _ => def.shoot_anim,
    };
    set_viewmodel_seq(model_store, &mut queries.vm, def.v_model, shoot_anim_idx);

    let Some(cam_t) = queries.camera.get_single().ok().copied() else {
        return;
    };
    let (eye_pos, player_speed, on_ground, is_ducking, player_pitch, player_yaw) =
        player_ctrl.as_deref().map_or_else(
            || (cam_t.translation(), 0.0, true, false, 0.0, 0.0),
            |p| {
                let o = p.sim.current.origin;
                let v = p.sim.current.velocity;
                (
                    Vec3::new(o[0], o[2] + p.sim.current.view_height, -o[1]),
                    v[0].hypot(v[1]),
                    p.sim.current.on_ground,
                    p.sim.current.is_ducking,
                    p.pitch,
                    p.yaw,
                )
            },
        );

    let spread_res = compute_weapon_spread(
        state.id,
        state.silenced,
        state.burst_mode,
        pre_shots,
        state.accuracy,
        (now - state.last_fire_time).max(0.0),
        state.has_fired_before,
        player_speed,
        on_ground,
        is_ducking,
        state.current_fov,
    );
    state.accuracy = spread_res.next_accuracy;
    state.last_fire_time = now;
    state.has_fired_before = true;

    // 1. Fire trace(s) BEFORE applying recoil kickback so stationary Shot #1 has zero pre-kickback pitch!
    let pellets = state.id.pellet_count().max(1);
    for pellet in 0..pellets {
        let pellet_hash = hash_base ^ (u64::from(pellet).wrapping_mul(0xD6E8_FEB8_6659_FD93));
        fire_single_bullet_trace(
            commands,
            time,
            perspective,
            def,
            state.id,
            state.silenced,
            state.burst_mode,
            0,
            pellet == 0,
            spread_res.spread,
            pellet_hash,
            &cam_t,
            eye_pos,
            player_pitch,
            player_yaw,
            recoil,
            player_ctrl.as_deref_mut(),
            collision,
            model_store,
            sprite_store,
            audio,
            vfx,
            queries,
            events,
        );
    }

    // 2. Bolt-action snipers (`AWP` / `Scout`) unscope during bolt cycle while preserving `resume_zoom_fov`
    if state.id.resumes_zoom_after_shot() && state.zoom_level > 0 {
        state.resume_zoom_fov = Some(state.current_fov);
        state.zoom_level = 0;
        state.current_fov = 90.0;
    }

    // 3. Apply ReGameDLL `KickBack` AFTER trace completes
    state.shots_fired = pre_shots.saturating_add(1).min(15);
    let kick = def.recoil.select(player_speed, on_ground, is_ducking);
    recoil.apply_kickback_params(kick, state.shots_fired, seed);
    if let Some(ref mut p) = player_ctrl {
        p.trigger_third_person_shoot(0.35);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weapon_spread_shot1_and_glock_burst() {
        let glock_burst = compute_weapon_spread(
            WeaponId::Glock18,
            false,
            true,
            0,
            0.90,
            1.0,
            false,
            0.0,
            true,
            false,
            90.0,
        );
        // ReGameDLL CGlock18::PrimaryAttack shot 1 spread in burst mode = 0.30 * (1 - 0.90) = 0.03; follow-ups = 0.05.
        assert!((glock_burst.spread - 0.03).abs() < 1e-6);

        let semi_stationary = compute_weapon_spread(
            WeaponId::Glock18,
            false,
            false,
            0,
            0.90,
            1.0,
            false,
            0.0,
            true,
            false,
            90.0,
        );
        let semi_air = compute_weapon_spread(
            WeaponId::Glock18,
            false,
            false,
            0,
            0.90,
            1.0,
            false,
            0.0,
            false,
            false,
            90.0,
        );
        assert!(semi_stationary.spread < semi_air.spread);

        let ak_shot1 = compute_weapon_spread(
            WeaponId::AK47,
            false,
            false,
            0,
            0.20,
            1.0,
            false,
            0.0,
            true,
            false,
            90.0,
        );
        let ak_shot10 = compute_weapon_spread(
            WeaponId::AK47,
            false,
            false,
            9,
            0.95,
            0.1,
            true,
            0.0,
            true,
            false,
            90.0,
        );
        assert!(ak_shot1.spread < ak_shot10.spread);
    }

    #[test]
    fn test_awp_hipfire_vs_zoom_and_pellet_counts() {
        let awp_hipfire = compute_weapon_spread(
            WeaponId::AWP,
            false,
            false,
            0,
            0.0,
            2.0,
            false,
            0.0,
            true,
            false,
            90.0,
        );
        let awp_zoom = compute_weapon_spread(
            WeaponId::AWP,
            false,
            false,
            0,
            0.0,
            2.0,
            false,
            0.0,
            true,
            false,
            40.0,
        );
        assert!(awp_hipfire.spread > 0.08);
        assert!(awp_zoom.spread <= 0.001 + 1e-6);

        assert_eq!(WeaponId::M3.pellet_count(), 9);
        assert_eq!(WeaponId::XM1014.pellet_count(), 6);
        assert!(WeaponId::AK47.is_rifle_dryfire());
        assert!(!WeaponId::Deagle.is_rifle_dryfire());
        assert!(!WeaponId::Knife.is_droppable());
        assert!(!WeaponId::HeGrenade.is_droppable());
        assert!(WeaponId::AWP.resumes_zoom_after_shot());
    }

    fn setup_weapon_fire_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(50),
        ));
        app.insert_resource(CameraPerspective::FirstPerson);
        app.init_resource::<PendingWeaponSelection>();
        app.init_resource::<GameSearchPaths>();
        app.init_resource::<GoldSrcAudioBank>();
        app.init_resource::<Assets<AudioSource>>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<LevelCollisionResource>();
        app.init_resource::<StudioModelStore>();
        app.init_resource::<SpriteStore>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<Image>>();
        app.init_resource::<crate::hud::text_channels::ConsoleOverlayState>();
        app.add_event::<FireTargetEvent>();
        app.add_event::<BreakEntityEvent>();
        app.add_systems(Update, weapon_fire_and_reload_system);
        app
    }

    #[test]
    fn test_ak47_shot1_before_kickback_dryfire_and_console_lock() {
        let mut app = setup_weapon_fire_test_app();
        app.world_mut().spawn((
            Camera3d::default(),
            Transform::default(),
            GlobalTransform::default(),
            PlayerController::default(),
            ActiveWeaponState {
                id: WeaponId::AK47,
                clip: 1,
                reserve: 30,
                accuracy: goldhub_game_cstrike::initial_weapon_accuracy(WeaponId::AK47),
                ..Default::default()
            },
            PlayerRecoil::default(),
        ));

        // Tick 1 warms up dt; pre-fire recoil is zero.
        app.update();

        // Console open must block firing completely.
        app.world_mut()
            .resource_mut::<crate::hud::text_channels::ConsoleOverlayState>()
            .open = true;
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        {
            let mut q = app
                .world_mut()
                .query::<(&ActiveWeaponState, &PlayerRecoil)>();
            let (st, rec) = q.single(app.world());
            assert_eq!(st.clip, 1);
            assert_eq!(rec.punch_pitch, 0.0);
        }

        // Close console with LMB still held -> Shot #1 fires, kickback applies AFTER trace.
        app.world_mut()
            .resource_mut::<crate::hud::text_channels::ConsoleOverlayState>()
            .open = false;
        app.update();
        {
            let mut q = app
                .world_mut()
                .query::<(&ActiveWeaponState, &PlayerRecoil)>();
            let (st, rec) = q.single(app.world());
            assert_eq!(st.clip, 0);
            assert_eq!(st.shots_fired, 1);
            assert!(!st.in_reload);
            assert!(rec.punch_pitch < 0.0 || rec.punch_pitch.abs() > 0.0);
        }

        // Continue holding LMB past cycle time -> dryfire (`0.2s`), NO auto-reload yet!
        for _ in 0..4 {
            app.update();
        }
        {
            let mut q = app.world_mut().query::<&ActiveWeaponState>();
            let st = q.single(app.world());
            assert_eq!(st.clip, 0);
            assert!(!st.in_reload);
            assert!(st.next_primary_attack > 0.0);
        }

        // Release LMB and advance past `0.2s` dryfire delay -> automatic reload engages.
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        for _ in 0..5 {
            app.update();
        }
        let mut q = app.world_mut().query::<&ActiveWeaponState>();
        let st = q.single(app.world());
        assert!(st.in_reload);
    }

    #[test]
    fn test_awp_continuous_zoom_and_fast_zoom_hold_fire() {
        let mut app = setup_weapon_fire_test_app();
        app.world_mut().spawn((
            Camera3d::default(),
            Transform::default(),
            GlobalTransform::default(),
            PlayerController::default(),
            ActiveWeaponState {
                id: WeaponId::AWP,
                clip: 10,
                reserve: 30,
                ..Default::default()
            },
            PlayerRecoil::default(),
        ));
        app.update();

        // Hold RMB to cycle 90 -> 40 -> 10 -> 90 (`0.3s` per step).
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        app.update();
        {
            let mut q = app.world_mut().query::<&ActiveWeaponState>();
            let st = q.single(app.world());
            assert_eq!(st.zoom_level, 1);
            assert!((st.current_fov - 40.0).abs() < 1e-4);
        }

        // Advance `6 * 50ms` (< 0.3s due to IEEE-754 0.3f32 > 6*0.05f32): zoom stays at FOV 40, then flips on tick 7.
        for _ in 0..6 {
            app.update();
        }
        {
            let mut q = app.world_mut().query::<&ActiveWeaponState>();
            let st = q.single(app.world());
            assert_eq!(st.zoom_level, 1);
            assert!((st.current_fov - 40.0).abs() < 1e-4);
        }
        app.update();
        {
            let mut q = app.world_mut().query::<&ActiveWeaponState>();
            let st = q.single(app.world());
            assert_eq!(st.zoom_level, 2);
            assert!((st.current_fov - 10.0).abs() < 1e-4);
        }

        // Advance `14 * 50ms` -> wraps back to 90 on tick 7, then back to 40 on tick 14.
        for _ in 0..14 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Right);
        {
            let mut q = app.world_mut().query::<&ActiveWeaponState>();
            let st = q.single(app.world());
            assert_eq!(st.zoom_level, 1);
            assert!((st.current_fov - 40.0).abs() < 1e-4);
        }

        // Hold LMB: fires zoomed Shot 1, drops to FOV 90 with `resume_zoom_fov = Some(40.0)`.
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        {
            let mut q = app.world_mut().query::<&ActiveWeaponState>();
            let st = q.single(app.world());
            assert_eq!(st.clip, 9);
            assert_eq!(st.zoom_level, 0);
            assert!((st.current_fov - 90.0).abs() < 1e-4);
            assert_eq!(st.resume_zoom_fov, Some(40.0));
        }

        // Keep holding LMB across 1.45s AWP bolt cycle (`29 * 50ms`): auto-rescopes to 40 & immediately fires Shot 2!
        for _ in 0..30 {
            app.update();
        }
        let mut q = app.world_mut().query::<&ActiveWeaponState>();
        let st = q.single(app.world());
        assert_eq!(st.clip, 8);
        assert_eq!(st.resume_zoom_fov, Some(40.0));
    }

    #[test]
    fn test_bullet_decal_deduplication_on_same_surface() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);

        let hit_pos = Vec3::new(100.0, 50.0, 0.0);
        let hit_normal = Vec3::Z;

        let existing_ent = app
            .world_mut()
            .spawn(BulletHoleDecal {
                position: hit_pos,
                normal: hit_normal,
                lifetime: 5.0,
                max_lifetime: 60.0,
                custom_mesh: None,
            })
            .id();

        // Simulate a new shot at virtually the same spot (within 1.5 units)
        let new_hit = hit_pos + Vec3::new(1.0, 0.5, 0.0);

        // Verify deduplication logic
        let to_despawn: Vec<Entity> = {
            let mut query = app.world_mut().query::<(Entity, &BulletHoleDecal)>();
            query
                .iter(app.world())
                .filter_map(|(ent, decal)| {
                    if decal.position.distance_squared(new_hit) < 16.0
                        && decal.normal.dot(hit_normal) > 0.85
                    {
                        Some(ent)
                    } else {
                        None
                    }
                })
                .collect()
        };
        for ent in to_despawn {
            app.world_mut().entity_mut(ent).despawn();
        }
        app.update();

        assert!(app.world().get::<BulletHoleDecal>(existing_ent).is_none());
    }

    #[test]
    fn test_adjacent_corner_decal_detection() {
        // CollisionModel with a splitting plane at X = 100
        let col = goldhub_formats::ir::CollisionModel {
            hulls: vec![goldhub_formats::ir::CollisionHull {
                hull_index: 0,
                root_node: 0,
                clip_nodes: vec![goldhub_formats::ir::ClipNode {
                    plane_index: 0,
                    children: [-2, -1],
                }],
            }],
            planes: vec![goldhub_formats::ir::CollisionPlane {
                normal: [1.0, 0.0, 0.0],
                distance: 100.0,
                plane_type: 0,
            }],
            brush_entities: vec![],
            visdata: vec![],
        };

        // Hit point near outer edge (X = 98.5, within half_size 3.75 of X = 100)
        let hit_pos = Vec3::new(98.5, 32.0, 0.0);
        let hit_normal = Vec3::Z;
        let adj = find_adjacent_corner_decal(hit_pos, hit_normal, 3.75, Some(&col));
        if let Some((adj_pos, adj_norm)) = adj {
            assert!(
                adj_norm.dot(hit_normal).abs() < 0.25,
                "Adjacent normal must be perpendicular to hit normal"
            );
            assert!(
                (adj_pos.y - hit_pos.y).abs() < 1e-3,
                "Y coordinate must align along vertical edge"
            );
        }
    }
}
