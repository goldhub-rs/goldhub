//! First-person tournament player controller integrated with Bevy.
//!
//! Bridges 128 Hz fixed sub-tick physics (`goldhub-physics`) to Bevy's rendering pipeline
//! with smooth interpolation, camera mouse look, and seamless switching between
//! player movement and free-fly spectator mode via 'V'.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use goldhub_formats::ir::CollisionModel;
use goldhub_physics::{
    BspWorldCollision, EntityCompositeTrace, FixedSubtickSimulator, IN_BACK, IN_DUCK, IN_FORWARD,
    IN_JUMP, IN_MOVELEFT, IN_MOVERIGHT, IN_SPEED, IN_USE, PlayerCmd, WorldTrace,
};

use crate::camera::SpectatorCamera;

/// Active control mode for the engine viewport camera.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraControlMode {
    #[default]
    Player,
    Spectator,
}

/// Camera perspective mode (FirstPerson vs ThirdPerson).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraPerspective {
    #[default]
    FirstPerson,
    ThirdPerson,
}

/// Stores the BSP collision model for physical hull tracing.
#[derive(Resource, Default)]
pub struct LevelCollisionResource {
    pub model: Option<CollisionModel>,
}

/// Player controller component attached to the player camera entity.
#[derive(Component, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct PlayerController {
    pub sim: FixedSubtickSimulator,
    pub pitch: f32,
    pub yaw: f32,
    pub mouse_sensitivity: f32,
    pub is_active: bool,
    pub step_controller: goldhub_audio::StepSoundController,
    pub was_on_ground: bool,
    pub health: f32,
    pub armor: f32,
    pub has_helmet: bool,
    pub has_defuser: bool,
    pub is_vip: bool,
    pub money: i32,
    pub team: goldhub_game_cstrike::CsTeam,
    pub in_bomb_zone: bool,
    pub in_buy_zone: bool,
    pub in_rescue_zone: bool,
    pub in_vip_safety_zone: bool,
    pub in_escape_zone: bool,
    pub flash_blind_alpha: f32,
    pub flash_hold_remaining: f32,
    pub flash_fade_duration: f32,
    pub flash_fade_elapsed: f32,
    pub screen_shake_timer: f32,
    pub screen_shake_amp: f32,
    pub anim_shoot_timer: f32,
    pub anim_shoot_retrigger: bool,
    pub center_hud_msg: String,
    pub center_hud_timer: f32,
    pub duck_visual_blend: f32,
    pub seed_step: u8,
    pub seed_vol: u8,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self {
            sim: FixedSubtickSimulator::default(),
            pitch: 0.0,
            yaw: 0.0,
            mouse_sensitivity: 0.002,
            is_active: true,
            step_controller: goldhub_audio::StepSoundController::default(),
            was_on_ground: false,
            health: 100.0,
            armor: 100.0,
            has_helmet: true,
            has_defuser: false,
            is_vip: false,
            money: 16000,
            team: goldhub_game_cstrike::CsTeam::Terrorist,
            in_bomb_zone: false,
            in_buy_zone: false,
            in_rescue_zone: false,
            in_vip_safety_zone: false,
            in_escape_zone: false,
            flash_blind_alpha: 0.0,
            flash_hold_remaining: 0.0,
            flash_fade_duration: 0.0,
            flash_fade_elapsed: 0.0,
            screen_shake_timer: 0.0,
            screen_shake_amp: 0.0,
            anim_shoot_timer: 0.0,
            anim_shoot_retrigger: false,
            center_hud_msg: String::new(),
            center_hud_timer: 0.0,
            duck_visual_blend: 0.0,
            seed_step: 0,
            seed_vol: 0,
        }
    }
}

impl PlayerController {
    /// Triggers a third-person upper-body shooting/attack one-shot animation window.
    pub fn trigger_third_person_shoot(&mut self, duration: f32) {
        self.anim_shoot_timer = self.anim_shoot_timer.max(duration);
        self.anim_shoot_retrigger = true;
    }

    /// Shows a transient CS 1.6 centered status message (e.g., Glock burst/semi switch).
    pub fn set_center_message(&mut self, msg: impl Into<String>, duration: f32) {
        self.center_hud_msg = msg.into();
        self.center_hud_timer = duration;
    }

    /// Applies flashbang `ScreenFade` parameters matching `CBasePlayer::RadiusFlash` / `PlayerBlind`.
    pub fn apply_flash(&mut self, alpha: f32, fade_time: f32, mut hold_time: f32) {
        let already_holding = self.flash_hold_remaining > 0.0;
        let still_blind = already_holding
            || (self.flash_fade_duration > 0.0
                && self.flash_fade_elapsed < self.flash_fade_duration);

        if alpha >= 255.0 && already_holding {
            hold_time += self.flash_hold_remaining;
        }
        if still_blind {
            let rem_fade = (self.flash_fade_duration - self.flash_fade_elapsed).max(0.0);
            self.flash_fade_duration = rem_fade.max(fade_time);
            self.flash_fade_elapsed = 0.0;
            self.flash_blind_alpha = self.flash_blind_alpha.max(alpha);
        } else {
            self.flash_fade_duration = fade_time;
            self.flash_fade_elapsed = 0.0;
            self.flash_blind_alpha = alpha;
        }
        self.flash_hold_remaining = hold_time;
    }

    /// Evaluates current fullscreen white flash opacity (`0.0..=1.0`) matching GoldSrc `V_FadeAlpha`.
    #[must_use]
    pub fn current_flash_fraction(&self) -> f32 {
        if self.flash_blind_alpha <= 0.0 {
            return 0.0;
        }
        let max_alpha = (self.flash_blind_alpha / 255.0).clamp(0.0, 1.0);
        if self.flash_hold_remaining > 0.0 {
            return max_alpha;
        }
        if self.flash_fade_duration <= 0.0 || self.flash_fade_elapsed >= self.flash_fade_duration {
            return 0.0;
        }
        let frac = 1.0 - (self.flash_fade_elapsed / self.flash_fade_duration);
        (max_alpha * frac).clamp(0.0, 1.0)
    }

    /// Perceptually gamma-calibrated flashbang opacity for sRGB Bevy UI rendering (`current_flash_fraction^1.8`).
    #[must_use]
    pub fn current_flash_render_alpha(&self) -> f32 {
        self.current_flash_fraction().powf(1.8).clamp(0.0, 1.0)
    }

    /// Applies explosive/blast damage with CS 1.6 kevlar attenuation (`ARMOR_RATIO = 0.5`, `ARMOR_BONUS = 0.5`).
    pub fn apply_blast_damage(&mut self, damage: f32) {
        if damage < 1.0 {
            return;
        }
        let mut dmg = damage;
        if self.armor > 0.0 {
            let fl_new = dmg * 0.5;
            let fl_armor = (dmg - fl_new) * 0.5;
            if fl_armor > self.armor {
                dmg -= self.armor / 0.5;
                self.armor = 0.0;
            } else {
                self.armor = (self.armor - fl_armor).max(0.0);
                dmg = fl_new;
            }
        }
        let loss = dmg.floor().max(0.0);
        if loss >= 1.0 {
            self.health = (self.health - loss).max(0.0);
        }
    }

