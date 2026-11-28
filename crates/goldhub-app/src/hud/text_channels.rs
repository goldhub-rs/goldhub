//! GoldSrc 4-channel HUD text runtime, localization, hybrid fonts (`QFONT` + `Verdana`), and CS 1.6 scenario rules.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use goldhub_formats::codecs::AssetDecoder;
use goldhub_formats::codecs::legacy::{QFontData, TYP_QFONT, Wad3Decoder};
use goldhub_game_cstrike::{
    CsGameRules, CsLocalizationTable, CsTeam, RoundEndReason, RoundState, ScenarioTickCounts,
};

use crate::audio::{GameSearchPaths, GoldSrcAudioBank};
use crate::combat::weapon::{ActiveWeaponState, HudFastSwitch, PlantedC4, PlayerWeaponInventory};
use crate::entity::{BombMapParams, RoundResetEvent, ScenarioMapData};
use crate::player::PlayerController;
use crate::sprite::resolve_file_path;
use crate::studio::world_entity::{HostageEntity, HostageState};

const MAX_CONSOLE_LINES: usize = 64;
const MAX_CHAT_LINES: usize = 5;
const MAX_NOTIFY_LINES: usize = 4;

/// GoldSrc engine text channels matching `HUD_PRINT*` (`const.h` / `CHudTextMessage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudPrintChannel {
    Center,
    Talk,
    Notify,
    Console,
}

/// Event requesting localized output onto one of the 4 GoldSrc HUD/console text channels.
#[derive(Event, Debug, Clone)]
pub struct HudPrintEvent {
    pub channel: HudPrintChannel,
    pub text: String,
    pub duration: f32,
}

/// Resource storing CS 1.6 UTF-16LE/ASCII localization (`cstrike_russian.txt` -> `cstrike_english.txt` -> `titles.txt`).
#[derive(Resource, Clone, Default)]
pub struct CsLocalizationResource {
    pub table: CsLocalizationTable,
}

impl CsLocalizationResource {
    /// Loads localization dictionaries in canonical GoldSrc priority order from `search_paths`.
    #[must_use]
    pub fn load_from_search_paths(search_paths: &GameSearchPaths) -> Self {
        let mut table = CsLocalizationTable::new();
        for (rel_path, is_titles) in [
            ("resource/cstrike_russian.txt", false),
            ("resource/cstrike_english.txt", false),
            ("titles.txt", true),
        ] {
            if let Some(path) = resolve_file_path(rel_path, search_paths)
                && let Ok(bytes) = fs::read(path)
            {
                if is_titles {
                    table.load_titles_txt_bytes(&bytes, false);
                } else {
                    table.load_vdf_bytes(&bytes, false);
                }
            }
        }
        Self { table }
    }

    /// Resolves a localization token (`#Bomb_Planted`, `Weapon_Cannot_Be_Dropped`) or returns plain text.
    #[must_use]
    pub fn tr<'a>(&'a self, key_or_text: &'a str) -> &'a str {
        let first = self.table.resolve(key_or_text);
        if first.starts_with('#') && first != key_or_text {
            self.table.resolve(first)
        } else {
            first
        }
    }
}

/// Loaded GoldSrc `fonts.wad` `QFONT` bitmap font (`FONT0`, `FONT1`, `FONT2`) and `Verdana` TrueType UI font.
#[derive(Resource, Clone, Default)]
pub struct HudFontResources {
    pub ui_font: Handle<Font>,
    pub qfont0: Option<QFontData>,
    pub qfont1: Option<QFontData>,
    pub qfont2: Option<QFontData>,
}

impl HudFontResources {
    /// Blits an ASCII string into an RGBA bitmap using GoldSrc `FONT0` (`fonts.wad`).
    #[must_use]
    pub fn render_qfont_ascii_rgba(&self, text: &str, rgb: [u8; 3]) -> Option<(Vec<u8>, u32, u32)> {
        if !text.is_ascii() || text.is_empty() {
            return None;
        }
        let qf = self.qfont0.as_ref()?;
        let h = qf.row_height.max(1);
        let sw = qf.sheet_width;
        let total_w: u32 = text
            .bytes()
            .map(|b| u32::from(qf.glyphs[usize::from(b)].char_width.max(4)))
            .sum::<u32>()
            .max(1);
        let mut out = vec![0u8; (total_w * h * 4) as usize];
        let mut cursor_x = 0u32;

        for b in text.bytes() {
            let info = qf.glyphs[usize::from(b)];
            let cw = u32::from(info.char_width);
            if cw > 0 && cw <= sw {
                let src_x = u32::from(info.start_offset) % sw;
                let src_y = u32::from(info.start_offset) / sw;
                for row in 0..h {
                    let sy = src_y + row;
                    if sy >= qf.sheet_height {
                        break;
                    }
                    for col in 0..cw {
                        let sx = src_x + col;
                        if sx >= sw || cursor_x + col >= total_w {
                            break;
                        }
                        let src_idx = ((sy * sw + sx) * 4) as usize;
                        let a = qf.rgba_pixels.get(src_idx + 3).copied().unwrap_or(0);
                        if a > 0 {
                            let lum = u16::from(qf.rgba_pixels[src_idx])
                                .max(u16::from(qf.rgba_pixels[src_idx + 1]))
                                .max(u16::from(qf.rgba_pixels[src_idx + 2]));
                            let dst_idx = ((row * total_w + cursor_x + col) * 4) as usize;
                            out[dst_idx] = ((u16::from(rgb[0]) * lum) / 255) as u8;
                            out[dst_idx + 1] = ((u16::from(rgb[1]) * lum) / 255) as u8;
                            out[dst_idx + 2] = ((u16::from(rgb[2]) * lum) / 255) as u8;
                            out[dst_idx + 3] = a;
                        }
                    }
                }
            }
            cursor_x += cw.max(4);
        }
        Some((out, total_w, h))
    }
}

