//! First-person ViewModel (weapons and hands) attached to the player camera.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::view::RenderLayers;
use goldhub_physics::WorldTrace;

/// Dedicated render layer for the first-person ViewModel overlay camera (prevents wall clipping).
pub const VIEWMODEL_RENDER_LAYER: usize = 1;

use super::animation::StudioAnimatedModel;
use super::store::StudioModelStore;
use super::world_entity::{EntityCollisionBounds, WorldStudioEntity};
use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::player::{LevelCollisionResource, PlayerController};

/// Component marking the first-person ViewModel weapon entity.
#[derive(Component, Debug, Clone)]
pub struct ViewModelWeapon {
    pub last_swing: u8,
    pub last_swing_time: f32,
    pub attack_cooldown: f32,
    pub bob_phase: f32,
    pub current_sway: Vec2,
}

impl Default for ViewModelWeapon {
    fn default() -> Self {
        Self {
            last_swing: 0,
            last_swing_time: -10.0,
            attack_cooldown: 0.0,
            bob_phase: 0.0,
            current_sway: Vec2::ZERO,
        }
    }
}

/// Marker component indicating a camera entity already has a ViewModel spawned.
#[derive(Component)]
pub struct HasViewModel;

/// Spawns the default first-person weapon (v_knife.mdl) attached as a child to the player camera.
pub fn spawn_viewmodel_system(
    mut commands: Commands,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    camera_query: Query<
        Entity,
        (
            With<Camera3d>,
            With<PlayerController>,
            Without<HasViewModel>,
        ),
    >,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for cam_entity in &camera_query {
        const KNIFE_PATH: &str = "models/v_knife.mdl";
        let Some(model_data) =
            model_store.get_or_load(KNIFE_PATH, &search_paths, &mut images, &mut materials)
        else {
            tracing::warn!("Failed to load ViewModel v_knife.mdl from search paths");
            continue;
        };

        // Find idle sequence (sequence 0 in CS 1.6 v_knife)
        let idle_fps = model_data
            .model
            .rig
            .sequences
            .first()
            .map_or(16.0, |s| s.fps);
        let mut anim_component = StudioAnimatedModel::new(KNIFE_PATH, true);
        anim_component.set_sequence(0, idle_fps, true);

        // Spawn ViewModel root entity as a child of the camera (mirrored X for authentic CS 1.6 cl_righthand 1)
        let vm_root = commands
            .spawn((
                ViewModelWeapon::default(),
                Transform::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
                Visibility::default(),
            ))
            .id();

        // Spawn child entities for each submesh posed at frame 0 (+ NoFrustumCulling prevents near-plane cull on knife)
        anim_component.mesh_entities = crate::studio::store::spawn_model_submesh_children(
            &mut commands,
            vm_root,
            &model_data,
            0,
            true,
            Some(VIEWMODEL_RENDER_LAYER),
            &mut meshes,
        );
        commands.entity(vm_root).insert(anim_component);
        commands.entity(cam_entity).add_child(vm_root);

        // Spawn depth-cleared ViewModel overlay camera (order: 1) so first-person weapons render
        // cleanly over walls (`glDepthRange` parity).
        let vm_cam = commands
            .spawn((
                Camera3d::default(),
                Camera {
                    order: 1,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                Projection::Perspective(PerspectiveProjection {
                    fov: 74.0f32.to_radians(),
                    near: 0.5,
                    far: 256.0,
                    ..default()
                }),
                RenderLayers::layer(VIEWMODEL_RENDER_LAYER),
            ))
            .id();
        commands.entity(cam_entity).add_child(vm_cam);

        // Soft overlay fill light on Layer 1 so viewmodel speculars remain crisp
        let vm_light = commands
            .spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.98, 0.95),
                    intensity: 45_000.0,
                    range: 200.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_xyz(8.0, 12.0, -10.0),
                RenderLayers::layer(VIEWMODEL_RENDER_LAYER),
            ))
            .id();
        commands.entity(cam_entity).add_child(vm_light);

        commands.entity(cam_entity).insert(HasViewModel);

        tracing::info!(
            submeshes = model_data.submeshes.len(),
            "Attached first-person Knife ViewModel to player camera"
        );
    }
}