    /// Ticks flashbang fade, camera shake, 3rd-person shoot timer, and center HUD timer.
    pub fn tick_effects(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        if self.anim_shoot_timer > 0.0 {
            self.anim_shoot_timer = (self.anim_shoot_timer - dt).max(0.0);
        }
        if self.center_hud_timer > 0.0 {
            self.center_hud_timer = (self.center_hud_timer - dt).max(0.0);
        }
        if self.screen_shake_timer > 0.0 {
            self.screen_shake_timer = (self.screen_shake_timer - dt).max(0.0);
            if self.screen_shake_timer <= 0.0 {
                self.screen_shake_amp = 0.0;
            }
        }
        if self.flash_hold_remaining > 0.0 {
            self.flash_hold_remaining = (self.flash_hold_remaining - dt).max(0.0);
        } else if self.flash_fade_duration > 0.0
            && self.flash_fade_elapsed < self.flash_fade_duration
        {
            self.flash_fade_elapsed += dt;
            if self.flash_fade_elapsed >= self.flash_fade_duration {
                self.flash_blind_alpha = 0.0;
                self.flash_fade_duration = 0.0;
                self.flash_fade_elapsed = 0.0;
            }
        }
    }
}

/// Marker component for the in-game speedometer HUD text.
#[derive(Component)]
pub struct SpeedometerText;

/// Tracks the player's flashlight state, battery level (0.0 .. 1.0), and dynamic lighting entities.
#[derive(Component, Debug)]
pub struct PlayerFlashlight {
    pub is_on: bool,
    pub battery: f32, // 1.0 = 100% full, 0.0 = discharged
    pub light_entity: Option<Entity>,
    pub beam_hit_light: Option<Entity>,
    pub decal_entity: Option<Entity>,
}

impl Default for PlayerFlashlight {
    fn default() -> Self {
        Self {
            is_on: false,
            battery: 1.0,
            light_entity: None,
            beam_hit_light: None,
            decal_entity: None,
        }
    }
}

/// Marker component for the projected flashlight wall decal.
#[derive(Component)]
pub struct FlashlightDecalMarker;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraControlMode>();
        app.init_resource::<CameraPerspective>();
        app.init_resource::<LevelCollisionResource>();
        app.add_systems(Startup, setup_speedometer_hud);
        app.add_systems(
            Update,
            (
                handle_round_reset_player_system.before(player_movement_system),
                player_movement_system,
                controller_toggle_system,
                update_speedometer_system,
                player_flashlight_system,
                spawn_player_character_model_system,
                update_player_character_model_system
                    .after(player_movement_system)
                    .before(crate::studio::studio_animation_system),
                update_third_person_weapon_system.after(crate::studio::studio_animation_system),
            ),
        );
    }
}

fn aabb_overlaps_player_hull(p_bevy: Vec3, bmin: Vec3, bmax: Vec3) -> bool {
    (p_bevy.x + 16.0) >= bmin.x
        && (p_bevy.x - 16.0) <= bmax.x
        && (p_bevy.z + 16.0) >= bmin.z
        && (p_bevy.z - 16.0) <= bmax.z
        && (p_bevy.y + 36.0) >= bmin.y
        && (p_bevy.y - 36.0) <= bmax.y
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn player_flashlight_system(
    mut commands: Commands,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    collision_res: Res<LevelCollisionResource>,
    mut camera_query: Query<
        (Entity, &GlobalTransform, &mut PlayerFlashlight),
        (With<Camera3d>, With<PlayerController>),
    >,
    mut hit_light_query: Query<(&mut Transform, &mut PointLight), Without<PlayerController>>,
    mut decal_query: Query<
        (&mut Transform, &mut Visibility),
        (
            With<FlashlightDecalMarker>,
            Without<Camera3d>,
            Without<PointLight>,
        ),
    >,
    mut sprite_store: Option<ResMut<crate::sprite::SpriteStore>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut images: Option<ResMut<Assets<Image>>>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut audio_bank: Option<ResMut<crate::audio::GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
    console: Option<Res<crate::hud::text_channels::ConsoleOverlayState>>,
) {
    const FLASHLIGHT_DRAIN_SECS: f32 = 120.0;
    const FLASHLIGHT_RECHARGE_SECS: f32 = 20.0;
    const FLASHLIGHT_MAX_DIST: f32 = 2000.0;
    const FLASHLIGHT_HIT_BASE_INTENSITY: f32 = 12_000.0;

    let dt = time.delta_secs();
    let Ok((camera_entity, cam_gt, mut flashlight)) = camera_query.get_single_mut() else {
        return;
    };

    // GoldSrc flashlight battery (`FLASH_DRAIN_TIME = 120s`, `FLASH_CHARGE_TIME = 20s`)
    if flashlight.is_on {
        flashlight.battery = (flashlight.battery - dt / FLASHLIGHT_DRAIN_SECS).max(0.0);
        if flashlight.battery <= 0.0 {
            flashlight.is_on = false;
            if let Some(light) = flashlight.light_entity.take() {
                commands.entity(light).despawn_recursive();
            }
            if let Some(hit_light) = flashlight.beam_hit_light.take() {
                commands.entity(hit_light).despawn_recursive();
            }
            if let Some(decal) = flashlight.decal_entity.take() {
                commands.entity(decal).despawn_recursive();
            }
        }
    } else {
        flashlight.battery = (flashlight.battery + dt / FLASHLIGHT_RECHARGE_SECS).min(1.0);
    }

    let console_open = console.as_ref().is_some_and(|c| c.open);
    if !console_open && keyboard.just_pressed(KeyCode::KeyF) {
        if !flashlight.is_on && flashlight.battery <= 0.05 {
            return;
        }

        flashlight.is_on = !flashlight.is_on;

        if flashlight.is_on {
            let spot = commands
                .spawn((
                    SpotLight {
                        color: Color::srgb(1.0, 0.98, 0.94),
                        intensity: 85_000.0,
                        range: 2_000.0,
                        radius: 12.0,
                        inner_angle: 15.0f32.to_radians(),
                        outer_angle: 26.0f32.to_radians(),
                        shadows_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, -0.05, -0.3),
                ))
                .id();
            commands.entity(camera_entity).add_child(spot);
            flashlight.light_entity = Some(spot);

            let hit_pt = commands
                .spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.98, 0.92),
                        intensity: FLASHLIGHT_HIT_BASE_INTENSITY,
                        range: 340.0,
                        shadows_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(
                        cam_gt.translation() + cam_gt.forward().as_vec3() * 128.0,
                    ),
                ))
                .id();
            flashlight.beam_hit_light = Some(hit_pt);

            if let (
                Some(ref mut store),
                Some(ref mut m_assets),
                Some(ref mut mat_assets),
                Some(ref mut img_assets),
            ) = (
                sprite_store.as_mut(),
                meshes.as_mut(),
                materials.as_mut(),
                images.as_mut(),
            ) {
                let quad = store.get_or_create_unit_quad(m_assets);
                let decal_mat = store.get_or_create_flashlight_mat(img_assets, mat_assets);
                let decal = commands
                    .spawn((
                        FlashlightDecalMarker,
                        Mesh3d(quad),
                        MeshMaterial3d(decal_mat),
                        Transform::from_translation(
                            cam_gt.translation() + cam_gt.forward().as_vec3() * 128.0,
                        )
                        .with_scale(Vec3::splat(80.0)),
                        Visibility::Inherited,
                        bevy::render::view::RenderLayers::from_layers(&[0, 2]),
                    ))
                    .id();
                flashlight.decal_entity = Some(decal);
            }
        } else {
            if let Some(light) = flashlight.light_entity.take() {
                commands.entity(light).despawn_recursive();
            }
            if let Some(hit_light) = flashlight.beam_hit_light.take() {
                commands.entity(hit_light).despawn_recursive();
            }
            if let Some(decal) = flashlight.decal_entity.take() {
                commands.entity(decal).despawn_recursive();
            }
        }

        if let (Some(paths), Some(ref mut bank), Some(ref mut sources)) =
            (search_paths, audio_bank.as_mut(), audio_sources.as_mut())
            && let Some(handle) =
                bank.get_or_load_sound("items/flashlight1.wav", &paths.dirs, sources)
        {
            commands.spawn((AudioPlayer::new(handle), PlaybackSettings::DESPAWN));
        }
    }

    // GoldSrc `CL_PlayerFlashlight` surface projection: wide bright pool up to 2000u.
    if flashlight.is_on
        && let Some(hit_light_ent) = flashlight.beam_hit_light
        && let Ok((mut lt, mut pl)) = hit_light_query.get_mut(hit_light_ent)
    {
        let decal_item = if let Some(decal_ent) = flashlight.decal_entity
            && let Ok((dtf, dvis)) = decal_query.get_mut(decal_ent)
        {
            Some((dtf.into_inner(), dvis.into_inner()))
        } else {
            None
        };

        project_flashlight_hit(
            cam_gt,
            collision_res.model.as_ref(),
            FLASHLIGHT_MAX_DIST,
            FLASHLIGHT_HIT_BASE_INTENSITY,
            &mut lt,
            &mut pl,
            decal_item,
        );
    }
}

