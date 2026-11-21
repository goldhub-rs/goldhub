//! Counter-Strike 1.6 resource string & localization loader (`cstrike_russian.txt`, `cstrike_english.txt`, `titles.txt`).

#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(feature = "std")]
use std::string::{String, ToString};
#[cfg(feature = "std")]
use std::vec::Vec;

/// Built-in static English fallbacks when no game directory localization files are loaded (also `no_std` safe).
#[must_use]
pub const fn builtin_fallback(key: &str) -> Option<&'static str> {
    let clean = match key.as_bytes() {
        [b'#', rest @ ..] => match core::str::from_utf8(rest) {
            Ok(s) => s,
            Err(_) => return None,
        },
        _ => key,
    };

    if str_eq(clean, "Weapon_Cannot_Be_Dropped") {
        Some("This weapon cannot be dropped")
    } else if str_eq(clean, "C4_Plant_At_Bomb_Spot") {
        Some("C4 must be planted at a bomb site!")
    } else if str_eq(clean, "C4_Plant_Must_Be_On_Ground") {
        Some("You must be standing on\nthe ground to plant the C4!")
    } else if str_eq(clean, "C4_Arming_Cancelled") {
        Some("Arming Sequence Canceled.\nC4 can only be placed at a Bomb Target.")
    } else if str_eq(clean, "C4_Activated_At_Bomb_Spot") {
        Some("C4 must be activated at a Bomb Target")
    } else if str_eq(clean, "Bomb_Planted") {
        Some("The bomb has been planted!")
    } else if str_eq(clean, "C4_Defuse_Must_Be_On_Ground") {
        Some("You must be on the ground\nto defuse the bomb!")
    } else if str_eq(clean, "Bomb_Defused") {
        Some("The bomb has been defused!")
    } else if str_eq(clean, "Defusing_Bomb_With_Defuse_Kit") {
        Some("Defusing bomb WITH Defuse kit.")
    } else if str_eq(clean, "Defusing_Bomb_Without_Defuse_Kit") {
        Some("Defusing bomb WITHOUT Defuse kit.")
    } else if str_eq(clean, "Got_bomb") {
        Some("You picked up the bomb!")
    } else if str_eq(clean, "Got_defuser") {
        Some("You picked up a defuser kit!")
    } else if str_eq(clean, "Game_bomb_drop") {
        Some("%s1 dropped the bomb")
    } else if str_eq(clean, "Game_bomb_pickup") {
        Some("%s1 picked up the bomb")
    } else if str_eq(clean, "Switch_To_BurstFire") {
        Some("Switched to Burst-Fire mode")
    } else if str_eq(clean, "Switch_To_SemiAuto") {
        Some("Switched to semi-automatic")
    } else if str_eq(clean, "Switch_To_FullAuto") {
        Some("Switched to automatic")
    } else if str_eq(clean, "Fire_in_the_hole") {
        Some("Fire in the hole!")
    } else if str_eq(clean, "Target_Bombed") {
        Some("Target Successfully Bombed!")
    } else if str_eq(clean, "Target_Saved") {
        Some("Target has been saved!")
    } else if str_eq(clean, "Terrorists_Win") {
        Some("Terrorists Win!")
    } else if str_eq(clean, "CTs_Win") {
        Some("Counter-Terrorists Win!")
    } else if str_eq(clean, "All_Hostages_Rescued") {
        Some("All Hostages have been rescued!")
    } else if str_eq(clean, "Hostages_Not_Rescued") {
        Some("Hostages have not been rescued!")
    } else if str_eq(clean, "VIP_Escaped") {
        Some("The VIP has escaped!")
    } else if str_eq(clean, "VIP_Assassinated") {
        Some("VIP has been assassinated!")
    } else if str_eq(clean, "VIP_Not_Escaped") {
        Some("VIP has not escaped!")
    } else if str_eq(clean, "Round_Draw") {
        Some("Round Draw!")
    } else {
        None
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut i = 0;
    while i < ab.len() {
        if ab[i] != bb[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Decodes raw CS 1.6 resource bytes (supports UTF-16LE BOM used by `cstrike_*.txt`, UTF-8 BOM, and UTF-8/ASCII).
#[cfg(feature = "std")]
#[must_use]
pub fn decode_resource_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let code_units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&code_units);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let code_units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&code_units);
    }
    let trimmed = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(trimmed).into_owned()
}

