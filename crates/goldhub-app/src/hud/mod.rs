//! Classic Counter-Strike 1.6 HUD (Health, Armor, Ammo, Timer, and Dynamic Crosshair).

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{SpriteData, SpriteDecoder};
use std::fs;

use crate::audio::GameSearchPaths;
use crate::combat::recoil::PlayerRecoil;
use crate::combat::weapon::{ActiveWeaponState, WeaponId};
use crate::player::{CameraControlMode, CameraPerspective, PlayerController};
use crate::sprite::resolve_file_path;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_cs_hud);
        app.add_systems(
            Update,
            (
                update_hud_health_armor_system,
                update_hud_ammo_system,
                update_hud_flashlight_system,
                update_dynamic_crosshair_system,
            ),
        );
    }
}

/// Authentic CS 1.6 sliced amber sprite glyph atlas from `sprites/640hud7.spr`.
#[derive(Resource, Clone)]
pub struct HudAtlas {
    pub digits: [Handle<Image>; 10],
    pub divider: Handle<Image>,
    pub cross: Handle<Image>,
    pub suit_full: Handle<Image>,
    pub flashlight: Handle<Image>,
}

#[derive(Component)]
pub struct HudFlashlightIcon;

#[derive(Component)]
pub struct HudHealthDigit(pub usize);

#[derive(Component)]
pub struct HudArmorDigit(pub usize);

#[derive(Component)]
pub struct HudClipDigit(pub usize);

#[derive(Component)]
pub struct HudReserveDigit(pub usize);

#[derive(Component)]
pub struct HudAmmoRoot;

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

fn extract_sub_image(source: &[u8], src_width: u32, x: u32, y: u32, w: u32, h: u32) -> Image {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let src_row = y + row;
        let start = ((src_row * src_width + x) * 4) as usize;
        let end = start + (w * 4) as usize;
        data.extend_from_slice(&source[start..end]);
    }
    Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Builds the classic CS 1.6 HUD layout with authentic amber sprites from `640hud7.spr`.
