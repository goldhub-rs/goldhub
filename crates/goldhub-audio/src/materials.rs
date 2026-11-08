//! Canonical GoldSrc/CS 1.6 `materials.txt` texture material parser and mapping.
//!
//! Maps texture prefix/names to sound categories (Concrete, Metal, Dirt, Vent, Grate, Tile, Slosh, Wood, etc.).
//! Reimplemented with binary search matching canonical `PM_FindTextureType` / `PM_CatagorizeTextureType`.

use crate::error::AudioError;

/// Maximum number of texture materials cached in GoldSrc canonical engine.
pub const MAX_TEXTURES: usize = 1024;
/// Maximum length of a texture name loaded in `pm_materials.h` (17 chars: 16 chars + null terminator).
pub const MAX_TEXTURENAME_LENGTH: usize = 17;

/// Canonical GoldSrc texture material type byte characters (`pm_materials.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MaterialType {
    /// 'C' - Cinder block / Concrete
    Concrete = b'C',
    /// 'M' - Metal
    Metal = b'M',
    /// 'D' - Dirt / Earth / Sand
    Dirt = b'D',
    /// 'V' - Ventilation duct
    Vent = b'V',
    /// 'G' - Grate
    Grate = b'G',
    /// 'T' - Tile
    Tile = b'T',
    /// 'S' - Slosh (shallow liquid puddles)
    Slosh = b'S',
    /// 'W' - Wood
    Wood = b'W',
    /// 'P' - Computer console
    Computer = b'P',
    /// 'X' - Grass
    Grass = b'X',
    /// 'Y' - Glass
    Glass = b'Y',
    /// 'F' - Flesh
    Flesh = b'F',
    /// 'N' - Snow
    Snow = b'N',
}

impl MaterialType {
    /// Attempts to convert an ASCII byte (case-insensitive) to a `MaterialType`.
    #[must_use]
    pub fn from_ascii_byte(b: u8) -> Option<Self> {
        match b.to_ascii_uppercase() {
            b'C' => Some(Self::Concrete),
            b'M' => Some(Self::Metal),
            b'D' => Some(Self::Dirt),
            b'V' => Some(Self::Vent),
            b'G' => Some(Self::Grate),
            b'T' => Some(Self::Tile),
            b'S' => Some(Self::Slosh),
            b'W' => Some(Self::Wood),
            b'P' => Some(Self::Computer),
            b'X' => Some(Self::Grass),
            b'Y' => Some(Self::Glass),
            b'F' => Some(Self::Flesh),
            b'N' => Some(Self::Snow),
            _ => None,
        }
    }

    /// Converts to the standard GoldSrc ASCII character representation.
    #[must_use]
    pub const fn to_ascii_byte(self) -> u8 {
        self as u8
    }

    /// Converts to the corresponding GoldSrc `StepType` for footstep sound selection.
    ///
    /// Matches canonical `PM_MapTextureTypeStepType(char chTextureType)`:
    /// - Wood, Glass, Flesh, Grass, Computer fall back to Concrete footstep sounds.
    #[must_use]
    pub const fn to_step_type(self) -> StepType {
        match self {
            Self::Concrete
            | Self::Wood
            | Self::Computer
            | Self::Grass
            | Self::Glass
            | Self::Flesh => StepType::Concrete,
            Self::Metal => StepType::Metal,
            Self::Dirt => StepType::Dirt,
            Self::Vent => StepType::Vent,
            Self::Grate => StepType::Grate,
            Self::Tile => StepType::Tile,
            Self::Slosh => StepType::Slosh,
            Self::Snow => StepType::Snow,
        }
    }
}

/// GoldSrc footstep category index (`STEP_CONCRETE` .. `STEP_SNOW`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StepType {
    /// 0 - Concrete
    Concrete = 0,
    /// 1 - Metal
    Metal = 1,
    /// 2 - Dirt
    Dirt = 2,
    /// 3 - Ventilation duct
    Vent = 3,
    /// 4 - Metal Grate
    Grate = 4,
    /// 5 - Ceramic Tile
    Tile = 5,
    /// 6 - Slosh (shallow liquid)
    Slosh = 6,
    /// 7 - Wade (waist-deep water)
    Wade = 7,
    /// 8 - Ladder
    Ladder = 8,
    /// 9 - Snow
    Snow = 9,
}