fn project_flashlight_hit(
    cam_gt: &GlobalTransform,
    col_model: Option<&goldhub_formats::ir::CollisionModel>,
    max_dist: f32,
    base_intensity: f32,
    lt: &mut Transform,
    pl: &mut PointLight,
    decal_tf_vis: Option<(&mut Transform, &mut Visibility)>,
) {
    let start = cam_gt.translation();
    let dir = cam_gt.forward().as_vec3();
    let (hit_pos, _hit_norm, hit_dist) =
        col_model.map_or((start + dir * 256.0, -dir, 256.0_f32), |col| {
            use goldhub_physics::WorldTrace;
            let bsp = goldhub_physics::BspWorldCollision::new(col);
            let end = start + dir * max_dist;
            let tr = bsp.trace_player_hull([start.x, -start.z, start.y], [end.x, -end.z, end.y], 0);
            let p = Vec3::new(tr.endpos[0], tr.endpos[2], -tr.endpos[1]);
            let norm = Vec3::new(tr.plane_normal[0], tr.plane_normal[2], -tr.plane_normal[1])
                .normalize_or_zero();
            let dist = tr.fraction * max_dist;
            (p, norm, dist)
        });
    lt.translation = hit_pos - dir * 16.0;
    if hit_dist >= max_dist - 1.0 {
        pl.intensity = 0.0;
    } else {
        // Keep wall illumination crisp across hallways while attenuating gently toward 2000u max reach
        let falloff = ((max_dist - hit_dist) / (max_dist * 0.75)).clamp(0.45, 1.0);
        pl.intensity = base_intensity * falloff;
    }
    if let Some((_, vis)) = decal_tf_vis {
        *vis = Visibility::Hidden;
    }
}

fn setup_speedometer_hud(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(46.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                SpeedometerText,
                Text::new("spd: 0.00 u/s"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.0, 1.0, 0.4)),
            ));
        });
}

fn update_speedometer_system(
    player_query: Query<&PlayerController>,
    mut text_query: Query<&mut Text, With<SpeedometerText>>,
) {
    if let Ok(player) = player_query.get_single() {
        let spd = player.sim.current.horizontal_speed();
        for mut text in &mut text_query {
            **text = format!("spd: {spd:.2} u/s");
        }
    }
}

/// Fallback world collision when no BSP map is loaded (infinite floor at Z = 0).
struct FallbackFlatWorld;

impl WorldTrace for FallbackFlatWorld {
    fn trace_player_hull(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        _hull_idx: u32,
    ) -> goldhub_physics::TraceResult {
        const DIST_EPSILON: f32 = 0.03125;
        if end[2] <= 0.0 {
            let frac = if start[2] > 0.0 {
                (start[2] / (start[2] - end[2])).clamp(0.0, 1.0)
            } else {
                0.0
            };
            goldhub_physics::TraceResult {
                startsolid: start[2] < 0.0,
                allsolid: false,
                fraction: frac,
                endpos: [
                    start[0] + (end[0] - start[0]) * frac,
                    start[1] + (end[1] - start[1]) * frac,
                    DIST_EPSILON,
                ],
                plane_normal: [0.0, 0.0, 1.0],
                plane_dist: 0.0,
                hit_entity: None,
                ..Default::default()
            }
        } else {
            goldhub_physics::TraceResult {
                startsolid: false,
                allsolid: false,
                fraction: 1.0,
                endpos: end,
                plane_normal: [0.0, 0.0, 1.0],
                plane_dist: 0.0,
                hit_entity: None,
                ..Default::default()
            }
        }
    }

