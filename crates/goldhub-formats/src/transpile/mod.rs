//! Transpilers converting legacy format representations into modern GPU-ready Canonical IR.

pub mod bsp_transpile;
pub mod collision_transpile;

pub use bsp_transpile::{
    compile_bsp_geometry, compile_bsp_geometry_and_lightmap, compile_bsp_geometry_submeshes,
    compile_bsp_geometry_with_atlas,
};
pub use collision_transpile::compile_bsp_collision;
