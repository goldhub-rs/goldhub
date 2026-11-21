//! Classic Counter-Strike 1.6 HUD Radar (`CHudRadar` parity).
//!
//! Decoupled SPI design:
//! - `RadarMarker` + `RadarDialBuffer` implement the game-agnostic 2D radial projection
//!   using `goldhub_engine_api::ir::{RadarColor, RadarGlyphKind, RadarMarkerIr}`.
//! - `cs_gather_radar_markers_system` translates CS 1.6 entities (hostages, dropped/carried C4,
//!   teammates) into canonical `RadarMarkerIr` records without coupling rendering to game rules.

use std::fs;

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_engine_api::ir::{RadarColor, RadarGlyphKind, RadarMarkerIr};
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::SpriteDecoder;

use crate::audio::GameSearchPaths;
use crate::combat::weapon::{DroppedWeapon, GrenadeProjectile, PlantedC4, WeaponId};
use crate::player::{CameraControlMode, PlayerController};
use crate::studio::world_entity::HostageEntity;
use goldhub_game_cstrike::CsTeam;

/// Canonical 640x480 HUD sprite radius (`128 / 2 = 64 px`, `radar.cpp:172`).
pub const RADAR_RADIUS_PX: f32 = 64.0;
/// Diameter of the CS 1.6 radar dial sprite (`radar640.spr`).
pub const RADAR_SIZE_PX: f32 = RADAR_RADIUS_PX * 2.0;
/// Default `cl_radar_scale` world units per radar pixel (`radar.cpp:113`).
pub const RADAR_SCALE_UNITS_PER_PX: f32 = 32.0;
/// Elevation differential threshold in GoldSrc units (`radar.cpp:239`).
pub const RADAR_Z_THRESHOLD: f32 = 128.0;
/// Maximum pre-allocated zero-alloc blip nodes rendered simultaneously.
pub const MAX_RADAR_BLIPS: usize = 32;

const DOT_COLOR_HOSTAGE: RadarColor = RadarColor::rgba(70, 145, 255, 240);
const DOT_COLOR_HOSTAGE_DEAD: RadarColor = RadarColor::rgba(255, 45, 35, 255);
const DOT_COLOR_BOMB: RadarColor = RadarColor::rgba(255, 35, 25, 255);

/// Resource storing baked 8x8 glyph textures matching `radar.cpp` pixel masks.
#[derive(Resource, Clone)]
pub struct RadarAtlas {
    pub dial_texture: Handle<Image>,
    pub dot: Handle<Image>,
    pub above_t: Handle<Image>,
    pub below_t: Handle<Image>,
    pub cross: Handle<Image>,
}

/// Zero-allocation reusable buffer cleared and populated every frame in Presentation phase.
#[derive(Resource, Default)]
pub struct RadarDialBuffer {
    pub markers: Vec<RadarMarkerIr>,
}

/// Marker component for the 128x128 top-left radar dial root node.
#[derive(Component)]
pub struct HudRadarRoot;

/// Marker component indexing a pre-allocated slot in the radar blip pool.
#[derive(Component)]
pub struct HudRadarBlip(pub usize);

/// Registers the classic 1-to-1 CS 1.6 radar systems and pre-allocated blip pool.
pub struct CsRadarPlugin;

impl Plugin for CsRadarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RadarDialBuffer>();
        app.add_systems(Startup, setup_classic_radar_system);
        app.add_systems(
            Update,
            (
                cs_gather_radar_markers_system,
                update_radar_blips_system.after(cs_gather_radar_markers_system),
            ),
        );
    }
}