impl StepType {
    /// Returns integer discriminant matching HLSDK `#define STEP_*`.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Strips leading texture prefixes ('-', '+', '{', '!', '~', ' ') identical to canonical `PM_CatagorizeTextureType`.
#[must_use]
pub fn sanitize_texture_name(mut name: &str) -> &str {
    // Strip leading animated sequence prefix '-0', '+0', '+A', etc. (2 bytes)
    if (name.starts_with('-') || name.starts_with('+')) && name.len() >= 2 {
        name = &name[2..];
    }

    // Strip leading special type character '{', '!', '~', ' '
    if name.starts_with('{')
        || name.starts_with('!')
        || name.starts_with('~')
        || name.starts_with(' ')
    {
        name = &name[1..];
    }

    name
}

/// A parsed entry from `materials.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialEntry {
    /// Texture name (truncated up to 16 ASCII bytes as in HLSDK `MAX_TEXTURENAME_LENGHT - 1`).
    pub name: String,
    /// Material classification.
    pub material: MaterialType,
}

/// In-memory table of texture materials parsed from `sound/materials.txt`.
#[derive(Debug, Clone, Default)]
pub struct MaterialTable {
    entries: Vec<MaterialEntry>,
}

impl MaterialTable {
    /// Creates an empty materials table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Parses a `sound/materials.txt` string and builds a sorted lookup table.
    ///
    /// Matches canonical parsing loop in `PM_InitTextureTypes`:
    /// - Skips whitespace and lines starting with `/` or non-alphabetic character.
    /// - First non-whitespace char is material type (e.g. `C`, `M`, `W`, etc.).
    /// - Next non-whitespace token is texture name (up to 16 characters).
    /// - Sorts entries with case-insensitive lexicographical order (`Q_stricmp`).
    pub fn parse_materials_txt(content: &str) -> Result<Self, AudioError> {
        let mut entries = Vec::new();

        for (line_idx, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }

            // Skip comment or non-alpha first char
            let first_byte = trimmed.as_bytes()[0];
            if first_byte == b'/' || !first_byte.is_ascii_alphabetic() {
                continue;
            }

            let material = MaterialType::from_ascii_byte(first_byte).ok_or_else(|| {
                AudioError::InvalidMaterialEntry {
                    line_number: line_idx + 1,
                    reason: format!("unknown material type character '{}'", first_byte as char),
                }
            })?;

            // Remaining after material char
            let rest = trimmed[1..].trim_start();
            if rest.is_empty() {
                continue;
            }

            // Extract texture name token (up to next whitespace)
            let tex_name = rest.split_whitespace().next().unwrap_or("").trim();

            if tex_name.is_empty() {
                continue;
            }

            // Truncate up to MAX_TEXTURENAME_LENGTH - 1 (16 chars)
            let truncated = if tex_name.len() > MAX_TEXTURENAME_LENGTH - 1 {
                &tex_name[..MAX_TEXTURENAME_LENGTH - 1]
            } else {
                tex_name
            };

            entries.push(MaterialEntry {
                name: truncated.to_ascii_uppercase(),
                material,
            });

            if entries.len() >= MAX_TEXTURES {
                break;
            }
        }

        // Sort case-insensitively to allow canonical binary search (PM_SortTextures)
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self { entries })
    }

    /// Looks up the material type for a texture name using binary search.
    ///
    /// Emulates canonical `PM_FindTextureType`:
    /// Returns `MaterialType::Concrete` if texture is not found.
    #[must_use]
    pub fn find_texture_type(&self, raw_name: &str) -> MaterialType {
        let clean = sanitize_texture_name(raw_name);
        let search_name = if clean.len() > MAX_TEXTURENAME_LENGTH - 1 {
            &clean[..MAX_TEXTURENAME_LENGTH - 1]
        } else {
            clean
        };
        let search_upper = search_name.to_ascii_uppercase();

        if let Ok(idx) = self
            .entries
            .binary_search_by(|entry| entry.name.as_str().cmp(&search_upper))
        {
            self.entries[idx].material
        } else {
            // Prefix search fallback: canonical engine uses Q_strnicmp(name, pm_grgszTextureName[pivot], 16)
            // If direct match failed, see if any entry prefix matches
            for entry in &self.entries {
                if search_upper.starts_with(&entry.name) || entry.name.starts_with(&search_upper) {
                    return entry.material;
                }
            }
            MaterialType::Concrete
        }
    }

    /// Total loaded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no textures are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
