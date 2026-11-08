//! GoldSrc Entity and Trigger Runtime.
//!
//! Manages interactive BSP entities:
//! - `func_breakable`: takes damage, plays material hit/break sounds, spawns gibs, fires `target`.
//! - `env_render`: modifies rendering visibility of target entities.
//! - `ambient_generic`: plays looping or triggered audio sounds, responds to `USE_TOGGLE`.
//! - `env_shooter`: shoots particle/gib models (e.g. feathers, debris) with directional spread.

use bevy::prelude::*;

/// Minimalist zero-cost LCG PRNG for ballistic gib particle scatter.
struct SimpleRng(u64);

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x5851_f42d_4c95_7f2d } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (self.0 >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::studio::store::{StudioModelStore, create_initial_bevy_mesh};
use crate::studio::world_entity::{BspEntitiesLump, WorldStudioEntity};

/// Material types for func_breakable and impact effects (matching GoldSrc `Materials` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum BreakableMaterial {
    #[default]
    Glass = 0,
    Wood = 1,
    Metal = 2,
    Flesh = 3,
    CinderBlock = 4,
    CeilingTile = 5,
    Computer = 6,
    UnbreakableGlass = 7,
    Rocks = 8,
}

impl BreakableMaterial {
    #[must_use]
    pub fn from_u32(val: u32) -> Self {
        match val {
            1 => Self::Wood,
            2 => Self::Metal,
            3 => Self::Flesh,
            4 => Self::CinderBlock,
            5 => Self::CeilingTile,
            6 => Self::Computer,
            7 => Self::UnbreakableGlass,
            8 => Self::Rocks,
            _ => Self::Glass,
        }
    }
}

/// Breakable brush entity component (`func_breakable`).
#[derive(Component, Debug, Clone)]
pub struct BreakableEntity {
    pub health: f32,
    pub max_health: f32,
    pub material: BreakableMaterial,
    pub model: String,
    pub target: String,
    pub targetname: String,
    pub bmin: Vec3,
    pub bmax: Vec3,
    pub is_broken: bool,
}

/// Tag component marking BSP submodel brush meshes (e.g. "*65" for radio).
#[derive(Component, Debug, Clone)]
pub struct BspSubmodelMesh {
    pub model: String,
}

/// Entity modifying rendering properties of targets (`env_render`).
#[derive(Component, Debug, Clone)]
pub struct EnvRenderEntity {
    pub targetname: String,
    pub target: String,
    pub rendermode: u32,
    pub renderamt: u32,
}

/// Spatial or ambient audio entity (`ambient_generic`).
#[derive(Component, Debug, Clone)]
pub struct AmbientAudioEntity {
    pub targetname: String,
    pub message: String,
    pub is_looping: bool,
    pub is_playing: bool,
    pub player_entity: Option<Entity>,
    pub position: Vec3,
}

/// Gib and particle shooter entity (`env_shooter`).
#[derive(Component, Debug, Clone)]
pub struct EnvShooterEntity {
    pub targetname: String,
    pub shootmodel: String,
    pub gibs: u32,
    pub variance: f32,
    pub velocity: f32,
    pub origin: Vec3,
}

/// Active flying gib particle (e.g. chicken feather, shard).
#[derive(Component, Debug, Clone)]
pub struct GibParticle {
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub lifetime: f32,
    pub max_lifetime: f32,
}

/// Event fired when a target is triggered (e.g. on breakable death or button press).
#[derive(Event, Debug, Clone)]
pub struct FireTargetEvent {
    pub target: String,
}

/// Event fired when an entity is destroyed, triggering material gib particles.
#[derive(Event, Debug, Clone)]
pub struct BreakEntityEvent {
    pub origin: Vec3,
    pub material: BreakableMaterial,
    pub model: String,
}

pub struct EntityRuntimePlugin;