/// Evaluates CS 1.6 `WorldToRadar` (`cl_dll/radar.cpp:668-695`).
///
/// Accepts GoldSrc world coordinates `(X=Right, Y=Forward, Z=Up)` and observer yaw in degrees
/// matching GoldSrc conventions (0° = +X East, 90° = +Y North).
///
/// Returns `(radar_offset_x, radar_offset_y, delta_z_player_minus_target)` where
/// `radar_offset_x/y` are centered around `(0, 0)` within `[-RADAR_RADIUS_PX, +RADAR_RADIUS_PX]`.
#[must_use]
pub fn world_to_radar(
    player_gs: [f32; 3],
    target_gs: [f32; 3],
    player_yaw_deg: f32,
    radar_scale: f32,
) -> (f32, f32, f32) {
    let mut dx = target_gs[0] - player_gs[0];
    let mut dy = target_gs[1] - player_gs[1];

    if dx == 0.0 {
        dx = 1e-5;
    }
    if dy == 0.0 {
        dy = 1e-5;
    }

    let bearing_deg = dy.atan2(dx).to_degrees();
    let offset_rad = (player_yaw_deg - bearing_deg).to_radians();

    let clamped_scale = radar_scale.clamp(4.0, 128.0);
    let dist_2d = dx.hypot(dy);
    let radius = (dist_2d / clamped_scale).min(RADAR_RADIUS_PX - 4.0);

    let rx = radius * offset_rad.sin();
    let ry = radius * (-offset_rad.cos());
    let delta_z = player_gs[2] - target_gs[2];

    (rx, ry, delta_z)
}

/// Selects the canonical CS 1.6 Z-axis glyph (`DrawZAxis`, `radar.cpp:237-255`).
#[must_use]
pub fn resolve_radar_glyph(delta_z: f32, force_glyph: Option<RadarGlyphKind>) -> RadarGlyphKind {
    if let Some(forced) = force_glyph {
        return forced;
    }
    if delta_z <= -RADAR_Z_THRESHOLD {
        RadarGlyphKind::AboveT
    } else if delta_z >= RADAR_Z_THRESHOLD {
        RadarGlyphKind::BelowFlippedT
    } else {
        RadarGlyphKind::Dot
    }
}