    fn test_player_position(&self, pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
        if pos[2] < 0.0 { Some(0) } else { None }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct ZoneQueries<'w, 's> {
    pub bomb_zones: Query<'w, 's, &'static crate::entity::BombTargetZoneEntity>,
    pub bomb_points: Query<'w, 's, &'static crate::entity::BombTargetPointEntity>,
    pub rescue_zones: Query<'w, 's, &'static crate::entity::HostageRescueZoneEntity>,
    pub buy_zones: Query<'w, 's, &'static crate::entity::BuyZoneEntity>,
    pub vip_zones: Query<'w, 's, &'static crate::entity::VipSafetyZoneEntity>,
    pub escape_zones: Query<'w, 's, &'static crate::entity::EscapeZoneEntity>,
    pub bomb_params: Option<Res<'w, crate::entity::BombMapParams>>,
    pub console: Option<Res<'w, crate::hud::text_channels::ConsoleOverlayState>>,
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn player_movement_system(
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    collision_res: Res<LevelCollisionResource>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut query: Query<(
        &mut Transform,
        &mut PlayerController,
        Option<&crate::combat::PlayerRecoil>,
        Option<&crate::combat::ActiveWeaponState>,
    )>,
    zones: ZoneQueries,
    solid_query: Query<
        (
            &Transform,
            &crate::studio::EntityCollisionBounds,
            &crate::studio::WorldStudioEntity,
        ),
        Without<PlayerController>,
    >,
    mut commands: Commands,
    mut audio_bank: ResMut<crate::audio::GoldSrcAudioBank>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    search_paths: Res<crate::audio::GameSearchPaths>,
) {
    if *mode != CameraControlMode::Player {
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // Query active solid studio entities (filtering by is_solid so props/chickens don't block)
    let mut solid_entities = Vec::with_capacity(8);
    for (ent_trans, bounds, studio_ent) in &solid_query {
        if !studio_ent.is_solid {
            continue;
        }
        let p = ent_trans.translation;
        // Bevy to GoldSrc: gx = p.x, gy = -p.z, gz = p.y
        let gx = p.x;
        let gy = -p.z;
        let gz = p.y;
        let gs_mins = [gx + bounds.mins.x, gy - bounds.maxs.z, gz + bounds.mins.y];
        let gs_maxs = [gx + bounds.maxs.x, gy - bounds.mins.z, gz + bounds.maxs.y];
        solid_entities.push((gs_mins, gs_maxs));
    }

    let has_bomb_brushes = !zones.bomb_zones.is_empty();

    for (mut transform, mut player, recoil, active_weapon) in &mut query {
        player.tick_effects(dt);

        if !player.is_active {
            continue;
        }

        // Update ReGameDLL zone flags (`m_bInBombZone`, `m_bInBuyZone`, `m_bInRescueZone`, VIP/Escape)
        let ori = player.sim.current.origin;
        let p_bevy = Vec3::new(ori[0], ori[2], -ori[1]);
        player.in_bomb_zone = if has_bomb_brushes {
            zones
                .bomb_zones
                .iter()
                .any(|z| aabb_overlaps_player_hull(p_bevy, z.bmin, z.bmax))
        } else {
            zones
                .bomb_points
                .iter()
                .any(|pt| p_bevy.distance(pt.origin) <= pt.radius)
        };
        player.in_rescue_zone = zones
            .rescue_zones
            .iter()
            .any(|z| aabb_overlaps_player_hull(p_bevy, z.bmin, z.bmax));
        player.in_vip_safety_zone = zones
            .vip_zones
            .iter()
            .any(|z| aabb_overlaps_player_hull(p_bevy, z.bmin, z.bmax));
        player.in_escape_zone = zones
            .escape_zones
            .iter()
            .any(|z| aabb_overlaps_player_hull(p_bevy, z.bmin, z.bmax));

        let team_blocked = zones
            .bomb_params
            .as_ref()
            .is_some_and(|bp| match player.team {
                goldhub_game_cstrike::CsTeam::CounterTerrorist => bp.ct_cant_buy,
                goldhub_game_cstrike::CsTeam::Terrorist => bp.t_cant_buy,
                _ => false,
            });
        player.in_buy_zone = !team_blocked
            && (zones.buy_zones.is_empty()
                || zones
                    .buy_zones
                    .iter()
                    .any(|z| aabb_overlaps_player_hull(p_bevy, z.bmin, z.bmax)));

        // Freeze horizontal movement (`maxspeed = 1.0`) while actively arming C4
        let arming_c4 = active_weapon.is_some_and(|w| w.c4_arming);
        player.sim.vars.maxspeed = if arming_c4 {
            1.0
        } else {
            active_weapon.map_or(250.0, |w| {
                let fov_u8 = w.current_fov.round().clamp(1.0, 120.0) as u8;
                w.id.effective_max_speed(fov_u8)
            })
        };

        let console_open = zones.console.as_ref().is_some_and(|c| c.open);

        // 1. Mouse Look: Pitch and Yaw (with zoom_sensitivity_ratio proportional scaling)
        let mouse_delta = if console_open {
            Vec2::ZERO
        } else {
            mouse_motion.delta
        };
        if mouse_delta.length_squared() > 0.0 {
            let zoom_sens = active_weapon.map_or(1.0, |w| (w.current_fov / 90.0).clamp(0.1, 1.0));
            let sens = player.mouse_sensitivity * zoom_sens;
            player.yaw -= mouse_delta.x * sens;
            player.pitch = (player.pitch - mouse_delta.y * sens)
                .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        }

        // GoldSrc angles: pitch in degrees [-89, 89], yaw in degrees [0, 360]
        // In GoldSrc convention (and angle_vectors): negative pitch is looking UP (-sp > 0 -> +Z).
        // In Bevy mouse look, looking up gives positive player.pitch.
        // Therefore, pitch_deg must be inverted to match GoldSrc coordinate convention.
        let pitch_deg = -player.pitch.to_degrees();
        let yaw_deg = (player.yaw.to_degrees() + 90.0).rem_euclid(360.0);

        // 2. Keyboard Input mapping to PlayerCmd
        let mut forward_move = 0.0f32;
        let mut side_move = 0.0f32;
        let mut buttons = 0u32;
        let move_scale = player.sim.vars.maxspeed;

        if !console_open {
            if keyboard.pressed(KeyCode::KeyW) {
                forward_move += move_scale;
                buttons |= IN_FORWARD;
            }
            if keyboard.pressed(KeyCode::KeyS) {
                forward_move -= move_scale;
                buttons |= IN_BACK;
            }
            if keyboard.pressed(KeyCode::KeyA) {
                side_move -= move_scale;
                buttons |= IN_MOVELEFT;
            }
            if keyboard.pressed(KeyCode::KeyD) {
                side_move += move_scale;
                buttons |= IN_MOVERIGHT;
            }
            if keyboard.pressed(KeyCode::Space) {
                buttons |= IN_JUMP;
            }
            if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::KeyC) {
                buttons |= IN_DUCK;
            }
            if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                buttons |= IN_SPEED;
            }
            if keyboard.pressed(KeyCode::KeyE) {
                buttons |= IN_USE;
            }
        }

        let cmd = PlayerCmd {
            forward_move,
            side_move,
            up_move: 0.0,
            buttons,
            view_angles: [pitch_deg, yaw_deg, 0.0],
        };

        // 3. Advance fixed 128 Hz simulation against BSP collision or fallback world (with solid entities)
        if let Some(ref col_model) = collision_res.model {
            let bsp_world = BspWorldCollision::new(col_model);
            let composite = EntityCompositeTrace::new(&bsp_world, &solid_entities);
            player.sim.advance(dt, &cmd, &composite);
        } else {
            let flat_world = FallbackFlatWorld;
            let composite = EntityCompositeTrace::new(&flat_world, &solid_entities);
            player.sim.advance(dt, &cmd, &composite);
        }

        // 4. ReGameDLL authentic fall damage (`PM_PLAYER_MAX_SAFE_FALL_SPEED = 580.0`)
        let pre_land_fvel = player
            .sim
            .previous
            .fall_velocity
            .max(-player.sim.previous.velocity[2]);
        let just_landed = !player.sim.previous.on_ground && player.sim.current.on_ground;
        player.was_on_ground = player.sim.current.on_ground;
        if just_landed && pre_land_fvel > 580.0 {
            let dmg =
                ((pre_land_fvel - 580.0) * (100.0 / (1024.0 - 580.0)) * 1.25).clamp(1.0, 100.0);
            player.health = (player.health - dmg).max(0.0);
            let pain_idx = ((pre_land_fvel as usize) % 3) + 1;
            let pain_snd = if player.health <= 0.0 {
                "common/bodysplat.wav"
            } else {
                match pain_idx {
                    1 => "player/bhit_flesh-1.wav",
                    2 => "player/bhit_flesh-2.wav",
                    _ => "player/bhit_flesh-3.wav",
                }
            };
            if let Some(h) =
                audio_bank.get_or_load_sound(pain_snd, &search_paths.dirs, &mut audio_sources)
            {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
        }

        // 5. Interpolate origin & view height for smooth sub-frame rendering
        let (gs_pos, view_h) = player.sim.interpolate_for_render();

        // 6. Coordinate system transformation from GoldSrc (Z-up) to Bevy (Y-up)
        let eye_pos = Vec3::new(gs_pos[0], gs_pos[2] + view_h, -gs_pos[1]);
        let (recoil_pitch, recoil_yaw) = recoil.map_or((0.0, 0.0), |r| {
            (r.punch_pitch.to_radians(), r.punch_yaw.to_radians())
        });
        // Combine weapon kickback recoil with physical impact landing punch (pitch and roll)
        let sim_punch = player.sim.current.punch_angle;
        let landing_pitch = sim_punch[0].to_radians();
        let landing_roll = sim_punch[2].to_radians();

        let shake_p = if player.screen_shake_timer > 0.0 {
            (time.elapsed_secs() * 55.0).sin() * player.screen_shake_amp
        } else {
            0.0
        };
        let shake_r = if player.screen_shake_timer > 0.0 {
            (time.elapsed_secs() * 41.0).cos() * player.screen_shake_amp * 0.6
        } else {
            0.0
        };

        let total_pitch = player.pitch + recoil_pitch - landing_pitch + shake_p;
        let total_yaw = player.yaw + recoil_yaw;
        let total_roll = landing_roll + shake_r;

        let cam_rot = Quat::from_rotation_y(total_yaw)
            * Quat::from_rotation_x(total_pitch)
            * Quat::from_rotation_z(total_roll);

        // Support GoldSrc Third-Person View (cam_idealdist = 64) with wall-clip protection
        let final_pos = if *perspective == CameraPerspective::ThirdPerson {
            const THIRD_PERSON_DIST: f32 = 64.0;
            let back_dir = cam_rot * Vec3::Z;
            let target_pos = eye_pos + back_dir * THIRD_PERSON_DIST;

            collision_res
                .model
                .as_ref()
                .map_or(target_pos, |col_model| {
                    use goldhub_physics::WorldTrace;
                    let bsp_world = BspWorldCollision::new(col_model);
                    let gs_eye = [eye_pos.x, -eye_pos.z, eye_pos.y];
                    let gs_target = [target_pos.x, -target_pos.z, target_pos.y];
                    let tr = bsp_world.trace_player_hull(gs_eye, gs_target, 0);
                    if tr.fraction < 1.0 {
                        let clipped_dist = (tr.fraction * THIRD_PERSON_DIST - 4.0).max(8.0);
                        eye_pos + back_dir * clipped_dist
                    } else {
                        target_pos
                    }
                })
        } else {
            eye_pos
        };

        transform.translation = final_pos;
        transform.rotation = cam_rot;
    }
}

/// Allows toggling between authentic Player movement and Spectator noclip camera with 'M' / 'F3',
/// and toggling First-Person vs Third-Person with 'F5'.
fn controller_toggle_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    console: Option<Res<crate::hud::text_channels::ConsoleOverlayState>>,
    mut mode: ResMut<CameraControlMode>,
    mut perspective: ResMut<CameraPerspective>,
    mut query: Query<(&Transform, &mut PlayerController, &mut SpectatorCamera)>,
) {
    if console.as_ref().is_some_and(|c| c.open) {
        return;
    }

    if keyboard.just_pressed(KeyCode::F5) {
        *perspective = match *perspective {
            CameraPerspective::FirstPerson => {
                tracing::info!("Camera perspective: ThirdPerson (GoldSrc cam_command 1)");
                CameraPerspective::ThirdPerson
            }
            CameraPerspective::ThirdPerson => {
                tracing::info!("Camera perspective: FirstPerson");
                CameraPerspective::FirstPerson
            }
        };
    }

    if keyboard.just_pressed(KeyCode::F6)
        && let Ok((_, mut player, _)) = query.get_single_mut()
    {
        player.team = match player.team {
            goldhub_game_cstrike::CsTeam::Terrorist => {
                goldhub_game_cstrike::CsTeam::CounterTerrorist
            }
            goldhub_game_cstrike::CsTeam::CounterTerrorist => {
                goldhub_game_cstrike::CsTeam::Terrorist
            }
            _ => goldhub_game_cstrike::CsTeam::Terrorist,
        };
        player.has_defuser = player.team == goldhub_game_cstrike::CsTeam::CounterTerrorist;
        tracing::info!(team = ?player.team, has_defuser = player.has_defuser, "Switched player team (F6)");
    }

    if keyboard.just_pressed(KeyCode::KeyM) || keyboard.just_pressed(KeyCode::F3) {
        let Ok((transform, mut player, mut spec)) = query.get_single_mut() else {
            return;
        };

        *mode = match *mode {
            CameraControlMode::Player => {
                tracing::info!("Switched to Spectator Noclip Camera (Fly mode)");
                spec.yaw = player.yaw;
                spec.pitch = player.pitch;
                spec.is_active = true;
                player.is_active = false;
                CameraControlMode::Spectator
            }
            CameraControlMode::Spectator => {
                tracing::info!(
                    "Switched to First-Person Player Controller (128 Hz Tournament CS 1.6)"
                );
                // Convert back from Bevy coordinates to GoldSrc coordinates
                let gs_x = transform.translation.x;
                let gs_y = -transform.translation.z;
                let gs_z = transform.translation.y - 17.0;
                player.sim.current.origin = [gs_x, gs_y, gs_z];
                player.sim.previous.origin = [gs_x, gs_y, gs_z];
                // Seamlessly transfer spectator flight inertia into player simulation velocity
                // Bevy X = GS X, Bevy Y = GS Z, Bevy Z = -GS Y
                let gs_vx = spec.velocity.x;
                let gs_vy = -spec.velocity.z;
                let gs_vz = spec.velocity.y;
                player.sim.current.velocity = [gs_vx, gs_vy, gs_vz];
                player.sim.previous.velocity = [gs_vx, gs_vy, gs_vz];
                player.sim.current.on_ground = false;
                player.yaw = spec.yaw;
                player.pitch = spec.pitch;
                player.is_active = true;
                spec.is_active = false;
                CameraControlMode::Player
            }
        };
    }
}

/// Parses player spawn coordinates matching `team` (with fallback to any valid spawn) from BSP entities lump.
#[must_use]
pub fn parse_player_spawn_for_team(
    entities_raw: &[u8],
    team: goldhub_game_cstrike::CsTeam,
) -> Option<([f32; 3], f32)> {
    let target_classname = match team {
        goldhub_game_cstrike::CsTeam::Terrorist => "info_player_deathmatch",
        _ => "info_player_start",
    };
    let mut fallback: Option<([f32; 3], f32)> = None;
    let entities_str = std::str::from_utf8(entities_raw).ok()?;

    for block in entities_str.split('}') {
        let mut classname = "";
        let mut origin = None;
        let mut angle = 0.0f32;

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"classname\"")
                && let Some(c_str) = trimmed.split('"').nth(3)
            {
                classname = c_str;
            } else if trimmed.starts_with("\"origin\"")
                && let Some(o_str) = trimmed.split('"').nth(3)
            {
                let mut parts = o_str.split_whitespace();
                if let (Some(x), Some(y), Some(z)) = (parts.next(), parts.next(), parts.next())
                    && let (Ok(fx), Ok(fy), Ok(fz)) =
                        (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>())
                {
                    origin = Some([fx, fy, fz]);
                }
            } else if trimmed.starts_with("\"angle\"")
                && let Some(a_str) = trimmed.split('"').nth(3)
                && let Ok(fa) = a_str.parse::<f32>()
            {
                angle = fa;
            }
        }

        if let Some(orig) = origin
            && (classname == "info_player_start" || classname == "info_player_deathmatch")
        {
            if classname == target_classname {
                return Some((orig, angle));
            }
            if fallback.is_none() {
                fallback = Some((orig, angle));
            }
        }
    }