impl Plugin for EntityRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<FireTargetEvent>();
        app.add_event::<BreakEntityEvent>();
        app.add_systems(
            Startup,
            spawn_bsp_interactive_entities
                .after(crate::level::setup_level)
                .before(crate::studio::spawn_map_studio_entities),
        );
        app.add_systems(
            Update,
            (
                handle_fire_target_events,
                handle_break_entity_events,
                update_gib_particles_system,
            ),
        );
    }
}

/// Spawns interactive entities from the active BSP entity lump.
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn spawn_bsp_interactive_entities(
    mut commands: Commands,
    entities_res: Option<Res<BspEntitiesLump>>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    let Some(ref entities_lump) = entities_res else {
        return;
    };
    let Ok(entities_str) = std::str::from_utf8(&entities_lump.0) else {
        return;
    };

    for block in entities_str.split('}') {
        let mut classname = "";
        let mut target = "";
        let mut targetname = "";
        let mut model_str = "";
        let mut origin_str = "";
        let mut message_str = "";
        let mut health_str = "";
        let mut material_str = "";
        let mut rendermode_str = "";
        let mut renderamt_str = "";
        let mut shootmodel_str = "";
        let mut gibs_str = "";
        let mut variance_str = "";
        let mut velocity_str = "";
        let mut spawnflags_str = "";

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('"') {
                let parts: Vec<&str> = trimmed.split('"').collect();
                if parts.len() >= 4 {
                    match parts[1] {
                        "classname" => classname = parts[3],
                        "target" => target = parts[3],
                        "targetname" => targetname = parts[3],
                        "model" => model_str = parts[3],
                        "origin" => origin_str = parts[3],
                        "message" => message_str = parts[3],
                        "health" => health_str = parts[3],
                        "material" => material_str = parts[3],
                        "rendermode" => rendermode_str = parts[3],
                        "renderamt" => renderamt_str = parts[3],
                        "shootmodel" => shootmodel_str = parts[3],
                        "m_iGibs" => gibs_str = parts[3],
                        "m_flVariance" => variance_str = parts[3],
                        "m_flVelocity" => velocity_str = parts[3],
                        "spawnflags" => spawnflags_str = parts[3],
                        _ => {}
                    }
                }
            }
        }

        if classname.is_empty() {
            continue;
        }

        // Parse optional origin
        let mut gx = 0.0f32;
        let mut gy = 0.0f32;
        let mut gz = 0.0f32;
        if !origin_str.is_empty() {
            let mut coords = origin_str.split_whitespace();
            if let (Some(x), Some(y), Some(z)) = (coords.next(), coords.next(), coords.next()) {
                gx = x.parse::<f32>().unwrap_or(0.0);
                gy = y.parse::<f32>().unwrap_or(0.0);
                gz = z.parse::<f32>().unwrap_or(0.0);
            }
        }
        let bevy_origin = Vec3::new(gx, gz, -gy);

        match classname {
            "func_breakable" => {
                let health = health_str.parse::<f32>().unwrap_or(1.0).max(1.0);
                let mat_id = material_str.parse::<u32>().unwrap_or(0);
                let material = BreakableMaterial::from_u32(mat_id);

                // Submodel bounds estimation (will be matched via model "*N")
                let (bmin, bmax) = if model_str.starts_with('*') {
                    get_submodel_bounds_or_fallback(model_str, bevy_origin)
                } else {
                    (bevy_origin - Vec3::splat(16.0), bevy_origin + Vec3::splat(16.0))
                };

                commands.spawn((
                    BreakableEntity {
                        health,
                        max_health: health,
                        material,
                        model: model_str.to_string(),
                        target: target.to_string(),
                        targetname: targetname.to_string(),
                        bmin,
                        bmax,
                        is_broken: false,
                    },
                    Transform::from_translation((bmin + bmax) * 0.5),
                    Visibility::default(),
                ));

                tracing::info!(
                    model = model_str,
                    target = target,
                    health = health,
                    "Spawned func_breakable entity"
                );
            }
            "env_render" => {
                let rendermode = rendermode_str.parse::<u32>().unwrap_or(0);
                let renderamt = renderamt_str.parse::<u32>().unwrap_or(0);
                commands.spawn(EnvRenderEntity {
                    targetname: targetname.to_string(),
                    target: target.to_string(),
                    rendermode,
                    renderamt,
                });
            }
            "ambient_generic" => {
                if !message_str.is_empty() {
                    let spawnflags = spawnflags_str.parse::<u32>().unwrap_or(0);
                    let is_everywhere = (spawnflags & 1) != 0;
                    let is_start_off = (spawnflags & 16) != 0;
                    let is_not_looping = (spawnflags & 32) != 0;
                    let is_looping = !is_not_looping;

                    // If entity is marked not-looping or start-off, it must only play when triggered
                    let starts_active = !is_start_off && !is_not_looping;

                    let mut player_entity = None;
                    let is_playing = starts_active;

                    if is_playing
                        && let Some(handle) = audio_bank.get_or_load_sound(
                            message_str,
                            &search_paths.dirs,
                            &mut audio_sources,
                        )
                    {
                        let mut settings = if is_looping {
                            PlaybackSettings::LOOP
                        } else {
                            PlaybackSettings::DESPAWN
                        };
                        if !is_everywhere {
                            settings = settings
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 64.0));
                        }

                        let p = commands
                            .spawn((
                                AudioPlayer::new(handle),
                                settings,
                                Transform::from_translation(bevy_origin),
                            ))
                            .id();
                        player_entity = Some(p);
                        tracing::info!(
                            message = message_str,
                            pos = ?bevy_origin,
                            is_looping,
                            "Spawned active ambient_generic loop"
                        );
                    }

                    commands.spawn(AmbientAudioEntity {
                        targetname: targetname.to_string(),
                        message: message_str.to_string(),
                        is_looping,
                        is_playing,
                        player_entity,
                        position: bevy_origin,
                    });
                }
            }
            "env_shooter" => {
                let gibs = gibs_str.parse::<u32>().unwrap_or(10).max(1);
                let variance = variance_str.parse::<f32>().unwrap_or(5.0);
                let velocity = velocity_str.parse::<f32>().unwrap_or(100.0);
                commands.spawn(EnvShooterEntity {
                    targetname: targetname.to_string(),
                    shootmodel: shootmodel_str.to_string(),
                    gibs,
                    variance,
                    velocity,
                    origin: bevy_origin,
                });
            }
            _ => {}
        }
    }
}

