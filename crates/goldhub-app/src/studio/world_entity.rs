//! World entity spawner (hostages, chickens, decorative props) from BSP entity lumps.

use bevy::prelude::*;
use goldhub_physics::WorldTrace;

use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::player::LevelCollisionResource;
use super::animation::StudioAnimatedModel;
use super::store::{StudioModelStore, create_initial_bevy_mesh};

/// Resource holding the raw text entity lump from the active BSP level.
#[derive(Resource, Default, Clone)]
pub struct BspEntitiesLump(pub Vec<u8>);

/// Component tagging world StudioModel entities (hostages, chickens, props).
#[derive(Component, Debug, Clone)]
pub struct WorldStudioEntity {
    pub classname: String,
    pub model_path: String,
    pub targetname: String,
    pub is_solid: bool,
}

/// Bounding box component in Bevy coordinates for physical interaction and ray intersection.
#[derive(Component, Debug, Clone, Copy)]
pub struct EntityCollisionBounds {
    pub mins: Vec3,
    pub maxs: Vec3,
}

/// Hostage state matching GoldSrc enum state { FOLLOW, STAND, DUCK, SCARED, IDLE, RESCUED, DEAD }.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostageState {
    #[default]
    Stand,
    Follow,
    Rescued,
    Dead,
}

/// Interactive hostage entity component (`hostage_entity` / `monster_scientist`).
#[derive(Component, Debug, Clone)]
pub struct HostageEntity {
    pub health: f32,
    pub max_health: f32,
    pub is_dead: bool,
    pub pain_cooldown: f32,
    pub state: HostageState,
    pub leader: Option<Entity>,
    pub velocity: Vec3,
    pub next_use_time: f32,
    pub home_pos: Vec3,
}

