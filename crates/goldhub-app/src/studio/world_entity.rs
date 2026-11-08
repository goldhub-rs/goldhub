//! World entity spawner (hostages, chickens, decorative props) from BSP entity lumps.

use bevy::prelude::*;

use crate::audio::GameSearchPaths;
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
            let target_model = if classname == "hostage_entity" || classname == "monster_scientist" {
                if model_str.is_empty() {
                    "models/hostage.mdl"
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

            let is_solid = classname == "hostage_entity" || classname == "monster_scientist";

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
        targetname = targetname,
        pos = ?position,
        "Spawned World 3D StudioModel Entity"
    );
    true
}