    fallback
}

/// Parses player spawn coordinates (`info_player_start` / `info_player_deathmatch`) from BSP entities lump.
#[must_use]
pub fn parse_first_player_spawn(entities_raw: &[u8]) -> Option<([f32; 3], f32)> {
    let entities_str = std::str::from_utf8(entities_raw).ok()?;

    for block in entities_str.split('}') {
        let mut is_spawn = false;
        let mut origin = None;
        let mut angle = 0.0f32;

        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"classname\"")
                && let Some(c_str) = trimmed.split('"').nth(3)
                && (c_str == "info_player_start" || c_str == "info_player_deathmatch")
            {
                is_spawn = true;
            } else if trimmed.starts_with("\"origin\"")
                && let Some(o_str) = trimmed.split('"').nth(3)
            {
                let mut parts = o_str.split_whitespace();
                if let (Some(x), Some(y), Some(z)) = (parts.next(), parts.next(), parts.next())
                    && let (Ok(fx), Ok(fy), Ok(fz)) =
                        (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>())
                {
                    origin = Some([fx, fy, fz]);
                }
            } else if trimmed.starts_with("\"angle\"")
                && let Some(a_str) = trimmed.split('"').nth(3)
                && let Ok(fa) = a_str.parse::<f32>()
            {
                angle = fa;
            }
        }

        if is_spawn && let Some(orig) = origin {
            return Some((orig, angle));
        }
    }

    None
}