/// Spawns interactive 3D studio entities parsed from the BSP map's entity lump.
#[allow(clippy::too_many_lines, clippy::too_many_arguments, clippy::cognitive_complexity)]
pub fn spawn_map_studio_entities(
    mut commands: Commands,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    entities_res: Option<Res<BspEntitiesLump>>,
    collision_res: Res<LevelCollisionResource>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut spawned_any = false;

    if let Some(ref entities_lump) = entities_res
        && let Ok(entities_str) = std::str::from_utf8(&entities_lump.0)
    {
        for block in entities_str.split('}') {
            let mut classname = "";
            let mut origin_str = "";
            let mut angle_str = "";
            let mut model_str = "";
            let mut targetname_str = "";

            for line in block.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('"') {
                    let parts: Vec<&str> = trimmed.split('"').collect();
                    if parts.len() >= 4 {
                        match parts[1] {
                            "classname" => classname = parts[3],
                            "origin" => origin_str = parts[3],
                            "angle" | "angles" => angle_str = parts[3],
                            "model" => model_str = parts[3],
                            "targetname" => targetname_str = parts[3],
                            _ => {}
                        }
                    }
                }
            }

            if classname.is_empty() || origin_str.is_empty() {
                continue;
            }

            // Parse GoldSrc origin
            let mut coords = origin_str.split_whitespace();
            let Some((Ok(gx), Ok(gy), Ok(gz))) = coords
                .next()
                .zip(coords.next())
                .zip(coords.next())
                .map(|((x, y), z)| (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>()))
            else {
                continue;
            };

            // Parse GoldSrc yaw angle
            let mut gyaw = 0.0f32;
            if !angle_str.is_empty() {
                let mut ap = angle_str.split_whitespace();
                if let Some(first) = ap.next() {
                    if let Some(second) = ap.next() {
                        if let Ok(y) = second.parse::<f32>() {
                            gyaw = y;
                        }
                    } else if let Ok(y) = first.parse::<f32>() {
                        gyaw = y;
                    }
                }
            }

            // Determine model path based on entity classname and map-specified model
            let target_model = if classname == "hostage_entity" {
                // In authentic CS 1.6 (ReGameDLL CHostage::Spawn), hostages canonically use models/hostage.mdl,
                // overriding legacy Half-Life "models/scientist.mdl" strings stored in early BSP entity lumps.
                "models/hostage.mdl"
            } else if classname == "monster_scientist" {
                if model_str.is_empty() {
                    "models/scientist.mdl"
                } else {
                    model_str
                }
            } else if (classname == "cycler_sprite" || classname == "env_model")
                && std::path::Path::new(model_str)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("mdl"))
            {
                model_str
            } else if classname == "monster_chick" {
                "models/chick.mdl"
            } else {
                continue;
            };

            let is_hostage = classname == "hostage_entity" || classname == "monster_scientist";
            let is_solid = is_hostage;

            // Authentic GoldSrc DROP_TO_FLOOR:
            // In GoldSrc engine (ReGameDLL), DROP_TO_FLOOR is only executed by monsters/NPCs (CHostage, CBaseMonster).
            // Static visual props (cycler_sprite, env_model) are placed at exact map author coordinates (e.g. on tables).
            let mut floor_z = gz;
            if is_hostage
                && let Some(ref col_model) = collision_res.model
            {
                use goldhub_physics::WorldTrace;
                let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
                for start_offset in [16.0f32, 8.0, 24.0, 0.0, 32.0] {
                    let start = [gx, gy, gz + start_offset];
                    let end = [gx, gy, gz - 256.0];
                    let drop_tr = bsp_world.trace_player_hull(start, end, 0);
                    if !drop_tr.startsolid && drop_tr.fraction < 1.0 {
                        floor_z = drop_tr.endpos[2];
                        break;
                    }
                }
            }

            // Coordinate transformation: Bevy X = GS X, Bevy Y = GS Z, Bevy Z = -GS Y
            let bevy_pos = Vec3::new(gx, floor_z, -gy);
            // GoldSrc yaw 0 (+X) maps directly to Bevy yaw 0 (+X); 90 (+Y) to Bevy 90 (-Z)
            let bevy_yaw = gyaw.to_radians();
            let bevy_rot = Quat::from_rotation_y(bevy_yaw);

            if spawn_studio_entity(
                &mut commands,
                &mut model_store,
                &search_paths,
                &mut images,
                &mut materials,
                &mut meshes,
                target_model,
                bevy_pos,
                bevy_rot,
                classname,
                targetname_str,
                is_solid,
            ) {
                spawned_any = true;
            }
        }
    }

    // If no world model entities exist on this map (e.g. de_dust2, test map),
    // spawn ambient demonstration entities so hostages and chickens are visible!
    if !spawned_any {
        tracing::info!("No model entities in map lump; spawning ambient test hostage and chicken");

        spawn_studio_entity(
            &mut commands,
            &mut model_store,
            &search_paths,
            &mut images,
            &mut materials,
            &mut meshes,
            "models/scientist.mdl",
            Vec3::new(120.0, 36.0, -80.0),
            Quat::from_rotation_y(-45.0_f32.to_radians()),
            "hostage_entity",
            "demo_hostage",
            true,
        );

        spawn_studio_entity(
            &mut commands,
            &mut model_store,
            &search_paths,
            &mut images,
            &mut materials,
            &mut meshes,
            "models/chick.mdl",
            Vec3::new(60.0, 36.0, -100.0),
            Quat::from_rotation_y(30.0_f32.to_radians()),
            "cycler_sprite",
            "demo_chicken",
            false,
        );
    }
}