/// Runtime dictionary mapping CS 1.6 `#Key` identifiers to localized display strings.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Default)]
pub struct CsLocalizationTable {
    entries: HashMap<String, String>,
}

#[cfg(feature = "std")]
impl CsLocalizationTable {
    /// Creates an empty localization table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a Valve KeyValues VDF file (`cstrike_russian.txt` / `cstrike_english.txt`).
    /// Existing keys are preserved unless `overwrite == true`.
    pub fn load_vdf_bytes(&mut self, bytes: &[u8], overwrite: bool) {
        let text = decode_resource_text(bytes);
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('"') || trimmed.starts_with("//") {
                continue;
            }
            if let Some((raw_key, raw_val)) = parse_vdf_kv_line(trimmed) {
                let norm = normalize_key(raw_key);
                let val = unescape_c_escapes(raw_val);
                if overwrite || !self.entries.contains_key(&norm) {
                    self.entries.insert(norm, val);
                }
            }
        }
    }

    /// Parses GoldSrc `titles.txt` (`KeyName\n{\ntext\n}`) as fallback strings.
    pub fn load_titles_txt_bytes(&mut self, bytes: &[u8], overwrite: bool) {
        let text = decode_resource_text(bytes);
        let mut current_key: Option<String> = None;
        let mut in_block = false;
        let mut lines_buf: Vec<String> = Vec::new();

        for raw_line in text.lines() {
            let line = raw_line.trim_end_matches('\r');
            let trimmed = line.trim();
            if !in_block {
                if trimmed.starts_with("//") || trimmed.starts_with('$') || trimmed.is_empty() {
                    continue;
                }
                if trimmed == "{" {
                    if current_key.is_some() {
                        in_block = true;
                        lines_buf.clear();
                    }
                } else {
                    current_key = Some(normalize_key(trimmed));
                }
            } else if trimmed == "}" {
                if let Some(key) = current_key.take() {
                    let body = clean_titles_body(&lines_buf);
                    if !body.is_empty() && (overwrite || !self.entries.contains_key(&key)) {
                        self.entries.insert(key, body);
                    }
                }
                in_block = false;
            } else {
                lines_buf.push(line.to_string());
            }
        }
    }

    /// Resolves a `#Key` or plain key, falling back to `builtin_fallback` and finally the raw key itself.
    #[must_use]
    pub fn resolve<'a>(&'a self, key_or_text: &'a str) -> &'a str {
        let norm = normalize_key(key_or_text);
        if let Some(found) = self.entries.get(&norm) {
            return found.as_str();
        }
        if let Some(fallback) = builtin_fallback(key_or_text) {
            return fallback;
        }
        key_or_text
    }

    /// Resolves a localization key and formats `%s1` / `%s2` placeholders.
    #[must_use]
    pub fn format_1(&self, key: &str, arg1: &str) -> String {
        self.resolve(key).replace("%s1", arg1)
    }

    /// Number of loaded localized entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no localized entries are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(feature = "std")]
fn normalize_key(raw: &str) -> String {
    let s = raw.trim().trim_start_matches('#');
    let stripped = s
        .strip_prefix("Cstrike_TitlesTXT_")
        .or_else(|| s.strip_prefix("cstrike_titlestxt_"))
        .unwrap_or(s);
    stripped.to_ascii_lowercase()
}

#[cfg(feature = "std")]
fn unescape_c_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') | None => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(feature = "std")]
fn parse_vdf_kv_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('"')?;
    let k_end = find_unescaped_quote(rest)?;
    let key = &rest[..k_end];
    let after_key = rest[k_end + 1..].trim_start();
    let v_rest = after_key.strip_prefix('"')?;
    let v_end = find_unescaped_quote(v_rest)?;
    let val = &v_rest[..v_end];
    Some((key, val))
}

#[cfg(feature = "std")]
fn find_unescaped_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(feature = "std")]
fn clean_titles_body(lines: &[String]) -> String {
    let cleaned: Vec<String> = lines
        .iter()
        .map(|l| {
            let mut s = l.as_str();
            for prefix in ["\\y", "\\w", "\\r", "\\d", "\\R"] {
                if s.contains(prefix) {
                    // Strip HL menu formatting codes inline below
                    let _ = prefix;
                }
            }
            s = s.trim();
            s.replace("\\y", "")
                .replace("\\w", "")
                .replace("\\r", "")
                .replace("\\d", "")
                .replace("\\R", " ")
        })
        .collect();
    cleaned.join("\n").trim().to_string()
}