fn load_system_or_game_verdana(search_paths: &GameSearchPaths) -> Option<Vec<u8>> {
    for rel in ["resource/fonts/verdana.ttf", "fonts/verdana.ttf"] {
        if let Some(p) = resolve_file_path(rel, search_paths)
            && let Ok(bytes) = fs::read(p)
        {
            return Some(bytes);
        }
    }
    if let Some(windir) = std::env::var_os("WINDIR") {
        let p = PathBuf::from(windir).join("Fonts").join("verdana.ttf");
        if p.exists()
            && let Ok(bytes) = fs::read(p)
        {
            return Some(bytes);
        }
    }
    for unix_path in [
        "/usr/share/fonts/truetype/msttcorefonts/Verdana.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        let p = Path::new(unix_path);
        if p.exists()
            && let Ok(bytes) = fs::read(p)
        {
            return Some(bytes);
        }
    }
    None
}

/// Unpacks `fonts.wad` (`QFONT`) and `Verdana` UI font dynamically from `search_paths` and OS font paths.
#[must_use]
pub fn load_hud_fonts(
    search_paths: &GameSearchPaths,
    fonts: &mut Assets<Font>,
) -> HudFontResources {
    let ui_font = load_system_or_game_verdana(search_paths)
        .and_then(|bytes| Font::try_from_bytes(bytes).ok())
        .map_or_else(Handle::default, |f| fonts.add(f));

    let mut qfont0 = None;
    let mut qfont1 = None;
    let mut qfont2 = None;
    if let Some(wad_path) = resolve_file_path("fonts.wad", search_paths)
        && let Ok(raw) = fs::read(wad_path)
        && let Ok(wad) = Wad3Decoder.decode(&raw)
    {
        for (name, dst) in [
            ("FONT0", &mut qfont0),
            ("FONT1", &mut qfont1),
            ("FONT2", &mut qfont2),
        ] {
            if let Some(entry) = wad.find_entry(name)
                && entry.entry_type == TYP_QFONT
            {
                *dst = wad.unpack_qfont(entry).ok();
            }
        }
    }

    HudFontResources {
        ui_font,
        qfont0,
        qfont1,
        qfont2,
    }
}

/// Toggleable `developer 1` top-right telemetry overlay (`F8` key or console `developer 1`).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct DeveloperMode(pub bool);

/// State of the interactive CS 1.6 `~` console drawer (`HUD_PRINTCONSOLE`).
#[derive(Resource, Debug, Clone)]
pub struct ConsoleOverlayState {
    pub open: bool,
    pub input: String,
    pub lines: VecDeque<String>,
}

impl Default for ConsoleOverlayState {
    fn default() -> Self {
        let mut lines = VecDeque::with_capacity(MAX_CONSOLE_LINES);
        lines
            .push_back("GoldHub CS 1.6 Engine initialized (~ toggle console, F8 developer)".into());
        Self {
            open: false,
            input: String::new(),
            lines,
        }
    }
}

/// Transient feed lines for `HUD_PRINTTALK` (bottom-left) and `HUD_PRINTNOTIFY` (top-left).
#[derive(Resource, Debug, Clone, Default)]
pub struct HudFeedState {
    pub talk: VecDeque<(String, f32)>,
    pub notify: VecDeque<(String, f32)>,
}

/// Authoritative CS 1.6 round FSM wrapping [`goldhub_game_cstrike::CsGameRules`].
#[derive(Resource, Debug, Clone)]
pub struct CsGameRulesState {
    pub rules: CsGameRules,
    pub synced_map: bool,
}

impl Default for CsGameRulesState {
    fn default() -> Self {
        let mut rules = CsGameRules::new(0.0, 105.0, false);
        rules.state = RoundState::RoundLive;
        rules.round_timer = rules.round_time_limit;
        Self {
            rules,
            synced_map: false,
        }
    }
}

#[derive(Component)]
pub struct HudTalkLine(pub usize);

#[derive(Component)]
pub struct HudNotifySlotText(pub usize);

#[derive(Component)]
pub struct HudNotifySlotBitmap {
    pub slot: usize,
    pub last_text: Option<String>,
}

#[derive(Component)]
pub struct HudDeveloperText;

#[derive(Component)]
pub struct HudConsoleRoot;

#[derive(Component)]
pub struct HudConsoleBodyText;

#[derive(Component)]
pub struct HudConsoleInputText;

#[derive(Component)]
pub struct HudBuyMenuRoot;

#[derive(Component)]
pub struct HudBuyMenuText;