/// Spawns a single 3D StudioModel entity in world space.
#[allow(clippy::too_many_arguments)]
pub fn spawn_studio_entity(
    commands: &mut Commands,
    model_store: &mut StudioModelStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    model_path: &str,
    position: Vec3,
    rotation: Quat,
    classname: &str,
    targetname: &str,
    is_solid: bool,
) -> bool {
    let Some(model_data) = model_store.get_or_load(model_path, search_paths, images, materials)
    else {
        return false;
    };

    // Determine initial animation sequence
    let mut initial_seq = 0;
    let mut seq_fps = 30.0;
    let sequences = &model_data.model.rig.sequences;

    // For hostages/scientists, prefer "idle1" or "idle" over walk
    if model_path.contains("hostage") || model_path.contains("scientist") {
        if let Some(idx) = sequences.iter().position(|s| s.name == "idle1") {
            initial_seq = idx;
        } else if let Some(idx) = sequences.iter().position(|s| s.name.contains("idle")) {
            initial_seq = idx;
        }
    }
    if let Some(s) = sequences.get(initial_seq) {
        seq_fps = s.fps;
    }

    let mut anim_component = StudioAnimatedModel::new(model_path, false);
    anim_component.set_sequence(initial_seq, seq_fps, true);

    // Compute dynamic bounding box from rest-pose vertices
    let mut min_pt = Vec3::splat(f32::INFINITY);
    let mut max_pt = Vec3::splat(f32::NEG_INFINITY);
    for sub in &model_data.submeshes {
        for v in &sub.rest_vertices {
            // Bevy coordinates: X_bevy = X_gs, Y_bevy = Z_gs, Z_bevy = -Y_gs
            let p = Vec3::new(v.position[0], v.position[2], -v.position[1]);
            min_pt = min_pt.min(p);
            max_pt = max_pt.max(p);
        }
    }

    let bounds = if is_solid {
        // Standard GoldSrc player/hostage collision hull 0
        EntityCollisionBounds {
            mins: Vec3::new(-16.0, 0.0, -16.0),
            maxs: Vec3::new(16.0, 72.0, 16.0),
        }
    } else if min_pt.x.is_finite() && max_pt.x.is_finite() {
        EntityCollisionBounds {
            mins: min_pt,
            maxs: max_pt,
        }
    } else {
        EntityCollisionBounds {
            mins: Vec3::new(-8.0, 0.0, -8.0),
            maxs: Vec3::new(8.0, 16.0, 8.0),
        }
    };

    // Parent entity holding transform, collision bounds, and animation state
    let parent_entity = commands
        .spawn((
            WorldStudioEntity {
                classname: classname.to_string(),
                model_path: model_path.to_string(),
                targetname: targetname.to_string(),
                is_solid,
            },
            bounds,
            Transform::from_translation(position).with_rotation(rotation),
            Visibility::default(),
        ))
        .id();

    // Spawn child entities for each submesh
    let mut mesh_children = Vec::with_capacity(model_data.submeshes.len());
    for (sub_idx, template) in model_data.submeshes.iter().enumerate() {
        let bevy_mesh = create_initial_bevy_mesh(template, false);
        let mesh_handle = meshes.add(bevy_mesh);

        let child_entity = commands
            .spawn((
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(template.material.clone()),
                Transform::IDENTITY,
                bevy::render::view::NoFrustumCulling,
            ))
            .id();

        commands.entity(parent_entity).add_child(child_entity);
        mesh_children.push((child_entity, mesh_handle, sub_idx));
    }

    anim_component.mesh_entities = mesh_children;
    commands.entity(parent_entity).insert(anim_component);

    let is_hostage = classname == "hostage_entity" || classname == "monster_scientist";
    if is_hostage {
        commands.entity(parent_entity).insert(HostageEntity {
            health: 100.0,
            max_health: 100.0,
            is_dead: false,
            pain_cooldown: 0.0,
            state: HostageState::Stand,
            leader: None,
            velocity: Vec3::ZERO,
            next_use_time: 0.0,
            home_pos: position,
        });
    }

    tracing::info!(
        model = model_path,
        classname = classname,
        targetname = targetname,
        pos = ?position,
        "Spawned World 3D StudioModel Entity"
    );
    true
}