/// Helper mapping known cs_italy breakable brush submodels to exact Bevy world bounds.
fn get_submodel_bounds_or_fallback(model_str: &str, fallback_origin: Vec3) -> (Vec3, Vec3) {
    // Exact GoldSrc bounds from cs_italy.bsp:
    // Bevy X = GS X, Bevy Y = GS Z, Bevy Z = -GS Y
    match model_str {
        "*39" => (
            // Bottle 2: GS X: [-578, -558], Y: [617, 637], Z: [46, 62]
            Vec3::new(-578.0, 46.0, -637.0),
            Vec3::new(-558.0, 62.0, -617.0),
        ),
        "*40" => (
            // Bottle 1: GS X: [-570, -550], Y: [587, 607], Z: [46, 62]
            Vec3::new(-570.0, 46.0, -607.0),
            Vec3::new(-550.0, 62.0, -587.0),
        ),
        "*63" => (
            // Chicken 2: GS X: [1016, 1032], Y: [-72, -56], Z: [-152, -128]
            Vec3::new(1016.0, -152.0, 56.0),
            Vec3::new(1032.0, -128.0, 72.0),
        ),
        "*65" => (
            // Radio: GS X: [967, 1023], Y: [2306, 2362], Z: [128, 192]
            Vec3::new(967.0, 128.0, -2362.0),
            Vec3::new(1023.0, 192.0, -2306.0),
        ),
        "*66" => (
            // Chicken 1: GS X: [176, 192], Y: [-544, -528], Z: [-152, -128]
            Vec3::new(176.0, -152.0, 528.0),
            Vec3::new(192.0, -128.0, 544.0),
        ),
        _ => (
            fallback_origin - Vec3::splat(16.0),
            fallback_origin + Vec3::splat(16.0),
        ),
    }
}