#[allow(clippy::too_many_lines)]
fn setup_cs_hud(
    mut commands: Commands,
    search_paths: Res<GameSearchPaths>,
    mut images: ResMut<Assets<Image>>,
) {
    let atlas_opt = (|| {
        let path = resolve_file_path("sprites/640hud7.spr", &search_paths)?;
        let raw = fs::read(path).ok()?;
        let sprite: SpriteData = SpriteDecoder.decode(&raw).ok()?;
        let f0 = sprite.frames.first()?;
        let mut amber_rgba = Vec::with_capacity(f0.rgba_pixels.len());
        for chunk in f0.rgba_pixels.as_chunks::<4>().0 {
            let lum = chunk[0].max(chunk[1]).max(chunk[2]);
            let r = lum;
            let g = ((u16::from(lum) * 160) / 255) as u8;
            let b = 0;
            let a = lum;
            amber_rgba.extend_from_slice(&[r, g, b, a]);
        }
        let coords: [(u32, u32); 10] = [
            (0, 0),
            (23, 0),
            (47, 0),
            (70, 0),
            (95, 0),
            (119, 0),
            (144, 0),
            (169, 0),
            (192, 0),
            (216, 0),
        ];
        let mut digits = Vec::with_capacity(10);
        for &(cx, cy) in &coords {
            let img = extract_sub_image(&amber_rgba, 256, cx, cy, 20, 25);
            digits.push(images.add(img));
        }
        let digits: [Handle<Image>; 10] = digits.try_into().ok()?;
        let div_img = extract_sub_image(&amber_rgba, 256, 240, 0, 2, 25);
        let divider = images.add(div_img);
        let cross_img = extract_sub_image(&amber_rgba, 256, 48, 25, 24, 24);
        let cross = images.add(cross_img);
        let suit_img = extract_sub_image(&amber_rgba, 256, 0, 25, 24, 24);
        let suit_full = images.add(suit_img);
        let flash_img = extract_sub_image(&amber_rgba, 256, 144, 25, 48, 32);
        let flashlight = images.add(flash_img);
        Some(HudAtlas {
            digits,
            divider,
            cross,
            suit_full,
            flashlight,
        })
    })();

    if let Some(atlas) = atlas_opt {
        commands.insert_resource(atlas.clone());

        // 1. Bottom-Left: Health Row (Cross + 3 digits)
        commands
            .spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(48.0),
                left: Val::Px(32.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(atlas.cross.clone()),
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        ..default()
                    },
                ));
                for i in 0..3 {
                    let d = usize::from(i == 0);
                    parent.spawn((
                        HudHealthDigit(i),
                        ImageNode::new(atlas.digits[d].clone()),
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(25.0),
                            ..default()
                        },
                    ));
                }
            });

        // 2. Bottom-Left: Armor Row (Suit icon + 3 digits)
        commands
            .spawn(Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(48.0),
                left: Val::Px(150.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(2.0),
                ..default()
            })
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(atlas.suit_full.clone()),
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        ..default()
                    },
                ));
                for i in 0..3 {
                    let d = usize::from(i == 0);
                    parent.spawn((
                        HudArmorDigit(i),
                        ImageNode::new(atlas.digits[d].clone()),
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(25.0),
                            ..default()
                        },
                    ));
                }
            });

        // 3. Bottom-Right: Ammo Row (2 clip digits + divider + 3 reserve digits)
        commands
            .spawn((
                HudAmmoRoot,
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(48.0),
                    right: Val::Px(48.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(3.0),
                    ..default()
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    HudClipDigit(0),
                    ImageNode::new(atlas.digits[3].clone()),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudClipDigit(1),
                    ImageNode::new(atlas.digits[0].clone()),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    ImageNode::new(atlas.divider.clone()),
                    Node {
                        width: Val::Px(2.0),
                        height: Val::Px(25.0),
                        margin: UiRect::horizontal(Val::Px(4.0)),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudReserveDigit(0),
                    ImageNode::new(atlas.digits[0].clone()),
                    Visibility::Hidden,
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudReserveDigit(1),
                    ImageNode::new(atlas.digits[9].clone()),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudReserveDigit(2),
                    ImageNode::new(atlas.digits[0].clone()),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
            });

        // 3. Flashlight Indicator Icon (visible when flashlight is toggled on)
        commands.spawn((
            HudFlashlightIcon,
            ImageNode::new(atlas.flashlight),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(48.0),
                right: Val::Px(240.0),
                width: Val::Px(48.0),
                height: Val::Px(32.0),
                ..default()
            },
            Visibility::Hidden,
        ));
    }

    // 4. Dynamic Crosshair Root & 4 Lines
    // Fullscreen centered flexbox ensures crosshair is always at the true screen center
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

    let center_box = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            width: Val::Px(0.0),
            height: Val::Px(0.0),
            ..default()
        })
        .id();

    let green = Color::srgb(0.0, 1.0, 0.0);
    let half_thick = Val::Px(-CROSSHAIR_LINE_THICK * 0.5);

    // Top
    let top = commands
        .spawn((
            CrosshairTop,
            BackgroundColor(green),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(CROSSHAIR_LINE_THICK),
                height: Val::Px(CROSSHAIR_LINE_LEN),
                left: half_thick,
                top: Val::Px(-6.0 - CROSSHAIR_LINE_LEN),
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
                left: half_thick,
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
                left: Val::Px(-6.0 - CROSSHAIR_LINE_LEN),
                top: half_thick,
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
                top: half_thick,
                ..default()
            },
        ))
        .id();

    commands
        .entity(center_box)
        .add_children(&[top, bottom, left, right]);
    commands.entity(crosshair_root).add_child(center_box);
}

fn update_hud_health_armor_system(
    atlas_opt: Option<Res<HudAtlas>>,
    player_query: Query<&PlayerController>,
    mut hp_digits: Query<(&HudHealthDigit, &mut ImageNode), Without<HudArmorDigit>>,
    mut armor_digits: Query<(&HudArmorDigit, &mut ImageNode), Without<HudHealthDigit>>,
) {
    let Some(atlas) = atlas_opt else {
        return;
    };
    if let Ok(player) = player_query.get_single() {
        let hp = (player.health.round() as usize).min(999);
        let hp_d0 = (hp / 100) % 10;
        let hp_d1 = (hp / 10) % 10;
        let hp_d2 = hp % 10;
        for (digit, mut img) in &mut hp_digits {
            let d = match digit.0 {
                0 => hp_d0,
                1 => hp_d1,
                _ => hp_d2,
            };
            img.image = atlas.digits[d].clone();
        }
        for (digit, mut img) in &mut armor_digits {
            let d = usize::from(digit.0 == 0);
            img.image = atlas.digits[d].clone();
        }
    }
}