/// Upkeep tick and follow movement for hostages.
#[allow(clippy::too_many_arguments, clippy::type_complexity, clippy::too_many_lines)]
pub fn hostage_interaction_and_ai_system(
    mut commands: Commands,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    collision_res: Res<LevelCollisionResource>,
    model_store: Res<StudioModelStore>,
    player_query: Query<(Entity, &Transform, &crate::player::PlayerController)>,
    rescue_query: Query<&crate::entity::HostageRescueZoneEntity>,
    mut hostage_query: Query<(
        Entity,
        &mut HostageEntity,
        &mut Transform,
        &mut StudioAnimatedModel,
        &mut EntityCollisionBounds,
        &mut Visibility,
    ), Without<crate::player::PlayerController>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let Ok((player_ent, player_t, _)) = player_query.get_single() else {
        return;
    };
    let player_pos = player_t.translation;
    let player_fwd = player_t.forward().as_vec3();
    let use_pressed = keyboard.just_pressed(KeyCode::KeyE);

    for (h_ent, mut hostage, mut h_t, mut h_anim, mut bounds, mut vis) in &mut hostage_query {
        if hostage.is_dead || hostage.state == HostageState::Rescued {
            continue;
        }

        let h_pos = h_t.translation;
        let delta = player_pos - h_pos;
        let dist = delta.length();

        // 1. +use (E key) toggle between Stand and Follow
        if use_pressed && dist <= 80.0 {
            let dir_to_hostage = (h_pos - player_pos).normalize_or_zero();
            let dot = player_fwd.dot(dir_to_hostage);
            if dot >= 0.65 && hostage.next_use_time <= time.elapsed_secs() {
                hostage.next_use_time = time.elapsed_secs() + 1.0;

                if hostage.state == HostageState::Follow && hostage.leader == Some(player_ent) {
                    hostage.state = HostageState::Stand;
                    hostage.leader = None;
                    hostage.velocity = Vec3::ZERO;
                    tracing::info!(hostage = ?h_ent, "Hostage toggled to STAND");
                } else {
                    hostage.state = HostageState::Follow;
                    hostage.leader = Some(player_ent);
                    tracing::info!(hostage = ?h_ent, "Hostage toggled to FOLLOW player");
                }

                // Play follow sound hostage/hos1..5.wav
                let snd_idx = ((time.elapsed_secs() * 100.0) as usize % 5) + 1;
                let snd_name = format!("hostage/hos{snd_idx}.wav");
                if let Some(h) = audio_bank.get_or_load_sound(&snd_name, &search_paths.dirs, &mut audio_sources) {
                    commands.spawn((
                        AudioPlayer::new(h),
                        PlaybackSettings::DESPAWN.with_spatial(true),
                        Transform::from_translation(h_pos),
                    ));
                }
            }
        }

        // 2. Follow movement logic
        if hostage.state == HostageState::Follow && hostage.leader == Some(player_ent) {
            let h_flat = Vec3::new(h_pos.x, 0.0, h_pos.z);
            let p_flat = Vec3::new(player_pos.x, 0.0, player_pos.z);
            let flat_delta = p_flat - h_flat;
            let flat_dist = flat_delta.length();

            // ReGameDLL STOPPING DISTANCE: 80.0 units
            if flat_dist < 80.0 {
                hostage.velocity = Vec3::ZERO;
                // Switch to idle sequence
                if let Some(model_data) = model_store.models.get(&h_anim.model_id) {
                    let seqs = &model_data.model.rig.sequences;
                    if let Some(idle_idx) = seqs.iter().position(|s| s.name == "idle1")
                        && h_anim.sequence_index != idle_idx
                    {
                        h_anim.set_sequence(idle_idx, seqs[idle_idx].fps, true);
                    }
                }
            } else {
                let move_dir = flat_delta.normalize_or_zero();

                // ReGameDLL Piecewise Speed Equation:
                // >= 250 -> 400 u/s, >= 110 -> 300 u/s, < 110 -> dist u/s
                let speed = if flat_dist >= 250.0 {
                    400.0
                } else if flat_dist >= 110.0 {
                    300.0
                } else {
                    flat_dist.max(80.0)
                };

                hostage.velocity = move_dir * speed;
                h_t.translation += move_dir * speed * dt;

                // Face moving direction (GoldSrc model forward is -Z in Bevy coords, facing along yaw)
                let target_yaw = move_dir.x.atan2(-move_dir.z);
                h_t.rotation = Quat::from_rotation_y(target_yaw);

                // Snap feet to ground using BSP collision trace
                if let Some(ref col_model) = collision_res.model {
                    let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
                    let gs_start = [h_t.translation.x, -h_t.translation.z, h_t.translation.y + 16.0];
                    let gs_end = [h_t.translation.x, -h_t.translation.z, h_t.translation.y - 128.0];
                    let tr = bsp_world.trace_player_hull(gs_start, gs_end, 0);
                    if tr.fraction < 1.0 {
                        h_t.translation.y = tr.endpos[2];
                    }
                }

                // Animation sequence selection
                if let Some(model_data) = model_store.models.get(&h_anim.model_id) {
                    let seqs = &model_data.model.rig.sequences;
                    let target_seq_name = if speed > 160.0 { "run" } else { "walk" };
                    if let Some(seq_idx) = seqs.iter().position(|s| s.name == target_seq_name)
                        && h_anim.sequence_index != seq_idx
                    {
                        h_anim.set_sequence(seq_idx, seqs[seq_idx].fps, true);
                    }
                }
            }

            // 3. Rescue Zone Detection (func_hostage_rescue or info_hostage_rescue)
            for rescue_zone in &rescue_query {
                let inside = h_t.translation.x >= rescue_zone.bmin.x
                    && h_t.translation.x <= rescue_zone.bmax.x
                    && h_t.translation.y >= rescue_zone.bmin.y
                    && h_t.translation.y <= rescue_zone.bmax.y
                    && h_t.translation.z >= rescue_zone.bmin.z
                    && h_t.translation.z <= rescue_zone.bmax.z;

                if inside {
                    hostage.state = HostageState::Rescued;
                    *vis = Visibility::Hidden;
                    bounds.mins = Vec3::ZERO;
                    bounds.maxs = Vec3::ZERO;

                    if let Some(h) = audio_bank.get_or_load_sound("radio/rescued.wav", &search_paths.dirs, &mut audio_sources) {
                        commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
                    }
                    tracing::info!(hostage = ?h_ent, "Hostage successfully rescued in rescue zone!");
                    break;
                }
            }
        }
    }
}