/// Handles trigger dispatch when a target name is fired.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn handle_fire_target_events(
    mut commands: Commands,
    mut events: EventReader<FireTargetEvent>,
    mut render_query: Query<&EnvRenderEntity>,
    mut audio_query: Query<&mut AmbientAudioEntity>,
    shooter_query: Query<&EnvShooterEntity>,
    mut studio_query: Query<(&WorldStudioEntity, &mut Visibility)>,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    mut audio_bank: ResMut<GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for ev in events.read() {
        if ev.target.is_empty() {
            continue;
        }

        tracing::info!(target = ev.target, "Handling FireTargetEvent");

        // 1. EnvRender: Hide or modify target entities
        for render in &mut render_query {
            if render.targetname == ev.target {
                for (studio_ent, mut vis) in &mut studio_query {
                    if studio_ent.targetname == render.target {
                        *vis = Visibility::Hidden;
                        tracing::info!(target = render.target, "Hidden WorldStudioEntity via env_render");
                    }
                }
            }
        }

        // 2. AmbientAudio: Stop looping audio or play triggered sound
        for mut ambient in &mut audio_query {
            if ambient.targetname == ev.target {
                if ambient.is_playing {
                    // Turn off looping sound (e.g. Radio opera music)
                    if let Some(player_ent) = ambient.player_entity.take() {
                        commands.entity(player_ent).despawn();
                    }
                    ambient.is_playing = false;
                    tracing::info!(sound = ambient.message, "Stopped ambient_generic looping playback");
                } else {
                    // One-shot triggered playback (e.g. chicken death sound)
                    if let Some(handle) = audio_bank.get_or_load_sound(
                        &ambient.message,
                        &search_paths.dirs,
                        &mut audio_sources,
                    ) {
                        commands.spawn((
                            AudioPlayer::new(handle),
                            PlaybackSettings::DESPAWN
                                .with_spatial(true)
                                .with_spatial_scale(bevy::audio::SpatialScale::new(1.0 / 64.0)),
                            Transform::from_translation(ambient.position),
                        ));
                        tracing::info!(sound = ambient.message, "Triggered one-shot ambient sound");
                    }
                }
            }
        }

        // 3. EnvShooter: Shoot gib particles (e.g. feathers, debris)
        for shooter in &shooter_query {
            if shooter.targetname == ev.target {
                spawn_gibs(
                    &mut commands,
                    shooter,
                    &mut model_store,
                    &search_paths,
                    &mut images,
                    &mut materials,
                    &mut meshes,
                );
            }
        }
    }
}

/// Handles material-based gib bursts and brush despawning when a breakable entity is shattered.
#[allow(clippy::too_many_arguments)]
pub fn handle_break_entity_events(
    mut commands: Commands,
    mut events: EventReader<BreakEntityEvent>,
    mut submodel_query: Query<(&BspSubmodelMesh, &mut Visibility)>,
    mut model_store: ResMut<StudioModelStore>,
    search_paths: Res<GameSearchPaths>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for ev in events.read() {
        if !ev.model.is_empty() {
            for (submodel, mut vis) in &mut submodel_query {
                if submodel.model == ev.model {
                    *vis = Visibility::Hidden;
                    tracing::info!(model = ev.model, "Hidden BSP submodel on break");
                }
            }
        }

        let gib_model = match ev.material {
            BreakableMaterial::Computer => "models/computergibs.mdl",
            BreakableMaterial::Glass | BreakableMaterial::UnbreakableGlass => "models/glassgibs.mdl",
            BreakableMaterial::Wood => "models/woodgibs.mdl",
            BreakableMaterial::Metal => "models/metalplategibs.mdl",
            BreakableMaterial::Flesh => "models/fleshgibs.mdl",
            BreakableMaterial::CinderBlock => "models/cindergibs.mdl",
            BreakableMaterial::CeilingTile => "models/ceilinggibs.mdl",
            BreakableMaterial::Rocks => "models/rockgibs.mdl",
        };

        let shooter = EnvShooterEntity {
            targetname: String::new(),
            shootmodel: gib_model.to_string(),
            gibs: 12,
            variance: 6.0,
            velocity: 120.0,
            origin: ev.origin,
        };

        spawn_gibs(
            &mut commands,
            &shooter,
            &mut model_store,
            &search_paths,
            &mut images,
            &mut materials,
            &mut meshes,
        );
    }
}