fn update_hud_flashlight_system(
    fl_query: Query<&crate::player::PlayerFlashlight>,
    mut icon_query: Query<&mut Visibility, With<HudFlashlightIcon>>,
) {
    let is_on = fl_query.get_single().is_ok_and(|fl| fl.is_on);
    for mut vis in &mut icon_query {
        let target = if is_on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target {
            *vis = target;
        }
    }
}

fn update_hud_ammo_system(
    atlas_opt: Option<Res<HudAtlas>>,
    weapon_query: Query<&ActiveWeaponState>,
    mut root_query: Query<&mut Visibility, With<HudAmmoRoot>>,
    mut clip_query: Query<
        (&HudClipDigit, &mut ImageNode, &mut Visibility),
        (Without<HudReserveDigit>, Without<HudAmmoRoot>),
    >,
    mut res_query: Query<
        (&HudReserveDigit, &mut ImageNode, &mut Visibility),
        (Without<HudClipDigit>, Without<HudAmmoRoot>),
    >,
) {
    let Ok(state) = weapon_query.get_single() else {
        return;
    };
    let Some(atlas) = atlas_opt else {
        return;
    };

    if state.id == WeaponId::Knife {
        for mut vis in &mut root_query {
            *vis = Visibility::Hidden;
        }
        return;
    }
    for mut vis in &mut root_query {
        *vis = Visibility::Inherited;
    }

    // Clip (up to 2 digits)
    let clip = state.clip.clamp(0, 99) as usize;
    let d0 = clip / 10;
    let d1 = clip % 10;
    for (digit, mut img, mut vis) in &mut clip_query {
        if digit.0 == 0 {
            if d0 > 0 {
                img.image = atlas.digits[d0].clone();
                *vis = Visibility::Inherited;
            } else {
                *vis = Visibility::Hidden;
            }
        } else {
            img.image = atlas.digits[d1].clone();
            *vis = Visibility::Inherited;
        }
    }

    // Reserve (up to 3 digits)
    let res = state.reserve.clamp(0, 999) as usize;
    let r0 = res / 100;
    let r1 = (res / 10) % 10;
    let r2 = res % 10;
    for (digit, mut img, mut vis) in &mut res_query {
        match digit.0 {
            0 => {
                if r0 > 0 {
                    img.image = atlas.digits[r0].clone();
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
            1 => {
                if r0 > 0 || r1 > 0 {
                    img.image = atlas.digits[r1].clone();
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
            _ => {
                img.image = atlas.digits[r2].clone();
                *vis = Visibility::Inherited;
            }
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
    mut top_query: Query<
        &mut Node,
        (
            With<CrosshairTop>,
            Without<CrosshairBottom>,
            Without<CrosshairLeft>,
            Without<CrosshairRight>,
        ),
    >,
    mut bottom_query: Query<
        &mut Node,
        (
            With<CrosshairBottom>,
            Without<CrosshairTop>,
            Without<CrosshairLeft>,
            Without<CrosshairRight>,
        ),
    >,
    mut left_query: Query<
        &mut Node,
        (
            With<CrosshairLeft>,
            Without<CrosshairTop>,
            Without<CrosshairBottom>,
            Without<CrosshairRight>,
        ),
    >,
    mut right_query: Query<
        &mut Node,
        (
            With<CrosshairRight>,
            Without<CrosshairTop>,
            Without<CrosshairBottom>,
            Without<CrosshairLeft>,
        ),
    >,
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
    let speed_gap = if on_ground {
        (speed * 0.04).min(14.0)
    } else {
        16.0
    };
    let recoil_gap = (recoil.punch_pitch.abs() * 8.0).min(20.0);
    let total_gap = (base_gap + speed_gap + recoil_gap).round();
    let half_thick = Val::Px(-CROSSHAIR_LINE_THICK * 0.5);

    for mut n in &mut top_query {
        n.top = Val::Px(-total_gap - CROSSHAIR_LINE_LEN);
        n.left = half_thick;
    }
    for mut n in &mut bottom_query {
        n.top = Val::Px(total_gap);
        n.left = half_thick;
    }
    for mut n in &mut left_query {
        n.left = Val::Px(-total_gap - CROSSHAIR_LINE_LEN);
        n.top = half_thick;
    }
    for mut n in &mut right_query {
        n.left = Val::Px(total_gap);
        n.top = half_thick;
    }
}
