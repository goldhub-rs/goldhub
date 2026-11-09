//! Classic Counter-Strike 1.6 HUD (Health, Armor, Ammo, Timer, and Dynamic Crosshair).

use bevy::prelude::*;
use crate::combat::recoil::PlayerRecoil;
use crate::combat::weapon::{ActiveWeaponState, WeaponId};
use crate::player::{CameraControlMode, CameraPerspective, PlayerController};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_cs_hud);
        app.add_systems(
            Update,
            (
                update_hud_health_armor_system,
                update_hud_ammo_system,
                update_dynamic_crosshair_system,
            ),
        );
    }
}

#[derive(Component)]
pub struct HudHealthText;

#[derive(Component)]
pub struct HudArmorText;

#[derive(Component)]
pub struct HudAmmoText;

#[derive(Component)]
pub struct CrosshairTop;

#[derive(Component)]
pub struct CrosshairBottom;

#[derive(Component)]
pub struct CrosshairLeft;

#[derive(Component)]
pub struct CrosshairRight;

#[derive(Component)]
pub struct CrosshairRoot;

const CROSSHAIR_LINE_LEN: f32 = 9.0;
const CROSSHAIR_LINE_THICK: f32 = 2.0;

/// Builds the classic CS 1.6 HUD layout.
#[allow(clippy::too_many_lines)]
fn setup_cs_hud(mut commands: Commands) {
    // 1. Bottom-Left: Health & Armor
    commands.spawn((
        HudHealthText,
        Text::new("+ 100"),
        TextFont {
            font_size: 26.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.72, 0.0)), // Iconic GoldSrc HUD amber/orange
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(56.0),
            left: Val::Px(32.0),
            ..default()
        },
    ));

    commands.spawn((
        HudArmorText,
        Text::new("🛡 100"),
        TextFont {
            font_size: 26.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.72, 0.0)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(56.0),
            left: Val::Px(150.0),
            ..default()
        },
    ));

    // 2. Bottom-Right: Ammo Counter
    commands.spawn((
        HudAmmoText,
        Text::new("30 | 90"),
        TextFont {
            font_size: 28.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.72, 0.0)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(56.0),
            right: Val::Px(48.0),
            ..default()
        },
    ));

    // 3. Dynamic Crosshair Root & 4 Lines
    let crosshair_root = commands
        .spawn((
            CrosshairRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();

    let green = Color::srgb(0.0, 1.0, 0.0);

    // Top
    let top = commands
        .spawn((
            CrosshairTop,
            BackgroundColor(green),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(CROSSHAIR_LINE_THICK),
                height: Val::Px(CROSSHAIR_LINE_LEN),
                bottom: Val::Px(6.0),
                ..default()
            },
        ))
        .id();

    // Bottom
    let bottom = commands
        .spawn((
            CrosshairBottom,
            BackgroundColor(green),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(CROSSHAIR_LINE_THICK),
                height: Val::Px(CROSSHAIR_LINE_LEN),
                top: Val::Px(6.0),
                ..default()
            },
        ))
        .id();

    // Left
    let left = commands
        .spawn((
            CrosshairLeft,
            BackgroundColor(green),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(CROSSHAIR_LINE_LEN),
                height: Val::Px(CROSSHAIR_LINE_THICK),
                right: Val::Px(6.0),
                ..default()
            },
        ))
        .id();

    // Right
    let right = commands
        .spawn((
            CrosshairRight,
            BackgroundColor(green),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(CROSSHAIR_LINE_LEN),
                height: Val::Px(CROSSHAIR_LINE_THICK),
                left: Val::Px(6.0),
                ..default()
            },
        ))
        .id();

    commands
        .entity(crosshair_root)
        .add_children(&[top, bottom, left, right]);
}

fn update_hud_health_armor_system(
    player_query: Query<&PlayerController>,
    mut hp_query: Query<&mut Text, (With<HudHealthText>, Without<HudArmorText>)>,
    mut armor_query: Query<&mut Text, (With<HudArmorText>, Without<HudHealthText>)>,
) {
    if let Ok(_player) = player_query.get_single() {
        for mut text in &mut hp_query {
            **text = "+ 100".to_string();
        }
        for mut text in &mut armor_query {
            **text = "🛡 100".to_string();
        }
    }
}

fn update_hud_ammo_system(
    weapon_query: Query<&ActiveWeaponState>,
    mut ammo_query: Query<&mut Text, With<HudAmmoText>>,
) {
    let Ok(state) = weapon_query.get_single() else {
        return;
    };
    for mut text in &mut ammo_query {
        if state.id == WeaponId::Knife {
            **text = String::new();
        } else {
            **text = format!("{} | {}", state.clip, state.reserve);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_dynamic_crosshair_system(
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    player_query: Query<&PlayerController>,
    weapon_query: Query<(&ActiveWeaponState, &PlayerRecoil)>,
    mut root_query: Query<&mut Visibility, With<CrosshairRoot>>,
    mut top_query: Query<&mut Node, (With<CrosshairTop>, Without<CrosshairBottom>, Without<CrosshairLeft>, Without<CrosshairRight>)>,
    mut bottom_query: Query<&mut Node, (With<CrosshairBottom>, Without<CrosshairTop>, Without<CrosshairLeft>, Without<CrosshairRight>)>,
    mut left_query: Query<&mut Node, (With<CrosshairLeft>, Without<CrosshairTop>, Without<CrosshairBottom>, Without<CrosshairRight>)>,
    mut right_query: Query<&mut Node, (With<CrosshairRight>, Without<CrosshairTop>, Without<CrosshairBottom>, Without<CrosshairLeft>)>,
) {
    let Ok(mut root_vis) = root_query.get_single_mut() else {
        return;
    };

    if *mode != CameraControlMode::Player || *perspective == CameraPerspective::ThirdPerson {
        *root_vis = Visibility::Hidden;
        return;
    }
    *root_vis = Visibility::Inherited;

    let Ok(player) = player_query.get_single() else {
        return;
    };
    let default_state = ActiveWeaponState::default();
    let default_recoil = PlayerRecoil::default();
    let (state, recoil) = weapon_query
        .get_single()
        .unwrap_or((&default_state, &default_recoil));

    // Hide crosshair for scoped AWP when unzoomed, or show knife crosshair
    if state.id == WeaponId::AWP && state.zoom_level == 0 {
        *root_vis = Visibility::Hidden;
        return;
    }

    // Dynamic crosshair spread expansion matching CS 1.6 cl_dynamiccrosshair
    let speed = player.sim.current.horizontal_speed();
    let on_ground = player.sim.current.on_ground;
    let is_ducking = player.sim.current.is_ducking;

    let base_gap = if is_ducking { 4.0 } else { 6.0 };
    let speed_gap = if on_ground { (speed * 0.04).min(14.0) } else { 16.0 };
    let recoil_gap = (recoil.punch_pitch.abs() * 8.0).min(20.0);
    let total_gap = (base_gap + speed_gap + recoil_gap).round();

    for mut n in &mut top_query {
        n.bottom = Val::Px(total_gap);
    }
    for mut n in &mut bottom_query {
        n.top = Val::Px(total_gap);
    }
    for mut n in &mut left_query {
        n.right = Val::Px(total_gap);
    }
    for mut n in &mut right_query {
        n.left = Val::Px(total_gap);
    }
}