/// Spawns `HUD_PRINTTALK`, `HUD_PRINTNOTIFY`, `developer 1`, and `~` `HUD_PRINTCONSOLE` overlay nodes.
#[allow(clippy::too_many_lines)]
pub fn spawn_text_channel_nodes(commands: &mut Commands, font: Handle<Font>) {
    // 1. `HUD_PRINTTALK` bottom-left chat feed above Health bar
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(54.0),
            left: Val::Px(24.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|chat| {
            for idx in 0..MAX_CHAT_LINES {
                chat.spawn((
                    HudTalkLine(idx),
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.72, 0.22, 0.94)),
                    Visibility::Hidden,
                ));
            }
        });

    // 2. `HUD_PRINTNOTIFY` top-left notify lines (`QFONT` bitmap for pure ASCII, `Verdana` fallback for Cyrillic)
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(150.0),
            left: Val::Px(12.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|notify| {
            for idx in 0..MAX_NOTIFY_LINES {
                notify.spawn((
                    HudNotifySlotText(idx),
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.72, 0.0, 0.95)),
                    Visibility::Hidden,
                ));
                notify.spawn((
                    HudNotifySlotBitmap {
                        slot: idx,
                        last_text: None,
                    },
                    ImageNode::default(),
                    Node {
                        height: Val::Px(16.0),
                        width: Val::Auto,
                        display: Display::None,
                        ..default()
                    },
                    Visibility::Hidden,
                ));
            }
        });

    // 3. Top-right `developer 1` debug HUD (`F8`)
    commands.spawn((
        HudDeveloperText,
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgba(0.85, 0.95, 0.70, 0.90)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(64.0),
            right: Val::Px(24.0),
            ..default()
        },
        Visibility::Hidden,
    ));

    // 4. `~` Half-screen `HUD_PRINTCONSOLE` drawer
    spawn_console_drawer(commands, font.clone());

    // 5. Authentic CS 1.6 VGUI/Text Buy Menu
    commands
        .spawn((
            HudBuyMenuRoot,
            BackgroundColor(Color::srgba(0.04, 0.05, 0.06, 0.88)),
            BorderColor(Color::srgba(1.0, 0.65, 0.15, 0.40)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(20.0),
                left: Val::Px(24.0),
                min_width: Val::Px(260.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(14.0)),
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(4.0),
                ..default()
            },
            GlobalZIndex(85),
            Visibility::Hidden,
        ))
        .with_children(|menu| {
            menu.spawn((
                HudBuyMenuText,
                Text::new(""),
                TextFont {
                    font,
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.70, 0.18, 0.96)),
            ));
        });
}

fn spawn_console_drawer(commands: &mut Commands, font: Handle<Font>) {
    commands
        .spawn((
            HudConsoleRoot,
            BackgroundColor(Color::srgba(0.08, 0.10, 0.11, 0.84)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(42.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            GlobalZIndex(90),
            Visibility::Hidden,
        ))
        .with_children(|console| {
            console.spawn((
                HudConsoleBodyText,
                Text::new(""),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.88, 0.85)),
            ));
            console.spawn((
                HudConsoleInputText,
                Text::new("] "),
                TextFont {
                    font,
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.78, 0.15)),
            ));
        });
}

