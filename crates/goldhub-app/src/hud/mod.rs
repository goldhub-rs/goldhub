//! Classic Counter-Strike 1.6 HUD (Health, Armor, Ammo, Timer, DrawWList Weapon Menu, and Dynamic Crosshair).

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{SpriteData, SpriteDecoder};
use goldhub_game_cstrike::AmmoCaliber;
use std::collections::HashMap;
use std::fs;

use crate::audio::GameSearchPaths;
use crate::combat::recoil::PlayerRecoil;
use crate::combat::weapon::{
    ActiveWeaponState, PendingWeaponSelection, PlantedC4, PlayerWeaponInventory, WeaponId,
    get_weapon_def,
};
use crate::player::{CameraControlMode, CameraPerspective, PlayerController};
use crate::sprite::resolve_file_path;

pub mod radar;
pub mod text_channels;

/// Classic CS 1.6 semi-transparent additive-feel glowing amber HUD tint.
const CS_HUD_AMBER: Color = Color::srgba(1.0, 0.72, 0.12, 0.48);
const CS_HUD_RED: Color = Color::srgba(1.0, 0.16, 0.12, 0.80);
const CS_HUD_C4_GREEN: Color = Color::srgba(0.0, 165.0 / 255.0, 0.0, 0.82);
const CS_HUD_BARTIME_ORANGE: Color = Color::srgb(1.0, 140.0 / 255.0, 0.0);
const MAX_CARDS_PER_COL: usize = 6;

/// Event fired when player acquires weapons, ammo, or defusal equipment (CHudHistoryResource parity).
#[derive(Event, Debug, Clone)]
pub struct ItemPickupEvent {
    pub weapon_id: Option<WeaponId>,
    pub ammo_caliber: Option<AmmoCaliber>,
    pub is_defuser: bool,
}

#[derive(Debug, Clone)]
pub struct PickupHistoryEntry {
    pub image: Handle<Image>,
    pub lifetime: f32,
}

#[derive(Resource, Default)]
pub struct HudHistoryResource {
    pub entries: Vec<PickupHistoryEntry>,
}

#[derive(Component)]
pub struct HudPickupHistoryRoot;

#[derive(Component)]
pub struct HudPickupSlot(pub usize);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<text_channels::DeveloperMode>();
        app.init_resource::<text_channels::ConsoleOverlayState>();
        app.init_resource::<text_channels::HudFeedState>();
        app.init_resource::<text_channels::CsGameRulesState>();
        app.init_resource::<crate::combat::BuyMenuState>();
        app.init_resource::<HudHistoryResource>();
        app.add_event::<text_channels::HudPrintEvent>();
        app.add_event::<ItemPickupEvent>();
        app.add_plugins(radar::CsRadarPlugin);
        app.add_systems(Startup, setup_cs_hud);
        app.add_systems(
            Update,
            (
                text_channels::update_cs_game_rules_system
                    .after(crate::combat::weapon::update_planted_c4_system),
                text_channels::process_hud_print_events_system
                    .after(text_channels::update_cs_game_rules_system),
                text_channels::update_console_and_dev_hud_system,
                text_channels::update_buy_menu_system,
                update_hud_health_armor_system,
                update_hud_ammo_system,
                update_hud_flashlight_system,
                update_hud_money_system,
                update_hud_scenario_system,
                update_hud_timer_system.after(text_channels::update_cs_game_rules_system),
                update_hud_weapon_menu_system,
                update_hud_bartime_and_flash_system,
                update_hud_pickup_history_system,
                update_dynamic_crosshair_system,
            ),
        );
    }
}

/// Preloaded weapon selection sprite handles `(unselected, selected, width, height)`.
#[derive(Clone)]
pub struct HudWeaponCardImages {
    pub normal: Handle<Image>,
    pub selected: Handle<Image>,
    pub width: u32,
    pub height: u32,
}

/// Canonical CS 1.6 left mid-screen `CHudStatusIcons` slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusIconKind {
    BuyZone,
    C4,
    Defuser,
    Rescue,
    Escape,
    VipSafety,
}

/// Authentic CS 1.6 sliced sprite glyph mask atlas from `sprites/640hud*.spr` & `sniper_scope.spr`.
#[derive(Resource, Clone)]
pub struct HudAtlas {
    pub digits: [Handle<Image>; 10],
    pub divider: Handle<Image>,
    pub cross: Handle<Image>,
    pub suit_full: Handle<Image>,
    pub flash_empty: Handle<Image>,
    pub flash_full: Handle<Image>,
    pub flash_beam: Handle<Image>,
    pub dollar: Handle<Image>,
    pub c4_icon: Handle<Image>,
    pub defuser_icon: Handle<Image>,
    pub buyzone_icon: Handle<Image>,
    pub rescue_icon: Handle<Image>,
    pub escape_icon: Handle<Image>,
    pub vipsafety_icon: Handle<Image>,
    pub stopwatch: Handle<Image>,
    pub buckets: [Handle<Image>; 5],
    pub selection_frame: Option<Handle<Image>>,
    pub sniper_scope: Option<Handle<Image>>,
    pub scope_bezel: Handle<Image>,
    pub ammo_icons: HashMap<AmmoCaliber, Handle<Image>>,
    pub weapon_cards: HashMap<WeaponId, HudWeaponCardImages>,
}

#[derive(Component)]
pub struct HudFlashlightRoot;

#[derive(Component)]
pub struct HudFlashlightCasing;

#[derive(Component)]
pub struct HudFlashlightFull;

#[derive(Component)]
pub struct HudFlashlightBeam;

#[derive(Component)]
pub struct HudMoneyRoot;

#[derive(Component)]
pub struct HudMoneyDigit(pub usize);

#[derive(Component)]
pub struct HudStatusIcon(pub StatusIconKind);

#[derive(Component)]
pub struct HudSniperScopeRoot;

#[derive(Component)]
pub struct HudTimerDigit(pub usize);

#[derive(Component)]
pub struct HudTimerColonDot;

#[derive(Component)]
pub struct HudTimerStopwatch;

#[derive(Component)]
pub struct HudWeaponMenuRoot;

#[derive(Component)]
pub struct HudWeaponBucketIcon(pub u8);

#[derive(Component)]
pub struct HudSlotInactiveContainer(pub u8);

#[derive(Component)]
pub struct HudSlotActiveContainer(pub u8);

#[derive(Component)]
pub struct HudSlotInactiveBox(pub u8, pub usize);

#[derive(Component)]
pub struct HudSlotCard(pub u8, pub usize);

#[derive(Component)]
pub struct HudSlotCardSprite(pub u8, pub usize);

#[derive(Component)]
pub struct HudSlotCardSelection(pub u8, pub usize);

#[derive(Component)]
pub struct HudSlotCardAmmoTrack(pub u8, pub usize);

#[derive(Component)]
pub struct HudSlotCardAmmoFill(pub u8, pub usize);

#[derive(Component)]
pub struct HudHealthCrossIcon;

#[derive(Component)]
pub struct HudHealthDigit(pub usize);

#[derive(Component)]
pub struct HudArmorDigit(pub usize);

#[derive(Component)]
pub struct HudClipDigit(pub usize);

#[derive(Component)]
pub struct HudAmmoDivider;

#[derive(Component)]
pub struct HudReserveDigit(pub usize);

#[derive(Component)]
pub struct HudAmmoCaliberIcon;

#[derive(Component)]
pub struct HudAmmoRoot;

#[derive(Component)]
pub struct HudBarTimeRoot;

#[derive(Component)]
pub struct HudBarTimeFill;

#[derive(Component)]
pub struct HudCenterMessage;

#[derive(Component)]
pub struct HudFlashOverlay;

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

#[inline]
fn digit_at_and_visible(
    value: usize,
    idx: usize,
    total_digits: usize,
    draw_zero: bool,
) -> (usize, bool) {
    let divisor = 10usize.pow((total_digits - 1 - idx) as u32);
    let digit = (value / divisor) % 10;
    let is_last = idx + 1 == total_digits;
    let visible = value >= divisor || (is_last && draw_zero);
    (digit, visible)
}