/// Ray-AABB slab intersection test returning distance `t` along `dir` if hit.
fn ray_intersects_aabb(
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    bmin: Vec3,
    bmax: Vec3,
) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = max_dist;

    for axis in 0..3 {
        let (o, d, min_v, max_v) = match axis {
            0 => (origin.x, dir.x, bmin.x, bmax.x),
            1 => (origin.y, dir.y, bmin.y, bmax.y),
            _ => (origin.z, dir.z, bmin.z, bmax.z),
        };

        if d.abs() < 1e-6 {
            if o < min_v || o > max_v {
                return None;
            }
        } else {
            let inv_d = 1.0 / d;
            let mut t1 = (min_v - o) * inv_d;
            let mut t2 = (max_v - o) * inv_d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }

    if tmin <= max_dist { Some(tmin) } else { None }
}

/// Maximum forward trace reach for primary LMB knife slash (GoldSrc `CKnife::Swing`: 48u).
const KNIFE_SWING_DISTANCE: f32 = 48.0;

/// Maximum forward trace reach for secondary RMB knife stab (GoldSrc `CKnife::Stab`: 32u).
const KNIFE_STAB_DISTANCE: f32 = 32.0;

/// Bounding hull half-extents expansion for authentic GoldSrc knife swept hull (UTIL_TraceHull).
const KNIFE_HULL_EXPAND: Vec3 = Vec3::splat(16.0);

/// Authentic flesh impact sound effects for knife swings.
const KNIFE_FLESH_HITS: [&str; 4] = [
    "weapons/knife_hit1.wav",
    "weapons/knife_hit2.wav",
    "weapons/knife_hit3.wav",
    "weapons/knife_hit4.wav",
];