/// Spawns 3D gibs (feathers or shards) according to `EnvShooterEntity` properties.
#[allow(clippy::too_many_arguments)]
fn spawn_gibs(
    commands: &mut Commands,
    shooter: &EnvShooterEntity,
    model_store: &mut StudioModelStore,
    search_paths: &GameSearchPaths,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
) {
    let Some(model_data) = model_store.get_or_load(&shooter.shootmodel, search_paths, images, materials)
    else {
        tracing::warn!(model = shooter.shootmodel, "Could not load gib shootmodel");
        return;
    };

    let mut rng = SimpleRng::new(u64::from(shooter.origin.x.to_bits()) ^ u64::from(shooter.gibs));

    for _ in 0..shooter.gibs {
        // Compute randomized initial velocity with variance
        let vx = rng.range_f32(-shooter.variance, shooter.variance) * 15.0;
        let vy = shooter.velocity + rng.range_f32(-shooter.variance, shooter.variance) * 20.0;
        let vz = rng.range_f32(-shooter.variance, shooter.variance) * 15.0;

        let spawn_pos = shooter.origin + Vec3::new(
            rng.range_f32(-6.0, 6.0),
            rng.range_f32(0.0, 8.0),
            rng.range_f32(-6.0, 6.0),
        );

        let gib_parent = commands
            .spawn((
                GibParticle {
                    velocity: Vec3::new(vx, vy, vz),
                    angular_velocity: Vec3::new(
                        rng.range_f32(-5.0, 5.0),
                        rng.range_f32(-5.0, 5.0),
                        rng.range_f32(-5.0, 5.0),
                    ),
                    lifetime: 0.0,
                    max_lifetime: rng.range_f32(3.0, 5.0),
                },
                Transform::from_translation(spawn_pos).with_scale(Vec3::splat(rng.range_f32(0.8, 1.3))),
                Visibility::default(),
            ))
            .id();

        for template in &model_data.submeshes {
            let bevy_mesh = create_initial_bevy_mesh(template, false);
            let mesh_handle = meshes.add(bevy_mesh);
            let child = commands
                .spawn((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(template.material.clone()),
                    Transform::IDENTITY,
                ))
                .id();
            commands.entity(gib_parent).add_child(child);
        }
    }

    tracing::info!(
        gibs = shooter.gibs,
        model = shooter.shootmodel,
        "Spawned gib particles burst"
    );
}

/// Advances position, ballistic gravity, and rotation for active gib particles.
pub fn update_gib_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut GibParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (entity, mut gib, mut transform) in &mut query {
        gib.lifetime += dt;
        if gib.lifetime >= gib.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Apply GoldSrc downward gravity (400 u/s^2) and air drag
        gib.velocity.y -= 400.0 * dt;
        gib.velocity.x *= 1.0 - 0.5 * dt;
        gib.velocity.z *= 1.0 - 0.5 * dt;

        transform.translation += gib.velocity * dt;
        transform.rotate_local_x(gib.angular_velocity.x * dt);
        transform.rotate_local_y(gib.angular_velocity.y * dt);
        transform.rotate_local_z(gib.angular_velocity.z * dt);
    }
}