/// Dispatches `HudPrintEvent` messages into localized Center, Talk, Notify (`QFONT`), and Console feeds.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub fn process_hud_print_events_system(
    time: Res<Time>,
    mut events: EventReader<HudPrintEvent>,
    loc: Res<CsLocalizationResource>,
    mut feed: ResMut<HudFeedState>,
    mut console: ResMut<ConsoleOverlayState>,
    mut player_query: Query<&mut PlayerController>,
    mut images: ResMut<Assets<Image>>,
    font_res: Res<HudFontResources>,
    mut talk_q: Query<(&HudTalkLine, &mut Text, &mut Visibility)>,
    mut notify_text_q: Query<
        (&HudNotifySlotText, &mut Text, &mut Visibility),
        Without<HudTalkLine>,
    >,
    mut notify_bmp_q: Query<
        (
            &mut HudNotifySlotBitmap,
            &mut ImageNode,
            &mut Node,
            &mut Visibility,
        ),
        (Without<HudTalkLine>, Without<HudNotifySlotText>),
    >,
) {
    let dt = time.delta_secs();
    for ev in events.read() {
        let localized = loc.tr(&ev.text).to_string();
        match ev.channel {
            HudPrintChannel::Center => {
                if let Ok(mut p) = player_query.get_single_mut() {
                    p.set_center_message(localized, ev.duration.max(1.0));
                }
            }
            HudPrintChannel::Talk => {
                if feed.talk.len() >= MAX_CHAT_LINES {
                    feed.talk.pop_front();
                }
                feed.talk
                    .push_back((localized.clone(), ev.duration.max(5.0)));
                push_console_line(&mut console, &localized);
            }
            HudPrintChannel::Notify => {
                if feed.notify.len() >= MAX_NOTIFY_LINES {
                    feed.notify.pop_front();
                }
                feed.notify
                    .push_back((localized.clone(), ev.duration.max(4.0)));
                push_console_line(&mut console, &localized);
            }
            HudPrintChannel::Console => {
                push_console_line(&mut console, &localized);
            }
        }
    }

    feed.talk.retain_mut(|(_, ttl)| {
        *ttl -= dt;
        *ttl > 0.0
    });
    feed.notify.retain_mut(|(_, ttl)| {
        *ttl -= dt;
        *ttl > 0.0
    });

    for (line, mut txt, mut vis) in &mut talk_q {
        if let Some((msg, _)) = feed.talk.get(line.0) {
            (**txt).clone_from(msg);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }

    for (slot, mut txt, mut vis) in &mut notify_text_q {
        if let Some((msg, _)) = feed.notify.get(slot.0)
            && !msg.is_ascii()
        {
            (**txt).clone_from(msg);
            *vis = Visibility::Inherited;
        } else {
            *vis = Visibility::Hidden;
        }
    }

    for (mut slot, mut img_node, mut ui_node, mut vis) in &mut notify_bmp_q {
        let current_msg = feed.notify.get(slot.slot).map(|(msg, _)| msg.as_str());
        if current_msg.is_none() || !current_msg.is_some_and(str::is_ascii) {
            if slot.last_text.is_some() {
                if img_node.image != Handle::default() {
                    images.remove(&img_node.image);
                    img_node.image = Handle::default();
                }
                slot.last_text = None;
            }
            ui_node.display = Display::None;
            *vis = Visibility::Hidden;
            continue;
        }

        let msg = current_msg.unwrap();
        if slot.last_text.as_deref() == Some(msg) {
            ui_node.display = Display::Flex;
            *vis = Visibility::Inherited;
            continue;
        }

        if img_node.image != Handle::default() {
            images.remove(&img_node.image);
        }

        if let Some((rgba, w, h)) = font_res.render_qfont_ascii_rgba(msg, [255, 180, 45]) {
            let img = Image::new(
                Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            img_node.image = images.add(img);
            ui_node.display = Display::Flex;
            ui_node.width = Val::Px(w as f32);
            ui_node.height = Val::Px(h as f32);
            *vis = Visibility::Inherited;
            slot.last_text = Some(msg.to_string());
        } else {
            img_node.image = Handle::default();
            ui_node.display = Display::None;
            *vis = Visibility::Hidden;
            slot.last_text = None;
        }
    }
}

fn push_console_line(console: &mut ConsoleOverlayState, line: &str) {
    for sub in line.lines() {
        if console.lines.len() >= MAX_CONSOLE_LINES {
            console.lines.pop_front();
        }
        console.lines.push_back(sub.to_string());
    }
}

/// Handles `~` (`Backquote`) console toggle & commands, plus `F8` `developer 1` HUD telemetry.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub fn update_console_and_dev_hud_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut dev_mode: ResMut<DeveloperMode>,
    mut console: ResMut<ConsoleOverlayState>,
    mut fast_switch: Option<ResMut<HudFastSwitch>>,
    mut reset_events: EventWriter<RoundResetEvent>,
    mut print_events: EventWriter<HudPrintEvent>,
    mut rules_state: ResMut<CsGameRulesState>,
    mut player_query: Query<(
        &mut PlayerController,
        Option<&mut PlayerWeaponInventory>,
        Option<&ActiveWeaponState>,
    )>,
    loc: Res<CsLocalizationResource>,
    mut root_q: Query<&mut Visibility, (With<HudConsoleRoot>, Without<HudDeveloperText>)>,
    mut dev_q: Query<(&mut Text, &mut Visibility), With<HudDeveloperText>>,
    mut body_q: Query<
        &mut Text,
        (
            With<HudConsoleBodyText>,
            Without<HudDeveloperText>,
            Without<HudConsoleInputText>,
        ),
    >,
    mut input_q: Query<
        &mut Text,
        (
            With<HudConsoleInputText>,
            Without<HudDeveloperText>,
            Without<HudConsoleBodyText>,
        ),
    >,
) {
    if keyboard.just_pressed(KeyCode::F8) {
        dev_mode.0 = !dev_mode.0;
        print_events.send(HudPrintEvent {
            channel: HudPrintChannel::Notify,
            text: format!("developer {}", u8::from(dev_mode.0)),
            duration: 3.0,
        });
    }

    if keyboard.just_pressed(KeyCode::Backquote) {
        console.open = !console.open;
    }

    if console.open {
        for key in keyboard.get_just_pressed() {
            match key {
                KeyCode::Backspace => {
                    console.input.pop();
                }
                KeyCode::Enter => {
                    let cmd = console.input.trim().to_string();
                    console.input.clear();
                    if !cmd.is_empty() {
                        push_console_line(&mut console, &format!("] {cmd}"));
                        let player_opt = player_query.get_single_mut().ok().map(|(p, inv, _)| {
                            (p.into_inner(), inv.map(bevy::prelude::Mut::into_inner))
                        });
                        execute_console_command(
                            &cmd,
                            &mut dev_mode,
                            &mut console,
                            fast_switch.as_deref_mut(),
                            &mut rules_state,
                            &mut reset_events,
                            player_opt,
                            &loc,
                        );
                    }
                }
                other => {
                    if let Some(ch) = console_key_char(*other) {
                        console.input.push(ch);
                    }
                }
            }
        }
    }

    for mut vis in &mut root_q {
        *vis = if console.open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if console.open {
        let start = console.lines.len().saturating_sub(14);
        let joined: String = console
            .lines
            .iter()
            .skip(start)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        for mut txt in &mut body_q {
            if **txt != joined {
                (**txt).clone_from(&joined);
            }
        }
        let prompt = format!("] {}_", console.input);
        for mut txt in &mut input_q {
            if **txt != prompt {
                (**txt).clone_from(&prompt);
            }
        }
    }

    for (mut txt, mut vis) in &mut dev_q {
        if dev_mode.0 {
            if let Ok((player, _, weapon)) = player_query.get_single() {
                let ori = player.sim.current.origin;
                let w_name = weapon.map_or("Knife", |w| crate::combat::get_weapon_def(w.id).name);
                let formatted = format!(
                    "dev 1 | {:?} {:?} t:{:.1} | T:{} CT:{} | {} (${}) | ({:.0},{:.0},{:.0})",
                    rules_state.rules.map_flags.primary_kind(),
                    rules_state.rules.state,
                    rules_state.rules.round_timer.max(0.0),
                    rules_state.rules.t_score,
                    rules_state.rules.ct_score,
                    w_name,
                    player.money,
                    ori[0],
                    ori[1],
                    ori[2],
                );
                if **txt != formatted {
                    **txt = formatted;
                }
            }
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        } else if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
}

const fn console_key_char(key: KeyCode) -> Option<char> {
    Some(match key {
        KeyCode::Space => ' ',
        KeyCode::Minus => '_',
        KeyCode::Digit0 => '0',
        KeyCode::Digit1 => '1',
        KeyCode::Digit2 => '2',
        KeyCode::Digit3 => '3',
        KeyCode::Digit4 => '4',
        KeyCode::Digit5 => '5',
        KeyCode::Digit6 => '6',
        KeyCode::Digit7 => '7',
        KeyCode::Digit8 => '8',
        KeyCode::Digit9 => '9',
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        _ => return None,
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn execute_console_command(
    cmd: &str,
    dev_mode: &mut DeveloperMode,
    console: &mut ConsoleOverlayState,
    fast_switch: Option<&mut HudFastSwitch>,
    rules_state: &mut CsGameRulesState,
    reset_events: &mut EventWriter<RoundResetEvent>,
    mut player_opt: Option<(&mut PlayerController, Option<&mut PlayerWeaponInventory>)>,
    loc: &CsLocalizationResource,
) {
    let mut parts = cmd.split_whitespace();
    let Some(head) = parts.next() else {
        return;
    };
    let arg = parts.next();

    match head {
        "clear" => console.lines.clear(),
        "developer" => {
            if let Some(v) = arg {
                dev_mode.0 = v != "0";
            }
            push_console_line(
                console,
                &format!("\"developer\" is \"{}\"", u8::from(dev_mode.0)),
            );
        }
        "hud_fastswitch" => {
            if let Some(fs) = fast_switch {
                if let Some(v) = arg {
                    fs.0 = v != "0";
                }
                push_console_line(
                    console,
                    &format!("\"hud_fastswitch\" is \"{}\"", u8::from(fs.0)),
                );
            }
        }
        "sv_restart" | "sv_restartround" => {
            rules_state.rules.t_score = 0;
            rules_state.rules.ct_score = 0;
            rules_state.rules.state = RoundState::RoundLive;
            rules_state.rules.round_timer = rules_state.rules.round_time_limit;
            rules_state.rules.round_terminating = false;
            reset_events.send(RoundResetEvent);
            push_console_line(console, "The game will restart in 1 SECOND");
        }
        "status" => {
            push_console_line(
                console,
                &format!(
                    "scenario={:?} state={:?} T={} CT={} round_timer={:.1}",
                    rules_state.rules.map_flags.primary_kind(),
                    rules_state.rules.state,
                    rules_state.rules.t_score,
                    rules_state.rules.ct_score,
                    rules_state.rules.round_timer
                ),
            );
        }
        "money" => {
            if let Some(amt_str) = arg {
                if let Ok(amt) = amt_str.parse::<i32>()
                    && let Some((ref mut p, _)) = player_opt
                {
                    p.money = amt.clamp(0, 16000);
                    push_console_line(console, &format!("Player money set to ${}", p.money));
                }
            } else if let Some((ref p, _)) = player_opt {
                push_console_line(console, &format!("Current balance: ${}", p.money));
            }
        }
        "buy" => {
            if let Some(target) = arg {
                if let Some(item) = crate::combat::parse_buy_alias(target) {
                    if let Some((ref mut p, ref mut inv)) = player_opt {
                        match crate::combat::execute_buy_transaction(
                            p,
                            inv.as_deref_mut(),
                            &rules_state.rules,
                            item,
                        ) {
                            Ok((cost, _)) => push_console_line(
                                console,
                                &format!("Bought {} for ${cost} (balance: ${})", target, p.money),
                            ),
                            Err(loc_key) => {
                                let buy_sec = rules_state.rules.buy_time.round() as u32;
                                let msg = loc.tr(loc_key).replace("%s1", &buy_sec.to_string());
                                push_console_line(console, &msg);
                            }
                        }
                    } else {
                        push_console_line(console, "No active player found for transaction");
                    }
                } else {
                    push_console_line(console, &format!("Unknown buy alias: '{target}'"));
                }
            } else {
                push_console_line(
                    console,
                    "Usage: buy <item> (e.g. ak47, m4a1, awp, deagle, vest, vesthelm, defuser)",
                );
            }
        }
        "buyammo1" | "primammo" => {
            if let Some((ref mut p, ref mut inv)) = player_opt {
                match crate::combat::execute_buy_transaction(
                    p,
                    inv.as_deref_mut(),
                    &rules_state.rules,
                    goldhub_game_cstrike::BuyItem::PrimaryAmmo,
                ) {
                    Ok((cost, _)) => push_console_line(
                        console,
                        &format!("Bought primary ammunition for ${cost}"),
                    ),
                    Err(loc_key) => {
                        let buy_sec = rules_state.rules.buy_time.round() as u32;
                        let msg = loc.tr(loc_key).replace("%s1", &buy_sec.to_string());
                        push_console_line(console, &msg);
                    }
                }
            }
        }
        "buyammo2" | "secammo" => {
            if let Some((ref mut p, ref mut inv)) = player_opt {
                match crate::combat::execute_buy_transaction(
                    p,
                    inv.as_deref_mut(),
                    &rules_state.rules,
                    goldhub_game_cstrike::BuyItem::SecondaryAmmo,
                ) {
                    Ok((cost, _)) => push_console_line(
                        console,
                        &format!("Bought secondary ammunition for ${cost}"),
                    ),
                    Err(loc_key) => {
                        let buy_sec = rules_state.rules.buy_time.round() as u32;
                        let msg = loc.tr(loc_key).replace("%s1", &buy_sec.to_string());
                        push_console_line(console, &msg);
                    }
                }
            }
        }
        "autobuy" => {
            if let Some((ref mut p, ref mut inv)) = player_opt {
                let team = p.team;
                let weapons_to_try: &[&str] = match team {
                    goldhub_game_cstrike::CsTeam::CounterTerrorist => &[
                        "m4a1", "famas", "mp5", "vesthelm", "defuser", "primammo", "secammo",
                    ],
                    _ => &["ak47", "galil", "mp5", "vesthelm", "primammo", "secammo"],
                };
                for alias in weapons_to_try {
                    if let Some(item) = crate::combat::parse_buy_alias(alias) {
                        let _ = crate::combat::execute_buy_transaction(
                            p,
                            inv.as_deref_mut(),
                            &rules_state.rules,
                            item,
                        );
                    }
                }
                push_console_line(
                    console,
                    &format!("Autobuy completed. Balance: ${}", p.money),
                );
            }
        }
        _ => push_console_line(console, &format!("Unknown command: {head}")),
    }
}

/// Evaluates `CsGameRules` every tick across `de_`, `cs_`, `as_`, `es_`, plays win audio/banners, and triggers `RoundResetEvent`.
#[allow(clippy::too_many_arguments)]
pub fn update_cs_game_rules_system(
    mut commands: Commands,
    time: Res<Time>,
    map_data: Res<ScenarioMapData>,
    bomb_params: Res<BombMapParams>,
    mut rules_state: ResMut<CsGameRulesState>,
    mut player_query: Query<&mut PlayerController>,
    planted_c4_query: Query<(), With<PlantedC4>>,
    hostage_query: Query<&HostageEntity>,
    mut print_events: EventWriter<HudPrintEvent>,
    mut reset_events: EventWriter<RoundResetEvent>,
    search_paths: Option<Res<GameSearchPaths>>,
    mut audio_bank: Option<ResMut<GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
) {
    if !rules_state.synced_map {
        rules_state.rules.map_flags = goldhub_game_cstrike::MapScenarioFlags {
            has_bomb_target: map_data.has_bomb_target,
            has_bomb_zone: map_data.has_bomb_zone,
            has_rescue_zone: map_data.has_rescue_zone,
            has_vip_safety_zone: map_data.has_vip_safety_zone,
            has_escape_zone: map_data.has_escape_zone,
            has_buy_zone: map_data.has_buy_zone,
            ct_cant_buy: bomb_params.ct_cant_buy,
            t_cant_buy: bomb_params.t_cant_buy,
            bomb_radius: bomb_params.bomb_radius,
            hostage_count: hostage_query.iter().count() as u32,
        };
        rules_state.synced_map = true;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let prev_state = rules_state.rules.state;
    let Ok(mut player) = player_query.get_single_mut() else {
        return;
    };

    let mut counts = ScenarioTickCounts {
        bomb_planted: !planted_c4_query.is_empty(),
        target_bombed: map_data.bomb_exploded,
        bomb_defused: map_data.bomb_defused,
        hostages_total: hostage_query.iter().count() as u32,
        hostages_alive: hostage_query
            .iter()
            .filter(|h| !h.is_dead && h.state != HostageState::Rescued)
            .count() as u32,
        hostages_rescued: map_data.hostages_rescued,
        vip_present: player.is_vip,
        vip_escaped: player.is_vip && player.in_vip_safety_zone,
        vip_dead: player.is_vip && player.health <= 0.0,
        ..Default::default()
    };

    match player.team {
        CsTeam::Terrorist => {
            counts.spawnable_t = 1;
            if player.health > 0.0 {
                counts.alive_t = 1;
            } else {
                counts.dead_t = 1;
            }
        }
        CsTeam::CounterTerrorist => {
            counts.spawnable_ct = 1;
            if player.health > 0.0 {
                counts.alive_ct = 1;
            } else {
                counts.dead_ct = 1;
            }
        }
        _ => {}
    }

    if let Some(outcome) = rules_state.rules.evaluate_scenario(dt, counts) {
        let reward = match player.team {
            CsTeam::Terrorist => outcome.t_reward,
            CsTeam::CounterTerrorist => outcome.ct_reward,
            _ => 0,
        };
        player.money = (player.money + reward).clamp(0, 16000);
        let key = outcome.localization_key();
        print_events.send(HudPrintEvent {
            channel: HudPrintChannel::Center,
            text: key.to_string(),
            duration: 5.0,
        });
        print_events.send(HudPrintEvent {
            channel: HudPrintChannel::Notify,
            text: format!(
                "{:?} (T:{} CT:{})",
                outcome.reason, rules_state.rules.t_score, rules_state.rules.ct_score
            ),
            duration: 5.0,
        });

        if let (Some(paths), Some(ref mut bank), Some(ref mut sources)) =
            (search_paths, audio_bank.as_mut(), audio_sources.as_mut())
        {
            if outcome.reason == RoundEndReason::BombDefused
                && let Some(h) = bank.get_or_load_sound("radio/bombdef.wav", &paths.dirs, sources)
            {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
            if let Some(h) = bank.get_or_load_sound(outcome.radio_sound(), &paths.dirs, sources) {
                commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
        }
    }

    // RoundEnd 5.0s intermission finished -> transition back to active round and trigger `CleanUpMap`
    if prev_state == RoundState::RoundEnd && rules_state.rules.state != RoundState::RoundEnd {
        if rules_state.rules.state == RoundState::FreezeTime && rules_state.rules.freeze_time <= 0.0
        {
            rules_state.rules.state = RoundState::RoundLive;
            rules_state.rules.round_timer = rules_state.rules.round_time_limit;
        }
        reset_events.send(RoundResetEvent);
    }
}

/// Interactive CS 1.6 VGUI/Text Buy Menu controller system.
/// Handles `KeyB` toggle, `Escape` back/close, and numeric digits `0..9` to navigate categories and buy items.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub fn update_buy_menu_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut buy_menu: ResMut<crate::combat::BuyMenuState>,
    rules_state: Res<CsGameRulesState>,
    mut player_query: Query<(&mut PlayerController, Option<&mut PlayerWeaponInventory>)>,
    console: Res<ConsoleOverlayState>,
    search_paths: Option<Res<GameSearchPaths>>,
    mut audio_bank: Option<ResMut<GoldSrcAudioBank>>,
    mut audio_sources: Option<ResMut<Assets<AudioSource>>>,
    mut root_q: Query<&mut Visibility, (With<HudBuyMenuRoot>, Without<HudBuyMenuText>)>,
    mut text_q: Query<&mut Text, (With<HudBuyMenuText>, Without<HudBuyMenuRoot>)>,
    mut print_events: EventWriter<HudPrintEvent>,
    mut pickup_events: EventWriter<super::ItemPickupEvent>,
    loc: Res<CsLocalizationResource>,
) {
    if console.open {
        if buy_menu.is_open() {
            buy_menu.close();
        }
        for mut vis in &mut root_q {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let Ok((mut player, mut inv_opt)) = player_query.get_single_mut() else {
        return;
    };

    let mut play_sound = |snd_path: &str| {
        if let (Some(paths), Some(ref mut bank), Some(ref mut sources)) = (
            search_paths.as_ref(),
            audio_bank.as_mut(),
            audio_sources.as_mut(),
        ) && let Some(h) = bank.get_or_load_sound(snd_path, &paths.dirs, sources)
        {
            commands.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
        }
    };

    // Toggle with KeyB
    if keyboard.just_pressed(KeyCode::KeyB) {
        if buy_menu.is_open() {
            buy_menu.close();
            play_sound("common/wpn_hudoff.wav");
        } else {
            let can_buy = rules_state
                .rules
                .can_team_buy(player.team, player.in_buy_zone);
            if !player.in_buy_zone {
                play_sound("common/wpn_denyselect.wav");
                print_events.send(HudPrintEvent {
                    channel: HudPrintChannel::Notify,
                    text: loc.tr("#Cannot_Buy_This").to_string(),
                    duration: 3.0,
                });
            } else if !can_buy {
                play_sound("common/wpn_denyselect.wav");
                let buy_sec = rules_state.rules.buy_time.round() as u32;
                let msg = loc.tr("#Cant_buy").replace("%s1", &buy_sec.to_string());
                print_events.send(HudPrintEvent {
                    channel: HudPrintChannel::Notify,
                    text: msg,
                    duration: 3.0,
                });
            } else {
                buy_menu.category = crate::combat::BuyMenuCategory::Main;
                play_sound("common/wpn_hudon.wav");
            }
        }
    }

    if buy_menu.is_open() {
        if keyboard.just_pressed(KeyCode::Escape) {
            buy_menu.close();
            play_sound("common/wpn_hudoff.wav");
        } else {
            let pressed_digit = if keyboard.just_pressed(KeyCode::Digit0)
                || keyboard.just_pressed(KeyCode::Numpad0)
            {
                Some(0)
            } else if keyboard.just_pressed(KeyCode::Digit1)
                || keyboard.just_pressed(KeyCode::Numpad1)
            {
                Some(1)
            } else if keyboard.just_pressed(KeyCode::Digit2)
                || keyboard.just_pressed(KeyCode::Numpad2)
            {
                Some(2)
            } else if keyboard.just_pressed(KeyCode::Digit3)
                || keyboard.just_pressed(KeyCode::Numpad3)
            {
                Some(3)
            } else if keyboard.just_pressed(KeyCode::Digit4)
                || keyboard.just_pressed(KeyCode::Numpad4)
            {
                Some(4)
            } else if keyboard.just_pressed(KeyCode::Digit5)
                || keyboard.just_pressed(KeyCode::Numpad5)
            {
                Some(5)
            } else if keyboard.just_pressed(KeyCode::Digit6)
                || keyboard.just_pressed(KeyCode::Numpad6)
            {
                Some(6)
            } else if keyboard.just_pressed(KeyCode::Digit7)
                || keyboard.just_pressed(KeyCode::Numpad7)
            {
                Some(7)
            } else if keyboard.just_pressed(KeyCode::Digit8)
                || keyboard.just_pressed(KeyCode::Numpad8)
            {
                Some(8)
            } else if keyboard.just_pressed(KeyCode::Digit9)
                || keyboard.just_pressed(KeyCode::Numpad9)
            {
                Some(9)
            } else {
                None
            };

            if let Some(digit) = pressed_digit {
                let action = buy_menu.handle_key(digit, player.team);
                match action {
                    crate::combat::BuyMenuInputResult::Close => {
                        play_sound("common/wpn_hudoff.wav");
                    }
                    crate::combat::BuyMenuInputResult::Navigated => {
                        play_sound("common/wpn_hudon.wav");
                    }
                    crate::combat::BuyMenuInputResult::Purchase(item) => {
                        match crate::combat::execute_buy_transaction(
                            &mut player,
                            inv_opt.as_deref_mut(),
                            &rules_state.rules,
                            item,
                        ) {
                            Ok((cost, _)) => {
                                let snd = match item {
                                    goldhub_game_cstrike::BuyItem::PrimaryAmmo
                                    | goldhub_game_cstrike::BuyItem::SecondaryAmmo => {
                                        "items/ammopickup1.wav"
                                    }
                                    goldhub_game_cstrike::BuyItem::Kevlar
                                    | goldhub_game_cstrike::BuyItem::KevlarHelmet => {
                                        "items/kevlar.wav"
                                    }
                                    _ => "items/gunpickup2.wav",
                                };
                                play_sound(snd);
                                let (wid, cal, is_def) = match item {
                                    goldhub_game_cstrike::BuyItem::Weapon(w) => {
                                        (Some(w), None, false)
                                    }
                                    goldhub_game_cstrike::BuyItem::Flashbang => {
                                        (Some(crate::combat::WeaponId::Flashbang), None, false)
                                    }
                                    goldhub_game_cstrike::BuyItem::HeGrenade => {
                                        (Some(crate::combat::WeaponId::HeGrenade), None, false)
                                    }
                                    goldhub_game_cstrike::BuyItem::SmokeGrenade => {
                                        (Some(crate::combat::WeaponId::SmokeGrenade), None, false)
                                    }
                                    goldhub_game_cstrike::BuyItem::DefusalKit => (None, None, true),
                                    _ => (None, None, false),
                                };
                                if wid.is_some() || is_def {
                                    pickup_events.send(super::ItemPickupEvent {
                                        weapon_id: wid,
                                        ammo_caliber: cal,
                                        is_defuser: is_def,
                                    });
                                }
                                print_events.send(HudPrintEvent {
                                    channel: HudPrintChannel::Notify,
                                    text: format!("Bought for ${cost}"),
                                    duration: 2.0,
                                });
                                match item {
                                    goldhub_game_cstrike::BuyItem::PrimaryAmmo
                                    | goldhub_game_cstrike::BuyItem::SecondaryAmmo => {}
                                    _ => {
                                        buy_menu.close();
                                    }
                                }
                            }
                            Err(loc_key) => {
                                play_sound("common/wpn_denyselect.wav");
                                let buy_sec = rules_state.rules.buy_time.round() as u32;
                                let msg = loc.tr(loc_key).replace("%s1", &buy_sec.to_string());
                                print_events.send(HudPrintEvent {
                                    channel: HudPrintChannel::Notify,
                                    text: msg,
                                    duration: 3.0,
                                });
                            }
                        }
                    }
                    crate::combat::BuyMenuInputResult::None => {}
                }
            }
        }
    }

    // Sync menu UI visibility and text
    let is_open = buy_menu.is_open();
    for mut vis in &mut root_q {
        let target = if is_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target {
            *vis = target;
        }
    }

    for mut txt in &mut text_q {
        if is_open {
            let rendered = buy_menu.render_text(player.team, player.money);
            if rendered != **txt {
                **txt = rendered;
            }
        } else if !txt.is_empty() {
            txt.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_localization_fallback_and_redirect() {
        let mut res = CsLocalizationResource::default();
        assert_eq!(res.tr("#Bomb_Planted"), "The bomb has been planted!");
        res.table.load_vdf_bytes(
            b"\"Cstrike_TitlesTXT_Bomb_Planted\"\t\"Bomb is planted!\"",
            false,
        );
        assert_eq!(res.tr("#Bomb_Planted"), "Bomb is planted!");
    }

    #[test]
    fn test_qfont_blitter_ignores_empty_and_non_ascii() {
        let fonts = HudFontResources::default();
        assert!(fonts.render_qfont_ascii_rgba("", [255, 180, 45]).is_none());
        assert!(
            fonts
                .render_qfont_ascii_rgba("Бомба", [255, 180, 45])
                .is_none()
        );
    }

    #[test]
    fn test_cs_game_rules_bomb_defusal_and_round_reset_fsm() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<BombMapParams>();
        app.insert_resource(ScenarioMapData {
            has_bomb_target: true,
            has_bomb_zone: true,
            ..Default::default()
        });
        app.init_resource::<CsGameRulesState>();
        app.init_resource::<GameSearchPaths>();
        app.init_resource::<GoldSrcAudioBank>();
        app.init_resource::<Assets<AudioSource>>();
        app.add_event::<HudPrintEvent>();
        app.add_event::<RoundResetEvent>();
        app.add_systems(Update, update_cs_game_rules_system);

        app.world_mut().spawn(PlayerController {
            team: CsTeam::CounterTerrorist,
            money: 800,
            ..Default::default()
        });

        // Tick 1 syncs map flags without terminating live round
        app.update();
        assert_eq!(
            app.world().resource::<CsGameRulesState>().rules.state,
            RoundState::RoundLive
        );

        // Defuse bomb -> immediate RoundEnd (CTs +3250, ct_score = 1, banner emitted)
        app.world_mut()
            .resource_mut::<ScenarioMapData>()
            .bomb_defused = true;
        app.update();
        let rules = &app.world().resource::<CsGameRulesState>().rules;
        assert_eq!(rules.state, RoundState::RoundEnd);
        assert_eq!(rules.ct_score, 1);

        let ct_money = app
            .world_mut()
            .query::<&PlayerController>()
            .single(app.world())
            .money;
        assert_eq!(ct_money, 4050);

        // Advance `round_timer` past 5.0s intermission -> transitions back to live round + emits RoundResetEvent
        app.world_mut()
            .resource_mut::<ScenarioMapData>()
            .bomb_defused = false;
        app.world_mut()
            .resource_mut::<CsGameRulesState>()
            .rules
            .round_timer = 0.0001;
        std::thread::sleep(std::time::Duration::from_millis(2));
        app.update();
        assert_eq!(
            app.world().resource::<CsGameRulesState>().rules.state,
            RoundState::RoundLive
        );
        let reset_count = app.world().resource::<Events<RoundResetEvent>>().len();
        assert_eq!(reset_count, 1);
    }
}