/// Handles player attack inputs (LMB slash, RMB stab) and audio triggers with forward ray tracing.
#[derive(Debug, Clone, Copy)]
enum HitTarget {
    Breakable(Entity, crate::entity::BreakableMaterial),
    Hostage(Entity),
    Flesh,
    Wall,
    Miss,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct KnifeVfxParams<'w> {
    pub sprite_store: ResMut<'w, crate::sprite::SpriteStore>,
    pub images: ResMut<'w, Assets<Image>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct KnifeAudioParams<'w> {
    pub search_paths: Res<'w, GameSearchPaths>,
    pub bank: ResMut<'w, GoldSrcAudioBank>,
    pub sources: ResMut<'w, Assets<AudioSource>>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct KnifeContextParams<'w> {
    pub time: Res<'w, Time>,
    pub mouse_button: Res<'w, ButtonInput<MouseButton>>,
    pub model_store: Res<'w, StudioModelStore>,
    pub collision_res: Res<'w, LevelCollisionResource>,
    pub pending_sel: Option<Res<'w, crate::combat::weapon::PendingWeaponSelection>>,
    pub console: Option<Res<'w, crate::hud::text_channels::ConsoleOverlayState>>,
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn viewmodel_input_system(
    mut commands: Commands,
    mut audio: KnifeAudioParams,
    mut vfx: KnifeVfxParams,
    ctx: KnifeContextParams,
    camera_query: Query<&GlobalTransform, (With<Camera3d>, With<PlayerController>)>,
    entity_query: Query<
        (
            Entity,
            &GlobalTransform,
            &EntityCollisionBounds,
            &WorldStudioEntity,
        ),
        Without<crate::studio::world_entity::HostageEntity>,
    >,
    mut breakable_query: Query<(Entity, &mut crate::entity::BreakableEntity)>,
    mut hostage_query: Query<
        (
            Entity,
            &mut crate::studio::world_entity::HostageEntity,
            &mut StudioAnimatedModel,
            &mut EntityCollisionBounds,
            &GlobalTransform,
        ),
        Without<ViewModelWeapon>,
    >,
    mut fire_events: EventWriter<crate::entity::FireTargetEvent>,
    mut break_events: EventWriter<crate::entity::BreakEntityEvent>,
    active_weapon_query: Query<&crate::combat::ActiveWeaponState>,
    mut player_ctrl_query: Query<&mut PlayerController>,
    mut vm_query: Query<(&mut ViewModelWeapon, &mut StudioAnimatedModel)>,
) {
    let dt = ctx.time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    if ctx.console.as_ref().is_some_and(|c| c.open)
        || ctx
            .pending_sel
            .as_ref()
            .is_some_and(|p| p.open || p.consume_next_attack)
    {
        return;
    }

    if let Ok(state) = active_weapon_query.get_single()
        && (state.id != crate::combat::WeaponId::Knife
            || state.next_primary_attack > 0.0
            || state.next_secondary_attack > 0.0)
    {
        return;
    }

    let Some(knife_data) = ctx.model_store.models.get("models/v_knife.mdl") else {
        return;
    };
    let sequences = &knife_data.model.rig.sequences;

    for (mut vm, mut anim) in &mut vm_query {
        if vm.attack_cooldown > 0.0 {
            vm.attack_cooldown -= dt;
        }

        // Check if current non-looping attack has completed
        if !anim.is_looping
            && let Some(seq) = sequences.get(anim.sequence_index)
        {
            let max_f = (seq.bone_matrices.len().saturating_sub(1)) as f32;
            if anim.frame_progress >= max_f {
                // Transition back to idle
                let idle_fps = sequences.first().map_or(16.0, |s| s.fps);
                anim.set_sequence(0, idle_fps, true);
            }
        }

        // Only allow new attack if cooldown expired
        if vm.attack_cooldown > 0.0 {
            continue;
        }

        let lmb = ctx.mouse_button.pressed(MouseButton::Left);
        let rmb = ctx.mouse_button.pressed(MouseButton::Right);
        if !lmb && !rmb {
            continue;
        }

        let reach = if lmb {
            KNIFE_SWING_DISTANCE
        } else {
            KNIFE_STAB_DISTANCE
        };

        // Raytrace from camera forward vector (LMB: 48u slash, RMB: 32u heavy stab)
        let (cam_pos, cam_forward) = camera_query
            .get_single()
            .map_or((Vec3::ZERO, Vec3::NEG_Z), |cam_t| {
                (cam_t.translation(), cam_t.forward().as_vec3())
            });

        // 1. Trace against func_breakable entities (bottles, chickens, radio) with ±16u hull
        let mut closest_breakable: Option<(Entity, f32, crate::entity::BreakableMaterial)> = None;
        for (b_ent, b) in &breakable_query {
            if b.is_broken {
                continue;
            }
            if let Some(t) = ray_intersects_aabb(
                cam_pos,
                cam_forward,
                reach,
                b.bmin - KNIFE_HULL_EXPAND,
                b.bmax + KNIFE_HULL_EXPAND,
            ) && closest_breakable.as_ref().is_none_or(|(_, d, _)| t < *d)
            {
                closest_breakable = Some((b_ent, t, b.material));
            }
        }

        // 2. Trace against studio world entities (hostages, chickens, bottles)
        let mut closest_entity_dist: Option<f32> = None;
        let mut closest_hostage_ent: Option<Entity> = None;

        for (h_ent, _, _, bounds, h_t) in &hostage_query {
            let ep = h_t.translation();
            let bmin = ep + bounds.mins - KNIFE_HULL_EXPAND;
            let bmax = ep + bounds.maxs + KNIFE_HULL_EXPAND;
            if let Some(t) = ray_intersects_aabb(cam_pos, cam_forward, reach, bmin, bmax)
                && closest_entity_dist.is_none_or(|cd| t < cd)
            {
                closest_entity_dist = Some(t);
                closest_hostage_ent = Some(h_ent);
            }
        }

        for (_ent, ent_t, bounds, studio_ent) in &entity_query {
            let ep = ent_t.translation();
            let bmin = ep + bounds.mins - KNIFE_HULL_EXPAND;
            let bmax = ep + bounds.maxs + KNIFE_HULL_EXPAND;
            if let Some(t) = ray_intersects_aabb(cam_pos, cam_forward, reach, bmin, bmax) {
                if studio_ent.is_solid {
                    if closest_entity_dist.is_none_or(|cd| t < cd) {
                        closest_entity_dist = Some(t);
                    }
                } else if !studio_ent.targetname.is_empty() {
                    // Route hit on visual studio entity (e.g. chicken, bottle) to any overlapping breakable
                    for (b_ent, b) in &breakable_query {
                        if !b.is_broken
                            && ((b.bmin + b.bmax) * 0.5).distance(ep) < 64.0
                            && closest_breakable.as_ref().is_none_or(|(_, d, _)| t < *d)
                        {
                            closest_breakable = Some((b_ent, t, b.material));
                            break;
                        }
                    }
                }
            }
        }

        // 3. Trace against world BSP collision geometry
        let mut wall_hit_dist: Option<f32> = None;
        if let Some(ref col_model) = ctx.collision_res.model {
            let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
            let gs_start = [cam_pos.x, -cam_pos.z, cam_pos.y];
            let ray_end = cam_pos + cam_forward * reach;
            let gs_end = [ray_end.x, -ray_end.z, ray_end.y];

            let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0); // 0 is HULL_POINT
            if tr.fraction < 1.0 {
                wall_hit_dist = Some(tr.fraction * reach);
            }
        }

        // Determine closest struck object

        let mut hit = HitTarget::Miss;
        let mut min_t = reach + 1.0;

        if let Some((b_ent, b_t, mat)) = closest_breakable
            && b_t < min_t
        {
            min_t = b_t;
            hit = HitTarget::Breakable(b_ent, mat);
        }
        if let Some(e_t) = closest_entity_dist
            && e_t < min_t
        {
            min_t = e_t;
            if let Some(h_ent) = closest_hostage_ent {
                hit = HitTarget::Hostage(h_ent);
            } else {
                hit = HitTarget::Flesh;
            }
        }
        if let Some(w_t) = wall_hit_dist
            && w_t < min_t
        {
            hit = HitTarget::Wall;
        }

        let now = ctx.time.elapsed_secs();
        let damage = if lmb {
            if now - vm.last_swing_time < 0.85 {
                20.0
            } else {
                15.0
            }
        } else if let HitTarget::Hostage(h_ent) = hit
            && let Ok((_, _, _, _, h_t)) = hostage_query.get(h_ent)
        {
            let h_fwd = h_t.right().as_vec3();
            let h_xz = Vec2::new(h_fwd.x, h_fwd.z).normalize_or_zero();
            let p_xz = Vec2::new(cam_forward.x, cam_forward.z).normalize_or_zero();
            if p_xz.dot(h_xz) > 0.8 { 195.0 } else { 65.0 }
        } else {
            65.0
        };

        if lmb {
            // Primary Attack: alternating midslash1 (seq 6) and midslash2 (seq 7)
            vm.last_swing = 1 - vm.last_swing;
            vm.last_swing_time = now;
            let target_seq = if vm.last_swing == 0 { 6 } else { 7 };
            anim.set_sequence(target_seq, 75.0, false);
            vm.attack_cooldown = if matches!(hit, HitTarget::Miss) {
                0.35
            } else {
                0.40
            };

            let sound_name = match hit {
                HitTarget::Breakable(b_ent, mat) => {
                    if let Ok((_, mut breakable)) = breakable_query.get_mut(b_ent) {
                        breakable.health -= damage;
                        if breakable.health <= 0.0 && !breakable.is_broken {
                            breakable.is_broken = true;
                            let center = (breakable.bmin + breakable.bmax) * 0.5;
                            break_events.send(crate::entity::BreakEntityEvent {
                                origin: center,
                                material: mat,
                                model: breakable.model.clone(),
                                explodemagnitude: breakable.explodemagnitude,
                            });
                            fire_events.send(crate::entity::FireTargetEvent {
                                target: breakable.target.clone(),
                            });
                            match mat {
                                crate::entity::BreakableMaterial::Glass => "debris/bustglass1.wav",
                                crate::entity::BreakableMaterial::Wood => "debris/bustcrate1.wav",
                                crate::entity::BreakableMaterial::Flesh => "debris/bustflesh1.wav",
                                _ => "debris/bustmetal1.wav",
                            }
                        } else {
                            match mat {
                                crate::entity::BreakableMaterial::Glass => "debris/glass1.wav",
                                crate::entity::BreakableMaterial::Wood => "debris/wood1.wav",
                                crate::entity::BreakableMaterial::Flesh => "weapons/knife_hit1.wav",
                                _ => "debris/metal1.wav",
                            }
                        }
                    } else {
                        "weapons/knife_hitwall1.wav"
                    }
                }
                HitTarget::Hostage(h_ent) => {
                    if let Ok((_, mut hostage, mut h_anim, mut bounds, h_t)) =
                        hostage_query.get_mut(h_ent)
                    {
                        crate::sprite::spawn_blood_sprite(
                            &mut commands,
                            h_t.translation() + Vec3::Y * 36.0,
                            &mut vfx.sprite_store,
                            &audio.search_paths,
                            &mut vfx.images,
                            &mut vfx.materials,
                            &mut vfx.meshes,
                        );
                        hostage.health -= damage;
                        if hostage.health > 0.0 {
                            h_anim.set_sequence(7, 30.0, false); // flinch
                            let pain_idx = ((ctx.time.elapsed_secs() * 100.0) as usize % 6) + 1;
                            let pain_snd = format!("hostage/hpain/hpain{pain_idx}.wav");
                            if let Some(h) = audio.bank.get_or_load_sound(
                                &pain_snd,
                                &audio.search_paths.dirs,
                                &mut audio.sources,
                            ) {
                                commands.spawn((
                                    AudioPlayer::new(h),
                                    PlaybackSettings::DESPAWN.with_spatial(true),
                                    Transform::from_translation(h_t.translation()),
                                ));
                            }
                        } else if !hostage.is_dead {
                            hostage.is_dead = true;
                            h_anim.set_sequence(31, 24.0, false); // diesimple
                            bounds.mins = Vec3::ZERO;
                            bounds.maxs = Vec3::ZERO;
                            if let Some(h) = audio.bank.get_or_load_sound(
                                "radio/hosdown.wav",
                                &audio.search_paths.dirs,
                                &mut audio.sources,
                            ) {
                                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                            }
                        }
                    }
                    let pick =
                        ((ctx.time.elapsed_secs() * 1000.0) as usize) % KNIFE_FLESH_HITS.len();
                    KNIFE_FLESH_HITS[pick]
                }
                HitTarget::Flesh => {
                    let pick =
                        ((ctx.time.elapsed_secs() * 1000.0) as usize) % KNIFE_FLESH_HITS.len();
                    KNIFE_FLESH_HITS[pick]
                }
                HitTarget::Wall => "weapons/knife_hitwall1.wav",
                HitTarget::Miss => {
                    if vm.last_swing == 0 {
                        "weapons/knife_slash1.wav"
                    } else {
                        "weapons/knife_slash2.wav"
                    }
                }
            };

            if let Some(handle) = audio.bank.get_or_load_sound(
                sound_name,
                &audio.search_paths.dirs,
                &mut audio.sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        } else if rmb {
            // Secondary Attack: heavy stab (seq 4 if hit, seq 5 if miss)
            let (target_seq, stab_fps, sound_name) = match hit {
                HitTarget::Breakable(b_ent, mat) => {
                    let fps = sequences.get(4).map_or(35.0, |s| s.fps);
                    let snd = if let Ok((_, mut breakable)) = breakable_query.get_mut(b_ent) {
                        breakable.health -= damage;
                        if breakable.health <= 0.0 && !breakable.is_broken {
                            breakable.is_broken = true;
                            let center = (breakable.bmin + breakable.bmax) * 0.5;
                            break_events.send(crate::entity::BreakEntityEvent {
                                origin: center,
                                material: mat,
                                model: breakable.model.clone(),
                                explodemagnitude: breakable.explodemagnitude,
                            });
                            fire_events.send(crate::entity::FireTargetEvent {
                                target: breakable.target.clone(),
                            });
                            match mat {
                                crate::entity::BreakableMaterial::Glass => "debris/bustglass2.wav",
                                crate::entity::BreakableMaterial::Wood => "debris/bustcrate2.wav",
                                crate::entity::BreakableMaterial::Flesh => "debris/bustflesh1.wav",
                                _ => "debris/bustmetal1.wav",
                            }
                        } else {
                            match mat {
                                crate::entity::BreakableMaterial::Glass => "debris/glass2.wav",
                                crate::entity::BreakableMaterial::Wood => "debris/wood2.wav",
                                crate::entity::BreakableMaterial::Flesh => "weapons/knife_hit2.wav",
                                _ => "debris/metal2.wav",
                            }
                        }
                    } else {
                        "weapons/knife_hitwall1.wav"
                    };
                    (4, fps, snd)
                }
                HitTarget::Hostage(h_ent) => {
                    let fps = sequences.get(4).map_or(35.0, |s| s.fps);
                    if let Ok((_, mut hostage, mut h_anim, mut bounds, h_t)) =
                        hostage_query.get_mut(h_ent)
                    {
                        crate::sprite::spawn_blood_sprite(
                            &mut commands,
                            h_t.translation() + Vec3::Y * 36.0,
                            &mut vfx.sprite_store,
                            &audio.search_paths,
                            &mut vfx.images,
                            &mut vfx.materials,
                            &mut vfx.meshes,
                        );
                        hostage.health -= damage;
                        if hostage.health > 0.0 {
                            h_anim.set_sequence(7, 30.0, false); // flinch
                            let pain_idx = ((ctx.time.elapsed_secs() * 100.0) as usize % 6) + 1;
                            let pain_snd = format!("hostage/hpain/hpain{pain_idx}.wav");
                            if let Some(h) = audio.bank.get_or_load_sound(
                                &pain_snd,
                                &audio.search_paths.dirs,
                                &mut audio.sources,
                            ) {
                                commands.spawn((
                                    AudioPlayer::new(h),
                                    PlaybackSettings::DESPAWN.with_spatial(true),
                                    Transform::from_translation(h_t.translation()),
                                ));
                            }
                        } else if !hostage.is_dead {
                            hostage.is_dead = true;
                            h_anim.set_sequence(31, 24.0, false); // diesimple
                            bounds.mins = Vec3::ZERO;
                            bounds.maxs = Vec3::ZERO;
                            if let Some(h) = audio.bank.get_or_load_sound(
                                "radio/hosdown.wav",
                                &audio.search_paths.dirs,
                                &mut audio.sources,
                            ) {
                                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                            }
                        }
                    }
                    (4, fps, "weapons/knife_stab.wav")
                }
                HitTarget::Flesh => {
                    let fps = sequences.get(4).map_or(35.0, |s| s.fps);
                    (4, fps, "weapons/knife_stab.wav")
                }
                HitTarget::Wall => {
                    let fps = sequences.get(4).map_or(35.0, |s| s.fps);
                    (4, fps, "weapons/knife_hitwall1.wav")
                }
                HitTarget::Miss => {
                    let fps = sequences.get(5).map_or(40.0, |s| s.fps);
                    (5, fps, "weapons/knife_slash1.wav")
                }
            };

            anim.set_sequence(target_seq, stab_fps, false);
            vm.attack_cooldown = if matches!(hit, HitTarget::Miss) {
                1.0
            } else {
                1.1
            };

            if let Ok(mut pc) = player_ctrl_query.get_single_mut() {
                pc.trigger_third_person_shoot(0.35);
            }

            if let Some(handle) = audio.bank.get_or_load_sound(
                sound_name,
                &audio.search_paths.dirs,
                &mut audio.sources,
            ) {
                commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
            }
        }
    }
}

/// Applies authentic weapon bobbing, mouse sway lag, viewmodel punch kick, handedness (`cl_righthand` + knife parity),
/// and manages first/third person visibility.
pub fn viewmodel_bob_and_sway_system(
    time: Res<Time>,
    perspective: Option<Res<crate::player::CameraPerspective>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    player_query: Query<(
        &PlayerController,
        Option<&crate::combat::ActiveWeaponState>,
        Option<&crate::combat::PlayerRecoil>,
    )>,
    mut vm_query: Query<(&mut ViewModelWeapon, &mut Transform, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok((player, active_weapon, recoil)) = player_query.get_single() else {
        return;
    };
    let speed = player.sim.current.horizontal_speed();
    let is_knife = active_weapon.is_some_and(|w| w.id == crate::combat::WeaponId::Knife);
    let (punch_pitch, punch_yaw) = recoil.map_or((0.0, 0.0), |r| {
        (r.punch_pitch.to_radians(), r.punch_yaw.to_radians())
    });

    for (mut vm, mut transform, mut visibility) in &mut vm_query {
        // Hide viewmodel in Third-Person mode or while sniper scope overlay is active
        let scoped_sniper =
            active_weapon.is_some_and(|w| w.id.is_sniper_scope() && w.current_fov <= 40.0);
        if scoped_sniper
            || perspective
                .as_ref()
                .is_some_and(|pers| **pers == crate::player::CameraPerspective::ThirdPerson)
        {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Inherited;

        // 1. Movement Bobbing
        if speed > 10.0 && player.sim.current.on_ground {
            let bob_freq = (speed / 120.0).clamp(2.0, 7.0);
            vm.bob_phase += dt * bob_freq * std::f32::consts::PI;

            let bob_x = (vm.bob_phase * 0.5).sin() * 0.35;
            let bob_y = (vm.bob_phase).sin().abs() * -0.25;
            transform.translation.x = bob_x;
            transform.translation.y = bob_y;
        } else {
            // Smoothly return to rest position
            transform.translation.x *= 1.0 - dt * 10.0;
            transform.translation.y *= 1.0 - dt * 10.0;
        }

        // 2. Mouse Sway Lag + CS 1.6 V_CalcGunAngle recoil feedback
        let target_sway_x = (-mouse_motion.delta.x * 0.0008).clamp(-0.05, 0.05);
        let target_sway_y = (mouse_motion.delta.y * 0.0008).clamp(-0.05, 0.05);

        vm.current_sway.x = vm.current_sway.x.lerp(target_sway_x, dt * 12.0);
        vm.current_sway.y = vm.current_sway.y.lerp(target_sway_y, dt * 12.0);

        transform.rotation = Quat::from_rotation_y(vm.current_sway.x + punch_yaw)
            * Quat::from_rotation_x(vm.current_sway.y + punch_pitch * 0.25);

        // CS 1.6 StudioModelRenderer::StudioDrawModel parity (`g_bHoldingKnife` inverts `cl_righthand`):
        // Gun models are authored left-handed and mirrored to right-hand (-X),
        // whereas v_knife.mdl is authored right-handed and renders unmirrored (+X) in right hand.
        transform.scale = if is_knife {
            Vec3::ONE
        } else {
            Vec3::new(-1.0, 1.0, 1.0)
        };
    }
}

/// True Hor+ Widescreen FOV system for the player camera (supports 90°, 55°, 40°, 15°, 10° weapon zoom).
pub fn widescreen_horplus_fov_system(
    active_weapon: Query<&crate::combat::ActiveWeaponState>,
    mut query: Query<&mut Projection, With<Camera3d>>,
) {
    let target_hfov_deg = active_weapon
        .get_single()
        .map_or(90.0_f32, |w| w.current_fov)
        .clamp(5.0, 120.0);
    // Hor+ vertical FOV keeping 4:3 GoldSrc horizontal framing invariant:
    // tan(hfov / 2) * 0.75 -> vfov = 2 * atan(tan(hfov / 2) * 0.75)
    let target_vfov_rad = 2.0 * ((target_hfov_deg * 0.5).to_radians().tan() * 0.75).atan();

    for mut projection in &mut query {
        if let Projection::Perspective(ref mut perspective) = *projection
            && (perspective.fov - target_vfov_rad).abs() > 1e-4
        {
            perspective.fov = target_vfov_rad;
        }
    }
}