/// Resets player health, armor, flash/shake effects, defuser state, and spawn position on round restart.
pub fn handle_round_reset_player_system(
    mut reset_events: EventReader<crate::entity::RoundResetEvent>,
    entities_lump: Option<Res<crate::studio::BspEntitiesLump>>,
    mut query: Query<&mut PlayerController>,
) {
    if reset_events.read().last().is_none() {
        return;
    }
    for mut player in &mut query {
        player.health = 100.0;
        player.armor = 100.0;
        player.flash_blind_alpha = 0.0;
        player.flash_hold_remaining = 0.0;
        player.flash_fade_duration = 0.0;
        player.flash_fade_elapsed = 0.0;
        player.screen_shake_timer = 0.0;
        player.screen_shake_amp = 0.0;
        player.anim_shoot_timer = 0.0;
        player.anim_shoot_retrigger = false;
        player.duck_visual_blend = 0.0;
        player.has_defuser = player.team == goldhub_game_cstrike::CsTeam::CounterTerrorist;
        if let Some(ref lump) = entities_lump
            && let Some((gs_origin, gs_angle)) = parse_player_spawn_for_team(&lump.0, player.team)
        {
            let origin = [gs_origin[0], gs_origin[1], gs_origin[2] + 18.0];
            player.sim.current.origin = origin;
            player.sim.previous.origin = origin;
            player.sim.current.velocity = [0.0; 3];
            player.sim.previous.velocity = [0.0; 3];
            player.sim.current.is_ducking = false;
            player.sim.current.in_duck = false;
            player.yaw = (gs_angle - 90.0).to_radians();
            player.pitch = 0.0;
        }
    }
}

/// Returns the CS 1.6 third-person player model path for the active team.
#[must_use]
pub const fn player_model_for_team(team: goldhub_game_cstrike::CsTeam) -> &'static str {
    match team {
        goldhub_game_cstrike::CsTeam::Terrorist => "models/player/terror/terror.mdl",
        goldhub_game_cstrike::CsTeam::CounterTerrorist
        | goldhub_game_cstrike::CsTeam::Unassigned
        | goldhub_game_cstrike::CsTeam::Spectator => "models/player/urban/urban.mdl",
    }
}

/// Marker component for the 3D player character model (rendered in 3rd person).
#[derive(Component, Debug)]
pub struct PlayerCharacterModel;

#[allow(clippy::too_many_arguments)]
fn spawn_player_character_model_system(
    mut commands: Commands,
    player_model_query: Query<Entity, With<PlayerCharacterModel>>,
    player_query: Query<(&PlayerController, &Transform)>,
    mut model_store: ResMut<crate::studio::StudioModelStore>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !player_model_query.is_empty() {
        return;
    }
    let Some(ref paths) = search_paths else {
        return;
    };
    let Ok((player, player_transform)) = player_query.get_single() else {
        return;
    };

    let model_path = player_model_for_team(player.team);
    let Some(model_data) = model_store.get_or_load(model_path, paths, &mut images, &mut materials)
    else {
        return;
    };

    let mut anim_component = crate::studio::StudioAnimatedModel::new(model_path, false);
    anim_component.set_sequence(1, 15.0, true); // initial idle1

    let parent = commands
        .spawn((
            PlayerCharacterModel,
            Transform::from_translation(player_transform.translation).with_rotation(
                Quat::from_rotation_y(player.yaw + std::f32::consts::FRAC_PI_2),
            ),
            Visibility::Hidden, // Hidden in 1st person by default
        ))
        .id();

    let mesh_children = crate::studio::store::spawn_model_submesh_children(
        &mut commands,
        parent,
        &model_data,
        1,
        false,
        Some(2),
        &mut meshes,
    );

    anim_component.mesh_entities = mesh_children;
    commands.entity(parent).insert(anim_component);
    tracing::info!(
        model_path,
        "Spawned PlayerCharacterModel for third-person view"
    );
}

/// Marker and state component for the weapon held by the player character in 3rd person.
#[derive(Component, Debug, Default)]
pub struct PlayerThirdPersonWeapon {
    pub current_weapon_id: Option<crate::combat::WeaponId>,
    pub model_id: String,
    pub mesh_entities: Vec<(Entity, Handle<Mesh>, usize)>,
}