/// Generates an 8x8 RGBA texture from an 8x8 binary mask matching `radar.cpp` shapes.
fn bake_mask_image(mask: [[u8; 8]; 8]) -> Image {
    let mut rgba = Vec::with_capacity(8 * 8 * 4);
    for row in mask {
        for cell in row {
            if cell != 0 {
                rgba.extend_from_slice(&[255, 255, 255, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Image::new(
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Loads `sprites/radar640.spr` and turns intensity into additive green disc alpha,
/// falling back cleanly to a synthesized 128x128 GoldSrc dial if the sprite file is missing.
fn build_radar_dial_image(search_paths: &GameSearchPaths) -> Image {
    if let Some(path) = crate::sprite::resolve_file_path("sprites/radar640.spr", search_paths)
        && let Ok(raw) = fs::read(path)
        && let Ok(sprite) = SpriteDecoder.decode(&raw)
        && let Some(f0) = sprite.frames.first()
    {
        let mut tinted = Vec::with_capacity(f0.rgba_pixels.len());
        for chunk in f0.rgba_pixels.as_chunks::<4>().0 {
            let lum = chunk[0].max(chunk[1]).max(chunk[2]);
            let r = ((u16::from(lum) * 45) / 255) as u8;
            let g = ((u16::from(lum) * 185) / 255) as u8;
            let b = ((u16::from(lum) * 45) / 255) as u8;
            let a = if lum > 8 { lum.max(150) } else { 0 };
            tinted.extend_from_slice(&[r, g, b, a]);
        }
        return Image::new(
            Extent3d {
                width: f0.width,
                height: f0.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            tinted,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
    }

    // Fallback synthesized 128x128 CS 1.6 circular green ring + crosshairs
    let size = RADAR_SIZE_PX as usize;
    let center = RADAR_RADIUS_PX - 0.5;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let r = dx.hypot(dy);
            let idx = (y * size + x) * 4;
            if (r - 61.5).abs() <= 1.2 {
                // Bright outer ring
                rgba[idx..idx + 4].copy_from_slice(&[35, 150, 35, 210]);
            } else if r < 61.5 {
                let on_crosshair = dx.abs() < 0.7 || dy.abs() < 0.7;
                if on_crosshair && r > 6.0 {
                    rgba[idx..idx + 4].copy_from_slice(&[30, 115, 30, 120]);
                } else {
                    rgba[idx..idx + 4].copy_from_slice(&[15, 55, 15, 95]);
                }
            }
        }
    }
    Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn setup_classic_radar_system(
    mut commands: Commands,
    search_paths: Res<GameSearchPaths>,
    mut images: ResMut<Assets<Image>>,
) {
    let dial_texture = images.add(build_radar_dial_image(&search_paths));

    // Exact 4x4 centered dot (`DrawRadarDot`, radar.cpp:638-640)
    let dot_mask = [
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 1, 1, 1, 1, 0, 0],
        [0, 0, 1, 1, 1, 1, 0, 0],
        [0, 0, 1, 1, 1, 1, 0, 0],
        [0, 0, 1, 1, 1, 1, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 0, 0, 0],
    ];
    // `r_RadarT` (`radar.cpp:56-66`)
    let above_t_mask = [
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
    ];
    // `r_RadarFlippedT` (`radar.cpp:68-78`)
    let below_t_mask = [
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [0, 0, 0, 1, 1, 0, 0, 0],
        [1, 1, 1, 1, 1, 1, 1, 1],
        [1, 1, 1, 1, 1, 1, 1, 1],
    ];
    // `r_RadarCross` (`radar.cpp:44-54`)
    let cross_mask = [
        [1, 1, 0, 0, 0, 0, 1, 1],
        [1, 1, 1, 0, 0, 1, 1, 1],
        [0, 1, 1, 1, 1, 1, 1, 0],
        [0, 0, 1, 1, 1, 1, 0, 0],
        [0, 0, 1, 1, 1, 1, 0, 0],
        [0, 1, 1, 1, 1, 1, 1, 0],
        [1, 1, 1, 0, 0, 1, 1, 1],
        [1, 1, 0, 0, 0, 0, 1, 1],
    ];

    let atlas = RadarAtlas {
        dial_texture: dial_texture.clone(),
        dot: images.add(bake_mask_image(dot_mask)),
        above_t: images.add(bake_mask_image(above_t_mask)),
        below_t: images.add(bake_mask_image(below_t_mask)),
        cross: images.add(bake_mask_image(cross_mask)),
    };
    commands.insert_resource(atlas.clone());

    commands
        .spawn((
            HudRadarRoot,
            ImageNode::new(dial_texture),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(16.0),
                left: Val::Px(16.0),
                width: Val::Px(RADAR_SIZE_PX),
                height: Val::Px(RADAR_SIZE_PX),
                ..default()
            },
        ))
        .with_children(|dial| {
            // Local player center marker (small subtle cross center pip)
            dial.spawn((
                ImageNode::new(atlas.dot.clone()).with_color(Color::srgba(0.65, 1.0, 0.65, 0.75)),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(RADAR_RADIUS_PX - 4.0),
                    top: Val::Px(RADAR_RADIUS_PX - 4.0),
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    ..default()
                },
            ));

            // Pre-allocated zero-alloc blip pool (32 reusable UI ImageNodes)
            for idx in 0..MAX_RADAR_BLIPS {
                dial.spawn((
                    HudRadarBlip(idx),
                    ImageNode::new(atlas.dot.clone()),
                    Visibility::Hidden,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(RADAR_RADIUS_PX - 4.0),
                        top: Val::Px(RADAR_RADIUS_PX - 4.0),
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        ..default()
                    },
                ));
            }
        });
}

/// Translates CS 1.6 world entities (Hostages, T C4 carried/dropped/planted, Explosives) into `RadarMarkerIr`.
fn cs_gather_radar_markers_system(
    time: Res<Time>,
    mut buffer: ResMut<RadarDialBuffer>,
    player_query: Query<&PlayerController>,
    hostage_query: Query<(&GlobalTransform, &HostageEntity)>,
    grenade_query: Query<(&GlobalTransform, &GrenadeProjectile)>,
    planted_c4_query: Query<&GlobalTransform, With<PlantedC4>>,
    dropped_c4_query: Query<(&GlobalTransform, &DroppedWeapon)>,
) {
    buffer.markers.clear();
    let flash_on = ((time.elapsed_secs() * 5.0) as u32).is_multiple_of(2);
    let is_terrorist = player_query
        .get_single()
        .is_ok_and(|p| p.team == CsTeam::Terrorist);

    // 1. Hostage markers (CT team radar logic from `radar.cpp:372-392`)
    for (ht, hostage) in &hostage_query {
        let pos = ht.translation();
        // Convert Bevy (X, Y, Z) -> GoldSrc (X=Bevy.x, Y=-Bevy.z, Z=Bevy.y)
        let world_pos = [pos.x, -pos.z, pos.y];
        let (color, visible) = if hostage.is_dead {
            (DOT_COLOR_HOSTAGE_DEAD, flash_on)
        } else {
            (DOT_COLOR_HOSTAGE, true)
        };
        buffer.markers.push(RadarMarkerIr {
            world_pos,
            color,
            force_glyph: None,
            visible,
        });
    }

    // 2. Terrorist Radar: dropped or planted C4 objective tracking (`radar.cpp`)
    if is_terrorist {
        for c4_t in &planted_c4_query {
            let pos = c4_t.translation();
            buffer.markers.push(RadarMarkerIr {
                world_pos: [pos.x, -pos.z, pos.y],
                color: DOT_COLOR_BOMB,
                force_glyph: Some(RadarGlyphKind::Cross),
                visible: flash_on,
            });
        }
        for (dw_t, dw) in &dropped_c4_query {
            if dw.data.id == Some(WeaponId::C4) {
                let pos = dw_t.translation();
                buffer.markers.push(RadarMarkerIr {
                    world_pos: [pos.x, -pos.z, pos.y],
                    color: DOT_COLOR_BOMB,
                    force_glyph: None,
                    visible: flash_on,
                });
            }
        }
    }

    // 3. Thrown HE / active explosives tracking (bright flashing warning pip)
    for (gt, _) in &grenade_query {
        let pos = gt.translation();
        buffer.markers.push(RadarMarkerIr {
            world_pos: [pos.x, -pos.z, pos.y],
            color: DOT_COLOR_BOMB,
            force_glyph: Some(RadarGlyphKind::Cross),
            visible: flash_on,
        });
    }
}

/// Projects `RadarMarkerIr` entries onto the pre-allocated HUD blip pool in-place.
fn update_radar_blips_system(
    mode: Res<CameraControlMode>,
    atlas_opt: Option<Res<RadarAtlas>>,
    buffer: Res<RadarDialBuffer>,
    player_query: Query<&PlayerController>,
    mut root_query: Query<&mut Visibility, (With<HudRadarRoot>, Without<HudRadarBlip>)>,
    mut blip_query: Query<
        (&HudRadarBlip, &mut ImageNode, &mut Node, &mut Visibility),
        Without<HudRadarRoot>,
    >,
) {
    let show_radar = *mode == CameraControlMode::Player;
    for mut root_vis in &mut root_query {
        let target = if show_radar {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *root_vis != target {
            *root_vis = target;
        }
    }

    if !show_radar {
        return;
    }

    let Some(atlas) = atlas_opt else {
        return;
    };
    let Ok(player) = player_query.get_single() else {
        return;
    };

    // Convert Bevy player yaw (`0 = -Z`) to GoldSrc yaw (`0 = +X, 90 = +Y`):
    // Wait: In GoldSrc angles, `90` = North (+Y, Bevy -Z), `0` = East (+X).
    // In `PlayerController`, `player.yaw = 0` looks toward Bevy `-Z` (GoldSrc `+Y`, i.e. 90 deg),
    // and turning left (`+yaw`) increases GoldSrc yaw toward 180 (-X).
    let gs_yaw_deg = player.yaw.to_degrees() + 90.0;
    let player_gs = player.sim.current.origin;

    for (slot, mut img_node, mut ui_node, mut vis) in &mut blip_query {
        let Some(marker) = buffer.markers.get(slot.0) else {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        };

        if !marker.visible {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }

        let (rx, ry, delta_z) = world_to_radar(
            player_gs,
            marker.world_pos,
            gs_yaw_deg,
            RADAR_SCALE_UNITS_PER_PX,
        );
        let glyph = resolve_radar_glyph(delta_z, marker.force_glyph);
        img_node.image = match glyph {
            RadarGlyphKind::Dot => atlas.dot.clone(),
            RadarGlyphKind::AboveT => atlas.above_t.clone(),
            RadarGlyphKind::BelowFlippedT => atlas.below_t.clone(),
            RadarGlyphKind::Cross => atlas.cross.clone(),
        };
        img_node.color = Color::srgba_u8(
            marker.color.r,
            marker.color.g,
            marker.color.b,
            marker.color.a,
        );
        ui_node.left = Val::Px(RADAR_RADIUS_PX + rx - 4.0);
        ui_node.top = Val::Px(RADAR_RADIUS_PX + ry - 4.0);

        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_radar_cardinal_bearings_and_clamp() {
        let player = [0.0, 0.0, 0.0];
        let yaw_north = 90.0; // Looking North (+Y)

        // Target directly ahead (+Y North, 320 units away -> 10 px up, y = -10)
        let (rx, ry, dz) = world_to_radar(player, [0.0, 320.0, 0.0], yaw_north, 32.0);
        assert!(rx.abs() < 1e-3, "Ahead target must center X, got {rx}");
        assert!(
            (ry - (-10.0)).abs() < 1e-3,
            "Ahead target must be top (-Y), got {ry}"
        );
        assert_eq!(dz, 0.0);

        // Target directly to the right (+X East, 640 units -> 20 px right, x = +20)
        let (rx, ry, _) = world_to_radar(player, [640.0, 0.0, 0.0], yaw_north, 32.0);
        assert!(
            (rx - 20.0).abs() < 1e-3,
            "Right target must be +X, got {rx}"
        );
        assert!(ry.abs() < 1e-3, "Right target Y must be 0, got {ry}");

        // Far target clamped to radar outer disc perimeter (radius 60 px)
        let (rx, ry, _) = world_to_radar(player, [10_000.0, 0.0, 0.0], yaw_north, 32.0);
        assert!(
            (rx - 60.0).abs() < 1e-3,
            "Far right target must clamp to perimeter, got {rx}"
        );
        assert!(ry.abs() < 1e-3);
    }

    #[test]
    fn test_resolve_radar_glyph_elevation_thresholds() {
        assert_eq!(resolve_radar_glyph(0.0, None), RadarGlyphKind::Dot);
        assert_eq!(resolve_radar_glyph(127.0, None), RadarGlyphKind::Dot);
        assert_eq!(resolve_radar_glyph(-127.0, None), RadarGlyphKind::Dot);
        // Target higher than player: Z_player - Z_target <= -128
        assert_eq!(resolve_radar_glyph(-128.0, None), RadarGlyphKind::AboveT);
        // Target lower than player: Z_player - Z_target >= 128
        assert_eq!(
            resolve_radar_glyph(128.0, None),
            RadarGlyphKind::BelowFlippedT
        );
        // Forced cross (planted bomb)
        assert_eq!(
            resolve_radar_glyph(-300.0, Some(RadarGlyphKind::Cross)),
            RadarGlyphKind::Cross
        );
    }
}
