//! GoldSrc legacy format decoders (BSP30, WAD3, Studio MDL).
//!
//! Enabled strictly under the `goldsrc` feature flag.

pub mod bsp;
pub mod mdl;
pub mod tga;
pub mod wad;

pub use bsp::{Bsp30Decoder, BspLevel};
pub use mdl::{MdlHeader, StudioMdlDecoder};
pub use tga::{TgaImage, decode_tga};
pub use wad::{Wad3Decoder, WadArchive};
