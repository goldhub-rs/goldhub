//! World entity spawner (hostages, chickens, decorative props) from BSP entity lumps.

use bevy::prelude::*;

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
}

/// Bounding box component in Bevy coordinates for physical interaction and ray intersection.
#[derive(Component, Debug, Clone, Copy)]
pub struct EntityCollisionBounds {
    pub mins: Vec3,
    pub maxs: Vec3,
}

/// Spawns interactive 3D studio entities parsed from the BSP map's entity lump.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn spawn_map_studio_entities(
    mut commands: Commands,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    entities_res: Option<Res<BspEntitiesLump>>,
    collision_res: Res<LevelCollisionResource>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
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
            let mut message_str = "";
            let mut spawnflags_str = "";

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
                            "message" => message_str = parts[3],
                            "spawnflags" => spawnflags_str = parts[3],
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

            // 1. Handle Ambient Sound Entities (e.g. cs_italy Opera Radio, cs_office ambience)
            if classname == "ambient_generic" && !message_str.is_empty() {
                let spawnflags = spawnflags_str.parse::<u32>().unwrap_or(0);
                // SF_AMBIENT_SOUND_START_OFF = 1
                if (spawnflags & 1) == 0 {
                    let bevy_pos = Vec3::new(gx, gz, -gy);
                    if let Some(handle) = audio_bank.get_or_load_sound(
                        message_str,
                        &search_paths.dirs,
                        &mut audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(handle),
                            PlaybackSettings::LOOP.with_spatial(true),
                            Transform::from_translation(bevy_pos),
                        ));
                        tracing::info!(
                            message = message_str,
                            pos = ?bevy_pos,
                            "Spawned active spatial ambient_generic radio audio"
                        );
                    }
                }
                continue;
            }

            // Parse GoldSrc yaw angle
            let mut gyaw = 0.0f32;
            if !angle_str.is_empty() {
                let mut ap = angle_str.split_whitespace();
                if let Some(first) = ap.next() {
                    // Check if 3-component "pitch yaw roll" or single "yaw"
                    if let Some(second) = ap.next() {
                        if let Ok(y) = second.parse::<f32>() {
                            gyaw = y;
                        }
                    } else if let Ok(y) = first.parse::<f32>() {
                        gyaw = y;
                    }
                }
            }

            // Determine model path based on entity classname
            let target_model = if classname == "hostage_entity" || classname == "monster_scientist" {
                "models/hostage.mdl"
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

            // Authentic GoldSrc DROP_TO_FLOOR: trace downwards 256 units against BSP collision to snap to floor
            let mut floor_z = gz;
            if let Some(ref col_model) = collision_res.model {
                use goldhub_physics::WorldTrace;
                let bsp_world = goldhub_physics::BspWorldCollision::new(col_model);
                let drop_tr = bsp_world.trace_player_hull([gx, gy, gz], [gx, gy, gz - 256.0], 0);
                if drop_tr.fraction < 1.0 && !drop_tr.startsolid {
                    floor_z = drop_tr.endpos[2];
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
            ) {
                spawned_any = true;
            }
        }
    }

    // If no world model entities exist on this map (e.g. de_dust2, test map),
    // spawn ambient demonstration entities so hostages and chickens are visible!
    if !spawned_any {
        tracing::info!("No model entities in map lump; spawning ambient test hostage and chicken");

        // Spawn test hostage at [120.0, 36.0, -80.0]
        spawn_studio_entity(
            &mut commands,
            &mut model_store,
            &search_paths,
            &mut images,
            &mut materials,
            &mut meshes,
            "models/hostage.mdl",
            Vec3::new(120.0, 36.0, -80.0),
            Quat::from_rotation_y(-45.0_f32.to_radians()),
            "hostage_entity",
        );

        // Spawn test chicken at [60.0, 36.0, -100.0]
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
) -> bool {
    let Some(model_data) = model_store.get_or_load(model_path, search_paths, images, materials)
    else {
        return false;
    };

    // Determine initial animation sequence
    let mut initial_seq = 0;
    let mut seq_fps = 30.0;
    let sequences = &model_data.model.rig.sequences;

    // For hostages, prefer "idle1" or "idle3" over walk
    if model_path.contains("hostage") {
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

    let bounds = if model_path.contains("hostage") || classname.contains("hostage") {
        EntityCollisionBounds {
            mins: Vec3::new(-16.0, 0.0, -16.0),
            maxs: Vec3::new(16.0, 72.0, 16.0),
        }
    } else if model_path.contains("chick") {
        EntityCollisionBounds {
            mins: Vec3::new(-8.0, 0.0, -8.0),
            maxs: Vec3::new(8.0, 16.0, 8.0),
        }
    } else {
        EntityCollisionBounds {
            mins: Vec3::new(-16.0, 0.0, -16.0),
            maxs: Vec3::new(16.0, 36.0, 16.0),
        }
    };

    // Parent entity holding transform, collision bounds, and animation state
    let parent_entity = commands
        .spawn((
            WorldStudioEntity {
                classname: classname.to_string(),
                model_path: model_path.to_string(),
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
            ))
            .id();

        commands.entity(parent_entity).add_child(child_entity);
        mesh_children.push((child_entity, mesh_handle, sub_idx));
    }

    anim_component.mesh_entities = mesh_children;
    commands.entity(parent_entity).insert(anim_component);

    tracing::info!(
        model = model_path,
        classname = classname,
        pos = ?position,
        "Spawned World 3D StudioModel Entity"
    );
    true
}
