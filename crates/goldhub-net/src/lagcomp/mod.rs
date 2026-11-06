//! Sub-tick lag compensation and hitbox history rewind.

pub mod history_ring;
pub mod interpolator;

pub use history_ring::{EntitySnapshot, LagCompHistoryRing, MAX_LAGCOMP_ENTITIES};
pub use interpolator::{interpolate_subtick_snapshot, ray_intersects_aabb};