fn extract_sub_image(source: &[u8], src_width: u32, x: u32, y: u32, w: u32, h: u32) -> Image {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let src_row = y + row;
        let start = ((src_row * src_width + x) * 4) as usize;
        let end = start + (w * 4) as usize;
        if end <= source.len() {
            data.extend_from_slice(&source[start..end]);
        } else {
            data.extend(std::iter::repeat_n(0, (w * 4) as usize));
        }
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

/// Converts a GoldSrc additive HUD sprite frame 0 into a white-RGB alpha mask so UI color tints
/// cleanly without black rectangular backgrounds or dark double attenuation.
fn load_hud_mask_rgba(
    sprite_path: &str,
    search_paths: &GameSearchPaths,
    cache: &mut HashMap<&'static str, (Vec<u8>, u32, u32)>,
    key: &'static str,
) -> Option<(Vec<u8>, u32, u32)> {
    if let Some(cached) = cache.get(key) {
        return Some(cached.clone());
    }
    let path = resolve_file_path(sprite_path, search_paths)?;
    let raw = fs::read(path).ok()?;
    let sprite: SpriteData = SpriteDecoder.decode(&raw).ok()?;
    let f0 = sprite.frames.first()?;
    let mut mask_rgba = Vec::with_capacity(f0.rgba_pixels.len());
    for chunk in f0.rgba_pixels.as_chunks::<4>().0 {
        let lum = chunk[0].max(chunk[1]).max(chunk[2]);
        let a = if lum > 8 {
            ((f32::from(lum) / 255.0).powf(0.8) * 255.0).round() as u8
        } else {
            0
        };
        mask_rgba.extend_from_slice(&[255, 255, 255, a]);
    }
    let entry = (mask_rgba, f0.width, f0.height);
    cache.insert(key, entry.clone());
    Some(entry)
}

const WEAPON_CARD_SPECS: &[(WeaponId, &str, u32, u32, &str, u32, u32, u32, u32)] = &[
    (
        WeaponId::AK47,
        "sprites/640hud10.spr",
        0,
        0,
        "sprites/640hud11.spr",
        0,
        0,
        170,
        45,
    ),
    (
        WeaponId::AWP,
        "sprites/640hud2.spr",
        0,
        135,
        "sprites/640hud5.spr",
        0,
        135,
        170,
        45,
    ),
    (
        WeaponId::G3SG1,
        "sprites/640hud2.spr",
        0,
        180,
        "sprites/640hud5.spr",
        0,
        180,
        170,
        45,
    ),
    (
        WeaponId::M249,
        "sprites/640hud3.spr",
        0,
        0,
        "sprites/640hud6.spr",
        0,
        0,
        170,
        45,
    ),
    (
        WeaponId::M3,
        "sprites/640hud1.spr",
        0,
        135,
        "sprites/640hud4.spr",
        0,
        135,
        170,
        45,
    ),
    (
        WeaponId::M4A1,
        "sprites/640hud2.spr",
        0,
        45,
        "sprites/640hud5.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Mp5,
        "sprites/640hud1.spr",
        0,
        180,
        "sprites/640hud4.spr",
        0,
        180,
        170,
        45,
    ),
    (
        WeaponId::P90,
        "sprites/640hud12.spr",
        0,
        0,
        "sprites/640hud13.spr",
        0,
        0,
        170,
        45,
    ),
    (
        WeaponId::Scout,
        "sprites/640hud12.spr",
        0,
        45,
        "sprites/640hud13.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Sg552,
        "sprites/640hud10.spr",
        0,
        45,
        "sprites/640hud11.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Tmp,
        "sprites/640hud2.spr",
        0,
        0,
        "sprites/640hud5.spr",
        0,
        0,
        170,
        45,
    ),
    (
        WeaponId::XM1014,
        "sprites/640hud12.spr",
        0,
        135,
        "sprites/640hud13.spr",
        0,
        135,
        170,
        45,
    ),
    (
        WeaponId::Mac10,
        "sprites/640hud14.spr",
        0,
        0,
        "sprites/640hud15.spr",
        0,
        0,
        170,
        45,
    ),
    (
        WeaponId::Aug,
        "sprites/640hud14.spr",
        0,
        45,
        "sprites/640hud15.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Ump45,
        "sprites/640hud16.spr",
        0,
        0,
        "sprites/640hud16.spr",
        0,
        0,
        170,
        45,
    ),
    (
        WeaponId::Sg550,
        "sprites/640hud14.spr",
        0,
        180,
        "sprites/640hud15.spr",
        0,
        180,
        170,
        45,
    ),
    (
        WeaponId::Galil,
        "sprites/640hud17.spr",
        0,
        45,
        "sprites/640hud18.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Famas,
        "sprites/640hud17.spr",
        0,
        90,
        "sprites/640hud18.spr",
        0,
        90,
        170,
        45,
    ),
    (
        WeaponId::Deagle,
        "sprites/640hud10.spr",
        0,
        90,
        "sprites/640hud11.spr",
        0,
        90,
        170,
        45,
    ),
    (
        WeaponId::Glock18,
        "sprites/640hud1.spr",
        0,
        45,
        "sprites/640hud4.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Glock,
        "sprites/640hud1.spr",
        0,
        45,
        "sprites/640hud4.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::P228,
        "sprites/640hud12.spr",
        0,
        90,
        "sprites/640hud13.spr",
        0,
        90,
        170,
        45,
    ),
    (
        WeaponId::USP45,
        "sprites/640hud1.spr",
        0,
        90,
        "sprites/640hud4.spr",
        0,
        90,
        170,
        45,
    ),
    (
        WeaponId::Elite,
        "sprites/640hud14.spr",
        0,
        90,
        "sprites/640hud15.spr",
        0,
        90,
        170,
        45,
    ),
    (
        WeaponId::FiveSeven,
        "sprites/640hud14.spr",
        0,
        135,
        "sprites/640hud15.spr",
        0,
        135,
        170,
        45,
    ),
    (
        WeaponId::Knife,
        "sprites/640hud10.spr",
        0,
        135,
        "sprites/640hud11.spr",
        0,
        135,
        170,
        45,
    ),
    (
        WeaponId::HeGrenade,
        "sprites/640hud3.spr",
        0,
        45,
        "sprites/640hud6.spr",
        0,
        45,
        170,
        45,
    ),
    (
        WeaponId::Flashbang,
        "sprites/640hud3.spr",
        0,
        90,
        "sprites/640hud6.spr",
        0,
        90,
        170,
        45,
    ),
    (
        WeaponId::SmokeGrenade,
        "sprites/640hud3.spr",
        0,
        135,
        "sprites/640hud6.spr",
        0,
        135,
        170,
        45,
    ),
    (
        WeaponId::C4,
        "sprites/640hud1.spr",
        0,
        0,
        "sprites/640hud4.spr",
        0,
        0,
        170,
        45,
    ),
];

/// Builds the classic CS 1.6 HUD layout with authentic translucent sprites from `640hud*.spr`.
#[allow(clippy::too_many_lines)]
fn build_sniper_scope_bezel_image() -> Image {
    const SIZE: u32 = 512;
    let center = (SIZE as f32) * 0.5 - 0.5;
    let aperture_r = 232.0f32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dist = (x as f32 - center).hypot(y as f32 - center);
            let alpha = ((dist - aperture_r + 1.5) / 2.5).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[0, 0, 0, (alpha * 255.0).round() as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn load_sniper_scope_sprite(
    search_paths: &GameSearchPaths,
    images: &mut Assets<Image>,
) -> Option<Handle<Image>> {
    let path = resolve_file_path("sprites/sniper_scope.spr", search_paths)?;
    let raw = fs::read(path).ok()?;
    let sprite: SpriteData = SpriteDecoder.decode(&raw).ok()?;
    let f0 = sprite.frames.first()?;
    Some(images.add(Image::new(
        Extent3d {
            width: f0.width,
            height: f0.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        f0.rgba_pixels.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )))
}

#[allow(clippy::too_many_lines)]
fn setup_cs_hud(
    mut commands: Commands,
    search_paths: Res<GameSearchPaths>,
    mut images: ResMut<Assets<Image>>,
    mut fonts: ResMut<Assets<Font>>,
) {
    let loc_res = text_channels::CsLocalizationResource::load_from_search_paths(&search_paths);
    let font_res = text_channels::load_hud_fonts(&search_paths, &mut fonts);
    let ui_font = font_res.ui_font.clone();
    commands.insert_resource(loc_res);
    commands.insert_resource(font_res);
    text_channels::spawn_text_channel_nodes(&mut commands, ui_font.clone());
    let atlas_opt = (|| {
        let mut sheet_cache: HashMap<&'static str, (Vec<u8>, u32, u32)> = HashMap::new();
        let (mask_rgba, hud7_w, _) = load_hud_mask_rgba(
            "sprites/640hud7.spr",
            &search_paths,
            &mut sheet_cache,
            "sprites/640hud7.spr",
        )?;

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
            let img = extract_sub_image(&mask_rgba, hud7_w, cx, cy, 20, 25);
            digits.push(images.add(img));
        }
        let digits: [Handle<Image>; 10] = digits.try_into().ok()?;
        let divider = images.add(extract_sub_image(&mask_rgba, hud7_w, 240, 0, 2, 25));
        let cross = images.add(extract_sub_image(&mask_rgba, hud7_w, 48, 25, 24, 24));
        let suit_full = images.add(extract_sub_image(&mask_rgba, hud7_w, 0, 25, 24, 24));
        let flash_empty = images.add(extract_sub_image(&mask_rgba, hud7_w, 112, 25, 32, 32));
        let flash_full = images.add(extract_sub_image(&mask_rgba, hud7_w, 160, 25, 32, 32));
        let flash_beam = images.add(extract_sub_image(&mask_rgba, hud7_w, 144, 25, 16, 32));
        let dollar = images.add(extract_sub_image(&mask_rgba, hud7_w, 192, 25, 18, 25));
        let c4_icon = images.add(extract_sub_image(&mask_rgba, hud7_w, 0, 148, 32, 32));
        let defuser_icon = images.add(extract_sub_image(&mask_rgba, hud7_w, 32, 148, 32, 32));
        let rescue_icon = images.add(extract_sub_image(&mask_rgba, hud7_w, 64, 148, 32, 32));
        let buyzone_icon = images.add(extract_sub_image(&mask_rgba, hud7_w, 96, 148, 32, 32));
        let escape_icon = images.add(extract_sub_image(&mask_rgba, hud7_w, 128, 148, 32, 32));
        let vipsafety_icon = images.add(extract_sub_image(&mask_rgba, hud7_w, 160, 148, 32, 32));
        let stopwatch = images.add(extract_sub_image(&mask_rgba, hud7_w, 144, 72, 24, 24));
        let sniper_scope = load_sniper_scope_sprite(&search_paths, &mut images);
        let scope_bezel = images.add(build_sniper_scope_bezel_image());
        let bucket_coords: [(u32, u32); 5] =
            [(168, 72), (188, 72), (208, 72), (168, 92), (188, 92)];
        let mut bucket_handles = Vec::with_capacity(5);
        for &(bx, by) in &bucket_coords {
            bucket_handles.push(images.add(extract_sub_image(&mask_rgba, hud7_w, bx, by, 20, 20)));
        }
        let buckets: [Handle<Image>; 5] = bucket_handles.try_into().ok()?;

        // Slice all 13 caliber / grenade / C4 icons from `640hud7.spr`
        let all_calibers = [
            AmmoCaliber::Buckshot12g,
            AmmoCaliber::Ae50,
            AmmoCaliber::Mm9,
            AmmoCaliber::Nato762,
            AmmoCaliber::Acp45,
            AmmoCaliber::Sig357,
            AmmoCaliber::Nato556,
            AmmoCaliber::Magnum338,
            AmmoCaliber::Flashbang,
            AmmoCaliber::HeGrenade,
            AmmoCaliber::C4,
            AmmoCaliber::Mm57,
            AmmoCaliber::SmokeGrenade,
        ];
        let mut ammo_icons = HashMap::new();
        for cal in all_calibers {
            if let Some((ax, ay, aw, ah)) = cal.hud7_rect() {
                ammo_icons.insert(
                    cal,
                    images.add(extract_sub_image(&mask_rgba, hud7_w, ax, ay, aw, ah)),
                );
            }
        }

        // Slice active selection frame (`640hud3.spr 0,180,170,45`)
        let selection_frame = load_hud_mask_rgba(
            "sprites/640hud3.spr",
            &search_paths,
            &mut sheet_cache,
            "sprites/640hud3.spr",
        )
        .map(|(buf, w, _)| images.add(extract_sub_image(&buf, w, 0, 180, 170, 45)));

        // Slice weapon cards from `640hud1..6, 10..18`
        let mut weapon_cards = HashMap::new();
        for &(wid, spr, x, y, spr_s, sx, sy, cw, ch) in WEAPON_CARD_SPECS {
            if let (Some((n_buf, n_w, _)), Some((s_buf, s_w, _))) = (
                load_hud_mask_rgba(spr, &search_paths, &mut sheet_cache, spr),
                load_hud_mask_rgba(spr_s, &search_paths, &mut sheet_cache, spr_s),
            ) {
                let normal = images.add(extract_sub_image(&n_buf, n_w, x, y, cw, ch));
                let selected = images.add(extract_sub_image(&s_buf, s_w, sx, sy, cw, ch));
                weapon_cards.insert(
                    wid,
                    HudWeaponCardImages {
                        normal,
                        selected,
                        width: cw,
                        height: ch,
                    },
                );
            }
        }

        Some(HudAtlas {
            digits,
            divider,
            cross,
            suit_full,
            flash_empty,
            flash_full,
            flash_beam,
            dollar,
            c4_icon,
            defuser_icon,
            buyzone_icon,
            rescue_icon,
            escape_icon,
            vipsafety_icon,
            stopwatch,
            buckets,
            selection_frame,
            sniper_scope,
            scope_bezel,
            ammo_icons,
            weapon_cards,
        })
    })();

    if let Some(atlas) = atlas_opt {
        commands.insert_resource(atlas.clone());

        // 1. Bottom-Left: Health Row (Cross + 3 digits, no leading zeros)
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(12.0),
                    left: Val::Px(32.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(2.0),
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|parent| {
                parent.spawn((
                    HudHealthCrossIcon,
                    ImageNode::new(atlas.cross.clone()).with_color(CS_HUD_AMBER),
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
                        ImageNode::new(atlas.digits[d].clone()).with_color(CS_HUD_AMBER),
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(25.0),
                            ..default()
                        },
                    ));
                }
            });

        // 2. Bottom-Left: Armor Row (Suit icon + 3 digits, driven by player.armor)
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(12.0),
                    left: Val::Px(150.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(2.0),
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(atlas.suit_full.clone()).with_color(CS_HUD_AMBER),
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
                        ImageNode::new(atlas.digits[d].clone()).with_color(CS_HUD_AMBER),
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(25.0),
                            ..default()
                        },
                    ));
                }
            });

        // 3. Bottom-Center: Round / C4 Countdown Timer (stopwatch + M:SS)
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(12.0),
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|parent| {
                parent.spawn((
                    HudTimerStopwatch,
                    ImageNode::new(atlas.stopwatch.clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        margin: UiRect::right(Val::Px(4.0)),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudTimerDigit(0),
                    ImageNode::new(atlas.digits[1].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::SpaceEvenly,
                        height: Val::Px(18.0),
                        padding: UiRect::horizontal(Val::Px(2.0)),
                        ..default()
                    })
                    .with_children(|colon| {
                        colon.spawn((
                            HudTimerColonDot,
                            BackgroundColor(CS_HUD_AMBER),
                            Node {
                                width: Val::Px(3.0),
                                height: Val::Px(3.0),
                                ..default()
                            },
                        ));
                        colon.spawn((
                            HudTimerColonDot,
                            BackgroundColor(CS_HUD_AMBER),
                            Node {
                                width: Val::Px(3.0),
                                height: Val::Px(3.0),
                                ..default()
                            },
                        ));
                    });
                parent.spawn((
                    HudTimerDigit(1),
                    ImageNode::new(atlas.digits[4].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudTimerDigit(2),
                    ImageNode::new(atlas.digits[5].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
            });

        // 4. Bottom-Right: Ammo Row (`[Clip | Reserve] [24x24 Caliber]` or `[Count] [24x24 Grenade/C4]`)
        commands
            .spawn((
                HudAmmoRoot,
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(12.0),
                    right: Val::Px(32.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(3.0),
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|parent| {
                parent.spawn((
                    HudClipDigit(0),
                    ImageNode::new(atlas.digits[3].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudClipDigit(1),
                    ImageNode::new(atlas.digits[0].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudAmmoDivider,
                    ImageNode::new(atlas.divider.clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(2.0),
                        height: Val::Px(25.0),
                        margin: UiRect::horizontal(Val::Px(4.0)),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudReserveDigit(0),
                    ImageNode::new(atlas.digits[0].clone()).with_color(CS_HUD_AMBER),
                    Visibility::Hidden,
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudReserveDigit(1),
                    ImageNode::new(atlas.digits[9].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                parent.spawn((
                    HudReserveDigit(2),
                    ImageNode::new(atlas.digits[0].clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(25.0),
                        ..default()
                    },
                ));
                let default_cal = atlas
                    .ammo_icons
                    .get(&AmmoCaliber::Nato556)
                    .cloned()
                    .unwrap_or_else(|| atlas.cross.clone());
                parent.spawn((
                    HudAmmoCaliberIcon,
                    ImageNode::new(default_cal).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(24.0),
                        height: Val::Px(24.0),
                        margin: UiRect::left(Val::Px(6.0)),
                        ..default()
                    },
                ));
            });

        // 5. Top-Right: Flashlight Indicator
        commands
            .spawn((
                HudFlashlightRoot,
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(24.0),
                    right: Val::Px(24.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|parent| {
                parent
                    .spawn(Node {
                        width: Val::Px(32.0),
                        height: Val::Px(32.0),
                        position_type: PositionType::Relative,
                        ..default()
                    })
                    .with_children(|casing_box| {
                        casing_box.spawn((
                            HudFlashlightCasing,
                            ImageNode::new(atlas.flash_empty.clone())
                                .with_color(Color::srgba(1.0, 0.65, 0.0, 0.25)),
                            Node {
                                position_type: PositionType::Absolute,
                                width: Val::Px(32.0),
                                height: Val::Px(32.0),
                                ..default()
                            },
                        ));
                        casing_box.spawn((
                            HudFlashlightFull,
                            ImageNode::new(atlas.flash_full.clone())
                                .with_color(Color::srgba(1.0, 0.65, 0.0, 0.90)),
                            Node {
                                position_type: PositionType::Absolute,
                                width: Val::Px(32.0),
                                height: Val::Px(32.0),
                                ..default()
                            },
                        ));
                    });
                // CS 1.6: Flashlight light beam cone displayed to the right of the casing when active
                parent.spawn((
                    HudFlashlightBeam,
                    ImageNode::new(atlas.flash_beam.clone())
                        .with_color(Color::srgba(1.0, 0.82, 0.05, 0.95)),
                    Node {
                        display: Display::None,
                        width: Val::Px(16.0),
                        height: Val::Px(32.0),
                        ..default()
                    },
                    Visibility::Hidden,
                ));
            });

        // 6. Bottom-Right: Money Counter ($ + digits) cleanly spaced above ammo display
        commands
            .spawn((
                HudMoneyRoot,
                Node {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(50.0),
                    right: Val::Px(32.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|parent| {
                parent.spawn((
                    ImageNode::new(atlas.dollar.clone()).with_color(CS_HUD_AMBER),
                    Node {
                        width: Val::Px(18.0),
                        height: Val::Px(25.0),
                        margin: UiRect::right(Val::Px(4.0)),
                        ..default()
                    },
                ));
                for i in 0..5 {
                    parent.spawn((
                        HudMoneyDigit(i),
                        ImageNode::new(atlas.digits[0].clone()).with_color(CS_HUD_AMBER),
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(25.0),
                            ..default()
                        },
                    ));
                }
            });

        // 7. Left Mid-Screen (`left: 5px, bottom: 50%`, 37px upward step): CS 1.6 `CHudStatusIcons` stack
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(5.0),
                    bottom: Val::Percent(50.0),
                    flex_direction: FlexDirection::ColumnReverse,
                    row_gap: Val::Px(5.0),
                    ..default()
                },
                GlobalZIndex(25),
            ))
            .with_children(|stack| {
                for (kind, handle) in [
                    (StatusIconKind::BuyZone, atlas.buyzone_icon.clone()),
                    (StatusIconKind::C4, atlas.c4_icon.clone()),
                    (StatusIconKind::Defuser, atlas.defuser_icon.clone()),
                    (StatusIconKind::Rescue, atlas.rescue_icon.clone()),
                    (StatusIconKind::Escape, atlas.escape_icon.clone()),
                    (StatusIconKind::VipSafety, atlas.vipsafety_icon.clone()),
                ] {
                    stack.spawn((
                        HudStatusIcon(kind),
                        ImageNode::new(handle).with_color(CS_HUD_C4_GREEN),
                        Node {
                            width: Val::Px(32.0),
                            height: Val::Px(32.0),
                            display: Display::None,
                            ..default()
                        },
                        Visibility::Hidden,
                    ));
                }
            });

        // 7b. Fullscreen `CHudSniperScope` Overlay (circular cutout + `sniper_scope.spr` + black side bars)
        commands
            .spawn((
                HudSniperScopeRoot,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Stretch,
                    ..default()
                },
                GlobalZIndex(2),
                Visibility::Hidden,
            ))
            .with_children(|scope| {
                scope.spawn((
                    BackgroundColor(Color::BLACK),
                    Node {
                        flex_grow: 1.0,
                        height: Val::Percent(100.0),
                        ..default()
                    },
                ));
                scope
                    .spawn(Node {
                        width: Val::Vh(100.0),
                        height: Val::Vh(100.0),
                        position_type: PositionType::Relative,
                        ..default()
                    })
                    .with_children(|lens| {
                        lens.spawn((
                            ImageNode::new(atlas.scope_bezel.clone()),
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                top: Val::Px(0.0),
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                        ));
                        if let Some(ref reticle_tex) = atlas.sniper_scope {
                            lens.spawn((
                                ImageNode::new(reticle_tex.clone()),
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: Val::Px(0.0),
                                    top: Val::Px(0.0),
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                            ));
                        }
                    });
                scope.spawn((
                    BackgroundColor(Color::BLACK),
                    Node {
                        flex_grow: 1.0,
                        height: Val::Percent(100.0),
                        ..default()
                    },
                ));
            });

        // 8. Top-Left next to Radar (`left = 154, top = 10`): Classic CS 1.6 `DrawWList` Selection Menu
        commands
            .spawn((
                HudWeaponMenuRoot,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(154.0),
                    top: Val::Px(10.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexStart,
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                GlobalZIndex(25),
                Visibility::Hidden,
            ))
            .with_children(|menu| {
                let fallback_card = atlas
                    .weapon_cards
                    .get(&WeaponId::Knife)
                    .map_or_else(|| atlas.cross.clone(), |c| c.normal.clone());
                let sel_img = atlas
                    .selection_frame
                    .clone()
                    .unwrap_or_else(|| fallback_card.clone());

                for slot_idx in 1..=5u8 {
                    menu.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::FlexStart,
                        row_gap: Val::Px(2.0),
                        ..default()
                    })
                    .with_children(|col| {
                        col.spawn((
                            HudWeaponBucketIcon(slot_idx),
                            ImageNode::new(atlas.buckets[(slot_idx - 1) as usize].clone())
                                .with_color(CS_HUD_AMBER),
                            Node {
                                width: Val::Px(20.0),
                                height: Val::Px(20.0),
                                ..default()
                            },
                        ));

                        col.spawn((
                            HudSlotInactiveContainer(slot_idx),
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(4.0),
                                ..default()
                            },
                        ))
                        .with_children(|inactive_col| {
                            for row in 0..MAX_CARDS_PER_COL {
                                inactive_col.spawn((
                                    HudSlotInactiveBox(slot_idx, row),
                                    BackgroundColor(Color::srgba(1.0, 0.65, 0.0, 0.20)),
                                    Node {
                                        width: Val::Px(20.0),
                                        height: Val::Px(20.0),
                                        ..default()
                                    },
                                ));
                            }
                        });

                        col.spawn((
                            HudSlotActiveContainer(slot_idx),
                            Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(2.0),
                                display: Display::None,
                                ..default()
                            },
                        ))
                        .with_children(|active_col| {
                            for row in 0..MAX_CARDS_PER_COL {
                                active_col
                                    .spawn((
                                        HudSlotCard(slot_idx, row),
                                        BackgroundColor(Color::srgba(0.85, 0.55, 0.0, 0.28)),
                                        Node {
                                            width: Val::Px(170.0),
                                            height: Val::Px(45.0),
                                            position_type: PositionType::Relative,
                                            ..default()
                                        },
                                    ))
                                    .with_children(|card| {
                                        card.spawn((
                                            HudSlotCardSprite(slot_idx, row),
                                            ImageNode::new(fallback_card.clone())
                                                .with_color(CS_HUD_AMBER),
                                            Node {
                                                position_type: PositionType::Absolute,
                                                left: Val::Px(0.0),
                                                top: Val::Px(0.0),
                                                width: Val::Px(170.0),
                                                height: Val::Px(45.0),
                                                ..default()
                                            },
                                        ));
                                        card.spawn((
                                            HudSlotCardSelection(slot_idx, row),
                                            ImageNode::new(sel_img.clone())
                                                .with_color(Color::srgba(1.0, 0.78, 0.1, 0.95)),
                                            Node {
                                                position_type: PositionType::Absolute,
                                                left: Val::Px(0.0),
                                                top: Val::Px(0.0),
                                                width: Val::Px(170.0),
                                                height: Val::Px(45.0),
                                                ..default()
                                            },
                                            Visibility::Hidden,
                                        ));
                                        card.spawn((
                                            HudSlotCardAmmoTrack(slot_idx, row),
                                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
                                            Node {
                                                position_type: PositionType::Absolute,
                                                left: Val::Px(4.0),
                                                top: Val::Px(4.0),
                                                width: Val::Px(20.0),
                                                height: Val::Px(4.0),
                                                ..default()
                                            },
                                        ))
                                        .with_children(
                                            |track| {
                                                track.spawn((
                                                    HudSlotCardAmmoFill(slot_idx, row),
                                                    BackgroundColor(Color::srgba(
                                                        0.0, 0.88, 0.0, 0.95,
                                                    )),
                                                    Node {
                                                        width: Val::Px(20.0),
                                                        height: Val::Px(4.0),
                                                        ..default()
                                                    },
                                                ));
                                            },
                                        );
                                    });
                            }
                        });
                    });
                }
            });
    }

    // 9. Centered `BarTime` Progress Bar (C4 arm/defuse indicator at 2/3 height, `#FF8C00` 1px frame + 2px inset)
    commands
        .spawn((
            HudBarTimeRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(66.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|wrapper| {
            wrapper
                .spawn((
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                    BorderColor(CS_HUD_BARTIME_ORANGE),
                    Node {
                        width: Val::Percent(50.0),
                        height: Val::Px(12.0),
                        border: UiRect::all(Val::Px(1.0)),
                        padding: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                ))
                .with_children(|track| {
                    track.spawn((
                        HudBarTimeFill,
                        BackgroundColor(CS_HUD_BARTIME_ORANGE),
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                    ));
                });
        });

    // 10. Centered transient `HUD_PRINTCENTER` status message (top 33%, crisp localized `Verdana`)
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(33.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|wrapper| {
            wrapper.spawn((
                HudCenterMessage,
                Text::new(""),
                TextFont {
                    font: ui_font,
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.24, 0.91, 0.24)),
                Visibility::Hidden,
            ));
        });

    // 11. Fullscreen white flashbang ScreenFade overlay (`V_FadeAlpha` parity)
    commands.spawn((
        HudFlashOverlay,
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        GlobalZIndex(100),
        Visibility::Hidden,
    ));

    // 12. Pickup history stack (CHudHistoryResource parity: right edge above ammo count)
    commands
        .spawn((
            HudPickupHistoryRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(24.0),
                bottom: Val::Px(75.0),
                flex_direction: FlexDirection::ColumnReverse,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(4.0),
                ..default()
            },
            GlobalZIndex(40),
        ))
        .with_children(|history_col| {
            for slot_idx in 0..4 {
                history_col.spawn((
                    HudPickupSlot(slot_idx),
                    ImageNode::default(),
                    Node {
                        width: Val::Auto,
                        height: Val::Px(24.0),
                        ..default()
                    },
                    Visibility::Hidden,
                ));
            }
        });

    // 6. Dynamic Crosshair Root & 4 Lines
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
    mut hp_cross_query: Query<
        &mut ImageNode,
        (
            With<HudHealthCrossIcon>,
            Without<HudHealthDigit>,
            Without<HudArmorDigit>,
        ),
    >,
    mut hp_digits: Query<
        (&HudHealthDigit, &mut ImageNode, &mut Visibility),
        Without<HudArmorDigit>,
    >,
    mut armor_digits: Query<
        (&HudArmorDigit, &mut ImageNode, &mut Visibility),
        Without<HudHealthDigit>,
    >,
) {
    let Some(atlas) = atlas_opt else {
        return;
    };
    if let Ok(player) = player_query.get_single() {
        let hp = (player.health.round() as usize).min(999);
        let hp_color = if player.health <= 25.0 {
            CS_HUD_RED
        } else {
            CS_HUD_AMBER
        };
        for mut img in &mut hp_cross_query {
            img.color = hp_color;
        }
        for (digit, mut img, mut vis) in &mut hp_digits {
            let (d, show) = digit_at_and_visible(hp, digit.0, 3, true);
            img.image = atlas.digits[d].clone();
            img.color = hp_color;
            *vis = if show {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }

        let armor = (player.armor.round() as usize).min(999);
        for (digit, mut img, mut vis) in &mut armor_digits {
            let (d, show) = digit_at_and_visible(armor, digit.0, 3, true);
            img.image = atlas.digits[d].clone();
            *vis = if show {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_hud_flashlight_system(
    fl_query: Query<&crate::player::PlayerFlashlight>,
    mut casing_query: Query<
        &mut ImageNode,
        (
            With<HudFlashlightCasing>,
            Without<HudFlashlightFull>,
            Without<HudFlashlightBeam>,
        ),
    >,
    mut full_query: Query<
        (&mut ImageNode, &mut Node, &mut Visibility),
        (
            With<HudFlashlightFull>,
            Without<HudFlashlightCasing>,
            Without<HudFlashlightBeam>,
        ),
    >,
    mut beam_query: Query<
        (&mut ImageNode, &mut Node, &mut Visibility),
        (
            With<HudFlashlightBeam>,
            Without<HudFlashlightFull>,
            Without<HudFlashlightCasing>,
        ),
    >,
) {
    let Ok(fl) = fl_query.get_single() else {
        return;
    };

    let bat = fl.battery.clamp(0.0, 1.0);
    // Authentic CS 1.6 radiant gold-yellow HUD color, shifts to red below 20%
    let bar_color = if bat < 0.20 {
        Color::srgba(1.0, 0.16, 0.12, 0.95)
    } else {
        Color::srgba(1.0, 0.82, 0.05, 0.95)
    };

    for (mut img, mut ui_node, mut vis) in &mut beam_query {
        if fl.is_on {
            *vis = Visibility::Inherited;
            ui_node.display = Display::Flex;
            img.color = bar_color.with_alpha(0.92);
        } else {
            *vis = Visibility::Hidden;
            ui_node.display = Display::None;
        }
    }

    let casing_alpha = if fl.is_on { 0.92 } else { 0.28 };
    for mut node in &mut casing_query {
        node.color = bar_color.with_alpha(casing_alpha);
    }

    let fill_alpha = if fl.is_on { 0.98 } else { 0.32 };
    let offset = (32.0 * (1.0 - bat)).round();
    for (mut img, mut ui_node, mut vis) in &mut full_query {
        if offset < 32.0 {
            *vis = Visibility::Inherited;
            img.color = bar_color.with_alpha(fill_alpha);
            img.rect = Some(Rect::new(offset, 0.0, 32.0, 32.0));
            ui_node.left = Val::Px(offset);
            ui_node.width = Val::Px((32.0 - offset).max(0.0));
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_hud_timer_system(
    time: Res<Time>,
    atlas_opt: Option<Res<HudAtlas>>,
    rules_state: Res<text_channels::CsGameRulesState>,
    planted_c4: Query<&PlantedC4>,
    mut stopwatch_q: Query<&mut ImageNode, (With<HudTimerStopwatch>, Without<HudTimerDigit>)>,
    mut colon_q: Query<&mut BackgroundColor, With<HudTimerColonDot>>,
    mut digits_q: Query<(&HudTimerDigit, &mut ImageNode), Without<HudTimerStopwatch>>,
) {
    let Some(atlas) = atlas_opt else {
        return;
    };

    let secs = if let Some(c4) = planted_c4.iter().next() {
        c4.timer.max(0.0)
    } else {
        rules_state.rules.round_timer.max(0.0)
    };
    let is_c4_planted = planted_c4.iter().next().is_some();
    let color = if is_c4_planted && secs <= 15.0 {
        if ((time.elapsed_secs() * 4.0) as u32).is_multiple_of(2) {
            CS_HUD_RED
        } else {
            Color::srgba(0.65, 0.14, 0.08, 0.48)
        }
    } else {
        CS_HUD_AMBER
    };

    let total = secs.ceil() as usize;
    let mins = (total / 60).min(9);
    let rem_s = total % 60;
    let s_tens = rem_s / 10;
    let s_units = rem_s % 10;

    for mut sw in &mut stopwatch_q {
        sw.color = color;
    }
    for mut dot in &mut colon_q {
        dot.0 = color;
    }
    for (digit, mut img) in &mut digits_q {
        let d = match digit.0 {
            0 => mins,
            1 => s_tens,
            _ => s_units,
        };
        img.image = atlas.digits[d].clone();
        img.color = color;
    }
}

#[allow(clippy::type_complexity, clippy::too_many_lines)]
fn update_hud_weapon_menu_system(
    atlas_opt: Option<Res<HudAtlas>>,
    pending_opt: Option<Res<PendingWeaponSelection>>,
    inv_query: Query<&PlayerWeaponInventory>,
    mut root_query: Query<&mut Visibility, With<HudWeaponMenuRoot>>,
    mut icon_query: Query<(&HudWeaponBucketIcon, &mut ImageNode), Without<HudSlotCardSprite>>,
    mut slot_nodes: ParamSet<(
        Query<(&HudSlotInactiveContainer, &mut Node)>,
        Query<(&HudSlotActiveContainer, &mut Node)>,
        Query<(&HudSlotInactiveBox, &mut Node)>,
        Query<(&HudSlotCard, &mut Node)>,
        Query<(&HudSlotCardSprite, &mut ImageNode), Without<HudWeaponBucketIcon>>,
        Query<(&HudSlotCardSelection, &mut Visibility), Without<HudWeaponMenuRoot>>,
        Query<(&HudSlotCardAmmoTrack, &mut Visibility), Without<HudWeaponMenuRoot>>,
        Query<(&HudSlotCardAmmoFill, &mut Node, &mut BackgroundColor)>,
    )>,
) {
    let show = pending_opt.as_ref().is_some_and(|p| p.open);
    for mut vis in &mut root_query {
        let target = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target {
            *vis = target;
        }
    }
    if !show {
        return;
    }
    let (Some(atlas), Some(pending), Ok(inv)) = (atlas_opt, pending_opt, inv_query.get_single())
    else {
        return;
    };

    let sel_slot = pending.slot;
    let sel_row = pending.bucket_idx;

    for (bucket_icon, mut img) in &mut icon_query {
        let b_idx = (bucket_icon.0.saturating_sub(1) as usize).min(4);
        let has_any = !inv.bucket_weapons[b_idx].is_empty();
        img.color = if sel_slot == bucket_icon.0 {
            Color::srgba(1.0, 0.78, 0.0, 0.95)
        } else if has_any {
            CS_HUD_AMBER
        } else {
            Color::srgba(1.0, 0.65, 0.0, 0.25)
        };
    }

    for (c, mut node) in &mut slot_nodes.p0() {
        node.display = if c.0 == sel_slot {
            Display::None
        } else {
            Display::Flex
        };
    }
    for (c, mut node) in &mut slot_nodes.p1() {
        node.display = if c.0 == sel_slot {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (b, mut node) in &mut slot_nodes.p2() {
        let b_idx = (b.0.saturating_sub(1) as usize).min(4);
        let len = inv.bucket_weapons[b_idx].len().min(MAX_CARDS_PER_COL);
        node.display = if b.1 < len {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (c, mut node) in &mut slot_nodes.p3() {
        let b_idx = (c.0.saturating_sub(1) as usize).min(4);
        let len = inv.bucket_weapons[b_idx].len().min(MAX_CARDS_PER_COL);
        node.display = if c.0 == sel_slot && c.1 < len {
            Display::Flex
        } else {
            Display::None
        };
    }

    for (s, mut img) in &mut slot_nodes.p4() {
        if s.0 != sel_slot {
            continue;
        }
        let b_idx = (s.0.saturating_sub(1) as usize).min(4);
        if let Some(item) = inv.bucket_weapons[b_idx].get(s.1)
            && let Some(id) = item.id
            && let Some(card) = atlas.weapon_cards.get(&id)
        {
            let is_selected = s.1 == sel_row;
            img.image = if is_selected {
                card.selected.clone()
            } else {
                card.normal.clone()
            };
            img.color = if is_selected {
                Color::srgba(1.0, 0.78, 0.05, 0.96)
            } else {
                CS_HUD_AMBER
            };
        }
    }

    for (sel, mut vis) in &mut slot_nodes.p5() {
        *vis = if sel.0 == sel_slot && sel.1 == sel_row {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (trk, mut vis) in &mut slot_nodes.p6() {
        let b_idx = (trk.0.saturating_sub(1) as usize).min(4);
        let visible = trk.0 == sel_slot
            && inv.bucket_weapons[b_idx]
                .get(trk.1)
                .and_then(|it| it.id)
                .is_some_and(|id| id != WeaponId::Knife);
        *vis = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (fill, mut node, mut bg) in &mut slot_nodes.p7() {
        if fill.0 != sel_slot {
            continue;
        }
        let b_idx = (fill.0.saturating_sub(1) as usize).min(4);
        if let Some(item) = inv.bucket_weapons[b_idx].get(fill.1)
            && let Some(id) = item.id
            && id != WeaponId::Knife
        {
            let def = get_weapon_def(id);
            let (total, max_total) = if def.uses_clip {
                (
                    (item.clip.max(0) + item.reserve.max(0)) as f32,
                    (def.clip_size + def.max_reserve).max(1) as f32,
                )
            } else {
                let count = item.reserve.max(item.clip).max(0) as f32;
                (count, def.max_reserve.max(1) as f32)
            };
            let ratio = (total / max_total).clamp(0.0, 1.0);
            node.width = Val::Px(if total > 0.0 {
                (ratio * 20.0).max(3.0)
            } else {
                0.0
            });
            bg.0 = if total > 0.0 {
                Color::srgba(0.0, 0.88, 0.0, 0.95)
            } else {
                CS_HUD_RED
            };
        }
    }
}

fn update_hud_money_system(
    atlas_opt: Option<Res<HudAtlas>>,
    player_query: Query<&PlayerController>,
    mut money_digits: Query<(&HudMoneyDigit, &mut ImageNode, &mut Visibility)>,
) {
    let Some(atlas) = atlas_opt else {
        return;
    };
    let Ok(player) = player_query.get_single() else {
        return;
    };

    let money = player.money.clamp(0, 16000) as usize;
    for (digit, mut img, mut vis) in &mut money_digits {
        let (d, show) = digit_at_and_visible(money, digit.0, 5, true);
        img.image = atlas.digits[d].clone();
        *vis = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn update_hud_scenario_system(
    time: Res<Time>,
    rules_state: Res<text_channels::CsGameRulesState>,
    player_query: Query<&PlayerController>,
    inv_query: Query<&PlayerWeaponInventory>,
    planted_c4: Query<&PlantedC4>,
    mut status_query: Query<(&HudStatusIcon, &mut ImageNode, &mut Node, &mut Visibility)>,
) {
    let Ok(player) = player_query.get_single() else {
        return;
    };
    let has_c4 = inv_query
        .get_single()
        .is_ok_and(|inv| inv.slot5.id == Some(WeaponId::C4));
    let is_planted = !planted_c4.is_empty();
    let c4_blink = player.in_bomb_zone && ((time.elapsed_secs() * 10.0) as u32).is_multiple_of(2);

    for (icon, mut img, mut node, mut vis) in &mut status_query {
        let (active, color) = match icon.0 {
            StatusIconKind::BuyZone => (
                rules_state
                    .rules
                    .can_team_buy(player.team, player.in_buy_zone),
                CS_HUD_C4_GREEN,
            ),
            StatusIconKind::C4 => (
                has_c4 && !is_planted,
                if c4_blink {
                    CS_HUD_RED
                } else {
                    CS_HUD_C4_GREEN
                },
            ),
            StatusIconKind::Defuser => (
                player.team == goldhub_game_cstrike::CsTeam::CounterTerrorist && player.has_defuser,
                CS_HUD_C4_GREEN,
            ),
            StatusIconKind::Rescue => (player.in_rescue_zone, CS_HUD_C4_GREEN),
            StatusIconKind::Escape => (player.in_escape_zone, CS_HUD_C4_GREEN),
            StatusIconKind::VipSafety => (player.in_vip_safety_zone, CS_HUD_C4_GREEN),
        };

        let target_disp = if active { Display::Flex } else { Display::None };
        let target_vis = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if node.display != target_disp {
            node.display = target_disp;
        }
        if *vis != target_vis {
            *vis = target_vis;
        }
        if active {
            img.color = color;
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_hud_ammo_system(
    atlas_opt: Option<Res<HudAtlas>>,
    weapon_query: Query<&ActiveWeaponState>,
    mut ammo_nodes: ParamSet<(
        Query<&mut Visibility, With<HudAmmoRoot>>,
        Query<(&HudClipDigit, &mut ImageNode, &mut Visibility, &mut Node)>,
        Query<&mut Node, With<HudAmmoDivider>>,
        Query<(&HudReserveDigit, &mut ImageNode, &mut Visibility)>,
        Query<(&mut ImageNode, &mut Visibility), With<HudAmmoCaliberIcon>>,
    )>,
) {
    let Ok(state) = weapon_query.get_single() else {
        return;
    };
    let Some(atlas) = atlas_opt else {
        return;
    };

    if state.id == WeaponId::Knife {
        for mut vis in &mut ammo_nodes.p0() {
            *vis = Visibility::Hidden;
        }
        return;
    }
    for mut vis in &mut ammo_nodes.p0() {
        *vis = Visibility::Inherited;
    }

    let def = get_weapon_def(state.id);
    if def.uses_clip {
        let clip = state.clip.clamp(0, 99) as usize;
        for (digit, mut img, mut vis, mut node) in &mut ammo_nodes.p1() {
            node.display = Display::Flex;
            let (d, show) = digit_at_and_visible(clip, digit.0, 2, true);
            img.image = atlas.digits[d].clone();
            *vis = if show {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        for mut div_node in &mut ammo_nodes.p2() {
            div_node.display = Display::Flex;
        }
    } else {
        for (_, _, _, mut node) in &mut ammo_nodes.p1() {
            node.display = Display::None;
        }
        for mut div_node in &mut ammo_nodes.p2() {
            div_node.display = Display::None;
        }
    }

    let count_val = if def.uses_clip {
        state.reserve.clamp(0, 999) as usize
    } else {
        state.reserve.max(state.clip).clamp(0, 999) as usize
    };
    for (digit, mut img, mut vis) in &mut ammo_nodes.p3() {
        let (d, show) = digit_at_and_visible(count_val, digit.0, 3, true);
        img.image = atlas.digits[d].clone();
        *vis = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (mut cal_img, mut cal_vis) in &mut ammo_nodes.p4() {
        if let Some(icon) = atlas.ammo_icons.get(&def.caliber) {
            cal_img.image = icon.clone();
            *cal_vis = Visibility::Inherited;
        } else {
            *cal_vis = Visibility::Hidden;
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn update_hud_bartime_and_flash_system(
    loc_opt: Option<Res<text_channels::CsLocalizationResource>>,
    player_query: Query<&PlayerController>,
    weapon_query: Query<&ActiveWeaponState>,
    planted_c4_query: Query<&PlantedC4>,
    mut bartime_root: Query<
        &mut Visibility,
        (
            With<HudBarTimeRoot>,
            Without<HudFlashOverlay>,
            Without<HudCenterMessage>,
        ),
    >,
    mut bartime_fill: Query<&mut Node, With<HudBarTimeFill>>,
    mut msg_query: Query<
        (&mut Text, &mut Visibility),
        (
            With<HudCenterMessage>,
            Without<HudBarTimeRoot>,
            Without<HudFlashOverlay>,
        ),
    >,
    mut flash_query: Query<
        (&mut BackgroundColor, &mut Visibility),
        (
            With<HudFlashOverlay>,
            Without<HudBarTimeRoot>,
            Without<HudCenterMessage>,
        ),
    >,
) {
    let arm_progress = weapon_query
        .get_single()
        .ok()
        .filter(|w| w.c4_arming)
        .map(|w| (w.c4_arm_timer / 3.0).clamp(0.0, 1.0));
    let defuse_progress = planted_c4_query
        .iter()
        .find(|c4| c4.is_being_defused && c4.defuse_duration > 0.0)
        .map(|c4| (c4.defuse_progress / c4.defuse_duration).clamp(0.0, 1.0));
    let bar_progress = arm_progress.or(defuse_progress);

    for mut vis in &mut bartime_root {
        *vis = if bar_progress.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(prog) = bar_progress {
        for mut fill_node in &mut bartime_fill {
            fill_node.width = Val::Percent(prog * 100.0);
        }
    }

    if let Ok(player) = player_query.get_single() {
        let show_msg = player.center_hud_timer > 0.0 && !player.center_hud_msg.is_empty();
        for (mut txt, mut vis) in &mut msg_query {
            if show_msg {
                let localized = loc_opt
                    .as_deref()
                    .map_or(player.center_hud_msg.as_str(), |l| {
                        l.tr(&player.center_hud_msg)
                    });
                if **txt != localized {
                    (**txt) = localized.to_string();
                }
                *vis = Visibility::Inherited;
            } else {
                *vis = Visibility::Hidden;
            }
        }

        let alpha = player.current_flash_render_alpha();
        for (mut bg, mut vis) in &mut flash_query {
            if alpha > 0.002 {
                bg.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
                *vis = Visibility::Inherited;
            } else {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Updates right-side weapon / ammo / equipment pickup history stack (`CHudHistoryResource` parity).
#[allow(clippy::option_if_let_else)]
pub fn update_hud_pickup_history_system(
    time: Res<Time>,
    atlas_opt: Option<Res<HudAtlas>>,
    mut pickup_events: EventReader<ItemPickupEvent>,
    mut history: ResMut<HudHistoryResource>,
    mut slot_query: Query<(&HudPickupSlot, &mut ImageNode, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    if dt > 0.0 {
        for entry in &mut history.entries {
            entry.lifetime += dt;
        }
        history.entries.retain(|e| e.lifetime < 3.5);
    }

    if let Some(ref atlas) = atlas_opt {
        for ev in pickup_events.read() {
            let img_opt = if let Some(wid) = ev.weapon_id {
                if wid == WeaponId::C4 {
                    Some(atlas.c4_icon.clone())
                } else {
                    atlas.weapon_cards.get(&wid).map(|c| c.normal.clone())
                }
            } else if let Some(cal) = ev.ammo_caliber {
                atlas.ammo_icons.get(&cal).cloned()
            } else if ev.is_defuser {
                Some(atlas.defuser_icon.clone())
            } else {
                None
            };

            if let Some(img) = img_opt {
                history.entries.insert(
                    0,
                    PickupHistoryEntry {
                        image: img,
                        lifetime: 0.0,
                    },
                );
                if history.entries.len() > 4 {
                    history.entries.truncate(4);
                }
            }
        }
    }

    for (slot, mut img_node, mut vis) in &mut slot_query {
        if let Some(entry) = history.entries.get(slot.0) {
            let fade = ((3.5 - entry.lifetime) / 0.8).clamp(0.0, 1.0);
            img_node.image = entry.image.clone();
            img_node.color = Color::srgba(1.0, 0.72, 0.12, 0.95 * fade);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_dynamic_crosshair_system(
    mode: Res<CameraControlMode>,
    perspective: Res<CameraPerspective>,
    player_query: Query<&PlayerController>,
    weapon_query: Query<(&ActiveWeaponState, &PlayerRecoil)>,
    mut root_query: Query<&mut Visibility, (With<CrosshairRoot>, Without<HudSniperScopeRoot>)>,
    mut scope_query: Query<&mut Visibility, (With<HudSniperScopeRoot>, Without<CrosshairRoot>)>,
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

    let first_person_player =
        *mode == CameraControlMode::Player && *perspective == CameraPerspective::FirstPerson;
    let default_state = ActiveWeaponState::default();
    let default_recoil = PlayerRecoil::default();
    let (state, recoil) = weapon_query
        .get_single()
        .unwrap_or((&default_state, &default_recoil));

    let is_sniper = state.id.is_sniper_scope();
    let scope_active = first_person_player && is_sniper && state.current_fov <= 40.5;
    for mut s_vis in &mut scope_query {
        let target = if scope_active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *s_vis != target {
            *s_vis = target;
        }
    }

    if !first_person_player || !state.id.has_crosshair() || scope_active {
        *root_vis = Visibility::Hidden;
        return;
    }
    *root_vis = Visibility::Inherited;

    let Ok(player) = player_query.get_single() else {
        return;
    };

    // Dynamic crosshair spread expansion matching CS 1.6 cl_dynamiccrosshair
    let speed = player.sim.current.horizontal_speed();
    let on_ground = player.sim.current.on_ground;
    let is_ducking = player.sim.current.is_ducking;

    let base_gap = if state.zoom_level > 0 {
        3.0
    } else if is_ducking {
        4.0
    } else {
        6.0
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leading_zero_suppression_across_widths() {
        assert_eq!(digit_at_and_visible(64, 0, 3, true), (0, false));
        assert_eq!(digit_at_and_visible(64, 1, 3, true), (6, true));
        assert_eq!(digit_at_and_visible(64, 2, 3, true), (4, true));

        assert_eq!(digit_at_and_visible(0, 0, 3, true), (0, false));
        assert_eq!(digit_at_and_visible(0, 1, 3, true), (0, false));
        assert_eq!(digit_at_and_visible(0, 2, 3, true), (0, true));

        assert_eq!(digit_at_and_visible(800, 0, 5, true), (0, false));
        assert_eq!(digit_at_and_visible(800, 1, 5, true), (0, false));
        assert_eq!(digit_at_and_visible(800, 2, 5, true), (8, true));
    }

    #[test]
    fn test_weapon_card_specs_cover_all_weapons() {
        for wid in WeaponId::all() {
            assert!(
                WEAPON_CARD_SPECS.iter().any(|spec| spec.0 == wid),
                "Missing HUD card spec for {wid:?}"
            );
        }
    }

    #[test]
    fn test_hud_health_cross_and_digits_color_sync() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);

        let dummy_img = Handle::default();
        let atlas = HudAtlas {
            digits: std::array::from_fn(|_| dummy_img.clone()),
            divider: dummy_img.clone(),
            cross: dummy_img.clone(),
            suit_full: dummy_img.clone(),
            flash_empty: dummy_img.clone(),
            flash_full: dummy_img.clone(),
            flash_beam: dummy_img.clone(),
            dollar: dummy_img.clone(),
            c4_icon: dummy_img.clone(),
            defuser_icon: dummy_img.clone(),
            buyzone_icon: dummy_img.clone(),
            rescue_icon: dummy_img.clone(),
            escape_icon: dummy_img.clone(),
            vipsafety_icon: dummy_img.clone(),
            stopwatch: dummy_img.clone(),
            buckets: std::array::from_fn(|_| dummy_img.clone()),
            selection_frame: None,
            sniper_scope: None,
            scope_bezel: dummy_img.clone(),
            ammo_icons: HashMap::new(),
            weapon_cards: HashMap::new(),
        };
        app.insert_resource(atlas);

        let cross_ent = app
            .world_mut()
            .spawn((
                HudHealthCrossIcon,
                ImageNode::new(dummy_img.clone()).with_color(CS_HUD_AMBER),
            ))
            .id();

        let digit_ent = app
            .world_mut()
            .spawn((
                HudHealthDigit(2),
                ImageNode::new(dummy_img).with_color(CS_HUD_AMBER),
                Visibility::Inherited,
            ))
            .id();

        let player_ent = app
            .world_mut()
            .spawn(PlayerController {
                health: 100.0,
                ..default()
            })
            .id();

        app.add_systems(Update, update_hud_health_armor_system);
        app.update();

        // Full HP -> amber cross and digits
        assert_eq!(
            app.world().get::<ImageNode>(cross_ent).unwrap().color,
            CS_HUD_AMBER
        );
        assert_eq!(
            app.world().get::<ImageNode>(digit_ent).unwrap().color,
            CS_HUD_AMBER
        );

        // Low HP (<= 25) -> cross and digits turn red in sync
        app.world_mut()
            .get_mut::<PlayerController>(player_ent)
            .unwrap()
            .health = 20.0;
        app.update();

        assert_eq!(
            app.world().get::<ImageNode>(cross_ent).unwrap().color,
            CS_HUD_RED
        );
        assert_eq!(
            app.world().get::<ImageNode>(digit_ent).unwrap().color,
            CS_HUD_RED
        );
    }

    #[test]
    fn test_hud_pickup_history_stack_and_fade() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.init_resource::<HudHistoryResource>();
        app.add_event::<ItemPickupEvent>();

        let dummy_img = Handle::default();
        let atlas = HudAtlas {
            digits: std::array::from_fn(|_| dummy_img.clone()),
            divider: dummy_img.clone(),
            cross: dummy_img.clone(),
            suit_full: dummy_img.clone(),
            flash_empty: dummy_img.clone(),
            flash_full: dummy_img.clone(),
            flash_beam: dummy_img.clone(),
            dollar: dummy_img.clone(),
            c4_icon: dummy_img.clone(),
            defuser_icon: dummy_img.clone(),
            buyzone_icon: dummy_img.clone(),
            rescue_icon: dummy_img.clone(),
            escape_icon: dummy_img.clone(),
            vipsafety_icon: dummy_img.clone(),
            stopwatch: dummy_img.clone(),
            buckets: std::array::from_fn(|_| dummy_img.clone()),
            selection_frame: None,
            sniper_scope: None,
            scope_bezel: dummy_img,
            ammo_icons: HashMap::new(),
            weapon_cards: HashMap::new(),
        };
        app.insert_resource(atlas);

        let slot0 = app
            .world_mut()
            .spawn((HudPickupSlot(0), ImageNode::default(), Visibility::Hidden))
            .id();

        app.add_systems(Update, update_hud_pickup_history_system);

        app.world_mut().send_event(ItemPickupEvent {
            weapon_id: Some(WeaponId::C4),
            ammo_caliber: None,
            is_defuser: false,
        });
        app.update();

        assert_eq!(
            *app.world().get::<Visibility>(slot0).unwrap(),
            Visibility::Inherited
        );
        let history = app.world().resource::<HudHistoryResource>();
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].lifetime, 0.0);
    }
}