fn resolve_player_upper_body_sequence(
    model_store: &crate::studio::StudioModelStore,
    normalized_id: &str,
    active_weapon: Option<&crate::combat::ActiveWeaponState>,
    shoot_anim_active: bool,
    is_crouching: bool,
) -> (usize, f32, bool) {
    let anim_ext = active_weapon.map_or("knife", |w| {
        crate::combat::get_weapon_def(w.id).anim_extension
    });
    let in_reload = active_weapon.is_some_and(|w| w.in_reload);

    let (action_prefix, fallback_prefix, looping) = if in_reload {
        (
            if is_crouching {
                "crouch_reload"
            } else {
                "ref_reload"
            },
            if is_crouching {
                "crouch_aim"
            } else {
                "ref_aim"
            },
            false,
        )
    } else if shoot_anim_active {
        (
            if is_crouching {
                "crouch_shoot"
            } else {
                "ref_shoot"
            },
            if is_crouching {
                "crouch_aim"
            } else {
                "ref_aim"
            },
            false,
        )
    } else {
        (
            if is_crouching {
                "crouch_aim"
            } else {
                "ref_aim"
            },
            "ref_aim",
            true,
        )
    };

    let target_seq_name = format!("{action_prefix}_{anim_ext}");
    let fallback_seq_name = format!("{fallback_prefix}_{anim_ext}");

    let (seq_idx, seq_fps) = model_store
        .models
        .get(normalized_id)
        .and_then(|m| {
            let seqs = &m.model.rig.sequences;
            let idx = seqs
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(&target_seq_name))
                .or_else(|| {
                    seqs.iter()
                        .position(|s| s.name.eq_ignore_ascii_case(&fallback_seq_name))
                })
                .or_else(|| {
                    seqs.iter()
                        .position(|s| s.name.eq_ignore_ascii_case("ref_aim_knife"))
                })?;
            Some((idx, seqs.get(idx).map_or(30.0, |s| s.fps)))
        })
        .unwrap_or((if is_crouching { 2 } else { 1 }, 15.0));

    (seq_idx, seq_fps, looping)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn update_player_character_model_system(
    mut commands: Commands,
    time: Res<Time>,
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    mut player_query: Query<(
        &mut PlayerController,
        Option<&crate::combat::ActiveWeaponState>,
        Option<&crate::combat::weapon::PlayerWeaponInventory>,
    )>,
    mut model_store: ResMut<crate::studio::StudioModelStore>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut model_query: Query<
        (
            Entity,
            &mut Transform,
            &mut crate::studio::StudioAnimatedModel,
            &mut Visibility,
        ),
        With<PlayerCharacterModel>,
    >,
    mut submesh_vis_query: Query<&mut Visibility, Without<PlayerCharacterModel>>,
) {
    let Ok((mut player, active_weapon, inventory)) = player_query.get_single_mut() else {
        return;
    };
    let Ok((parent_entity, mut transform, mut anim, mut visibility)) = model_query.get_single_mut()
    else {
        return;
    };
    let dt = time.delta_secs();

    // Hot-swap player skin (terror.mdl <-> urban.mdl) when team changes (F6)
    let desired_model = player_model_for_team(player.team);
    let desired_norm = desired_model.replace('\\', "/").to_lowercase();
    if anim.model_id != desired_norm
        && let Some(ref paths) = search_paths
        && let Some(model_data) =
            model_store.get_or_load(desired_model, paths, &mut images, &mut materials)
    {
        for &(child, ref mesh_handle, _) in &anim.mesh_entities {
            meshes.remove(mesh_handle);
            commands.entity(child).despawn_recursive();
        }
        let mesh_children = crate::studio::store::spawn_model_submesh_children(
            &mut commands,
            parent_entity,
            &model_data,
            1,
            false,
            Some(2),
            &mut meshes,
        );
        anim.model_id.clone_from(&desired_norm);
        anim.mesh_entities = mesh_children;
        anim.current_bone_transforms.clear();
        anim.prev_bone_transforms.clear();
        anim.blend_timer = 0.0;
        anim.sequence_index = usize::MAX;
        tracing::info!(model = desired_model, "Swapped third-person player model");
    }

    if *mode == CameraControlMode::Spectator {
        *visibility = Visibility::Hidden;
        return;
    }

    let (gs_pos, _) = player.sim.interpolate_for_render();
    let mut base_origin = Vec3::new(gs_pos[0], gs_pos[2], -gs_pos[1]);

    // Align StudioModel rest pose (+X) to Bevy camera forward (-Z)
    transform.rotation = Quat::from_rotation_y(player.yaw + std::f32::consts::FRAC_PI_2);

    // CS 1.6 StudioPlayerBlend pitch (blending[1]): 0 = looking +45° up, 128 = level, 255 = -45° down
    let pitch_deg = player.pitch.to_degrees();
    let pitch_blend = if pitch_deg <= -45.0 {
        255u8
    } else if pitch_deg >= 45.0 {
        0u8
    } else {
        ((45.0 - pitch_deg) * (255.0 / 90.0))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    anim.blending = [128, pitch_blend];

    let on_ground = player.sim.current.on_ground;
    let is_crouching = player.sim.current.is_ducking || player.sim.current.in_duck;
    if is_crouching {
        player.duck_visual_blend = (player.duck_visual_blend + dt / 0.2).min(1.0);
    } else {
        player.duck_visual_blend = (player.duck_visual_blend - dt / 0.2).max(0.0);
    }
    // Keep 3rd-person feet planted flush on the floor during both 0.4s duck wind-down and 0.2s uncrouch SLERP
    if on_ground
        && ((player.sim.current.in_duck && !player.sim.current.is_ducking)
            || (!is_crouching && player.sim.current.unduck_time_ms > 0.0))
    {
        base_origin.y -= 18.0 * player.duck_visual_blend;
    }
    let speed = player.sim.current.horizontal_speed();

    // 1. Lower-body locomotion gait sequence (idle1 / crouch_idle / walk / run / crouchrun / jump)
    let (gait_seq, gait_fps) = if !on_ground {
        if is_crouching {
            (2, 10.0) // crouch_idle tucks legs during crouch-jump without floating tilt
        } else {
            (6, 30.0) // jump
        }
    } else if speed > 10.0 {
        if is_crouching {
            (5, 30.0) // crouchrun
        } else if speed > 150.0 {
            (4, 60.0) // run
        } else {
            (3, 30.0) // walk
        }
    } else if is_crouching {
        (2, 10.0) // crouch_idle
    } else {
        (1, 15.0) // idle1
    };
    anim.set_gait_sequence(gait_seq, gait_fps);

    // 2. Upper-body weapon pose (split cleanly at Bip01 Spine in studio_animation_system)
    let arming_c4 = active_weapon.is_some_and(|w| w.c4_arming);
    let shoot_anim_active = arming_c4
        || player.anim_shoot_timer > 0.0
        || active_weapon
            .is_some_and(|w| !w.in_reload && w.shots_fired > 0 && w.next_primary_attack > 0.01);
    let retrigger = std::mem::take(&mut player.anim_shoot_retrigger);

    let (seq_idx, seq_fps, looping) = resolve_player_upper_body_sequence(
        &model_store,
        &desired_norm,
        active_weapon,
        shoot_anim_active,
        is_crouching,
    );

    if anim.sequence_index != seq_idx || retrigger || (looping && !anim.is_looping) {
        anim.set_sequence(seq_idx, seq_fps, looping);
    }

    // Toggle `backpack` bodygroup (submodel 1 = C4 bomb for T carrier / defuse kit for CT)
    let show_backpack = match player.team {
        goldhub_game_cstrike::CsTeam::Terrorist => inventory.is_some_and(|inv| {
            inv.slot5.id == Some(crate::combat::WeaponId::C4)
                || inv.bucket_weapons[4]
                    .iter()
                    .any(|s| s.id == Some(crate::combat::WeaponId::C4))
        }),
        goldhub_game_cstrike::CsTeam::CounterTerrorist => player.has_defuser,
        _ => false,
    };
    if let Some(model_data) = model_store.models.get(&desired_norm) {
        let active_sub = usize::from(show_backpack);
        for &(child, _, submesh_idx) in &anim.mesh_entities {
            if let Some(tpl) = model_data.submeshes.get(submesh_idx)
                && tpl.body_part_name.eq_ignore_ascii_case("backpack")
                && let Ok(mut vis) = submesh_vis_query.get_mut(child)
            {
                let target = if tpl.submodel_index == active_sub {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *vis != target {
                    *vis = target;
                }
            }
        }
    }

    if *perspective == CameraPerspective::ThirdPerson {
        transform.translation = base_origin;
        *visibility = Visibility::Inherited;
    } else {
        *visibility = Visibility::Hidden;
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
fn update_third_person_weapon_system(
    mut commands: Commands,
    perspective: Res<CameraPerspective>,
    player_query: Query<Option<&crate::combat::ActiveWeaponState>, With<PlayerController>>,
    model_parent_query: Query<
        (Entity, &crate::studio::StudioAnimatedModel),
        With<PlayerCharacterModel>,
    >,
    mut weapon_query: Query<(Entity, &mut PlayerThirdPersonWeapon)>,
    mut visibility_query: Query<&mut Visibility, Without<PlayerCharacterModel>>,
    mut model_store: ResMut<crate::studio::StudioModelStore>,
    search_paths: Option<Res<crate::audio::GameSearchPaths>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(active_weapon) = player_query.get_single() else {
        return;
    };
    let Ok((parent_entity, player_anim)) = model_parent_query.get_single() else {
        return;
    };
    let Some(ref paths) = search_paths else {
        return;
    };

    let current_id = active_weapon.map_or(crate::combat::WeaponId::Knife, |w| w.id);
    let def = crate::combat::get_weapon_def(current_id);

    // 1. Ensure weapon tracker component exists
    let Ok((_, mut weapon_data)) = weapon_query.get_single_mut() else {
        let wid = commands
            .spawn((
                PlayerThirdPersonWeapon::default(),
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        commands.entity(parent_entity).add_child(wid);
        return;
    };

    // 2. Check if weapon model needs switching
    if weapon_data.current_weapon_id != Some(current_id) {
        for &(child, ref mesh_handle, _) in &weapon_data.mesh_entities {
            meshes.remove(mesh_handle);
            commands.entity(child).despawn_recursive();
        }
        weapon_data.mesh_entities.clear();
        weapon_data.current_weapon_id = Some(current_id);
        weapon_data.model_id = def.p_model.to_string();

        if let Some(model_data) =
            model_store.get_or_load(def.p_model, paths, &mut images, &mut materials)
        {
            weapon_data.mesh_entities = crate::studio::store::spawn_model_submesh_children(
                &mut commands,
                parent_entity,
                &model_data,
                0,
                false,
                Some(2),
                &mut meshes,
            );
        }
    }

    // 3. Update visibility and bone-name attachment against active player model (terror.mdl / urban.mdl)
    let is_tp = *perspective == CameraPerspective::ThirdPerson;
    let player_model_norm = player_anim.model_id.replace('\\', "/").to_lowercase();
    let player_bones = model_store
        .models
        .get(&player_model_norm)
        .map(|m| &m.model.rig.bones);

    for &(child, ref mesh_handle, submesh_idx) in &weapon_data.mesh_entities {
        if let Ok(mut vis) = visibility_query.get_mut(child) {
            let target_vis = if is_tp {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != target_vis {
                *vis = target_vis;
            }
        }

        if !is_tp {
            continue;
        }

        let normalized_id = def.p_model.replace('\\', "/").to_lowercase();
        let Some(p_model_data) = model_store.models.get(&normalized_id) else {
            continue;
        };

        let char_bones = &player_anim.current_bone_transforms;
        if char_bones.is_empty() {
            continue;
        }

        let p_bones = &p_model_data.model.rig.bones;
        let mut merged_matrices = Vec::with_capacity(p_bones.len());
        for b in p_bones {
            let matched_mat = player_bones.and_then(|pb_list| {
                pb_list
                    .iter()
                    .position(|pb| pb.name.eq_ignore_ascii_case(&b.name))
                    .and_then(|idx| char_bones.get(idx).copied())
            });

            if let Some(m) = matched_mat {
                merged_matrices.push(m);
            } else if b.parent_index >= 0 && (b.parent_index as usize) < merged_matrices.len() {
                let parent_m = merged_matrices[b.parent_index as usize];
                let local_m = Mat4::from_cols_array(&b.local_matrix);
                merged_matrices.push(parent_m * local_m);
            } else {
                merged_matrices.push(Mat4::from_cols_array(&b.local_matrix));
            }
        }

        let Some(template) = p_model_data.submeshes.get(submesh_idx) else {
            continue;
        };
        let Some(mesh) = meshes.get_mut(mesh_handle) else {
            continue;
        };
        let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(pos_vec)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };

        for (v, out_p) in template.rest_vertices.iter().zip(pos_vec.iter_mut()) {
            let bone_idx = v.bone_index as usize;
            if bone_idx < merged_matrices.len() {
                let bm = &merged_matrices[bone_idx];
                let p = bm.transform_point3(Vec3::from(v.position));
                *out_p = [p.x, p.z, -p.y];
            }
        }
        if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(norm_vec)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
        {
            for (v, out_n) in template.rest_vertices.iter().zip(norm_vec.iter_mut()) {
                let bone_idx = v.bone_index as usize;
                if bone_idx < merged_matrices.len() {
                    let bm = &merged_matrices[bone_idx];
                    let n = bm.transform_vector3(Vec3::from(v.normal));
                    *out_n = [n.x, n.z, -n.y];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flash_fade_hold_stacking_and_decay() {
        let mut pc = PlayerController::default();
        assert!((pc.current_flash_fraction() - 0.0).abs() < 1e-6);

        pc.apply_flash(255.0, 2.0, 1.0);
        assert!((pc.current_flash_fraction() - 1.0).abs() < 1e-6);

        // Stacking full flash extends hold duration
        pc.apply_flash(255.0, 2.0, 0.5);
        assert!((pc.flash_hold_remaining - 1.5).abs() < 1e-6);

        pc.tick_effects(1.5);
        assert!((pc.current_flash_fraction() - 1.0).abs() < 1e-5);

        pc.tick_effects(1.0);
        assert!((pc.current_flash_fraction() - 0.5).abs() < 1e-5);

        pc.tick_effects(1.0);
        assert!((pc.current_flash_fraction() - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_kevlar_blast_damage_attenuation() {
        let mut pc = PlayerController::default();

        // 80 raw HE damage -> 40 HP taken, 20 armor consumed
        pc.apply_blast_damage(80.0);
        assert!((pc.health - 60.0).abs() < 1e-4);
        assert!((pc.armor - 80.0).abs() < 1e-4);
    }

    #[test]
    fn test_team_player_model_paths() {
        assert_eq!(
            player_model_for_team(goldhub_game_cstrike::CsTeam::Terrorist),
            "models/player/terror/terror.mdl"
        );
        assert_eq!(
            player_model_for_team(goldhub_game_cstrike::CsTeam::CounterTerrorist),
            "models/player/urban/urban.mdl"
        );
    }
}
