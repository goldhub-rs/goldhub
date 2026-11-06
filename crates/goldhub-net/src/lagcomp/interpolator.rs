//! Continuous sub-tick hitbox interpolation and raycast intersection.

use super::history_ring::EntitySnapshot;

/// Performs continuous interpolation between two entity snapshots for exact sub-tick registration.
pub fn interpolate_subtick_snapshot(
    s0: &EntitySnapshot,
    s1: &EntitySnapshot,
    target_micros: u64,
) -> EntitySnapshot {
    if s0.timestamp_micros == s1.timestamp_micros || target_micros <= s0.timestamp_micros {
        return *s0;
    }
    if target_micros >= s1.timestamp_micros {
        return *s1;
    }

    let delta_total = (s1.timestamp_micros - s0.timestamp_micros) as f32;
    let delta_target = (target_micros - s0.timestamp_micros) as f32;
    let t = (delta_target / delta_total).clamp(0.0, 1.0);

    let origin = [
        s0.origin[0] + (s1.origin[0] - s0.origin[0]) * t,
        s0.origin[1] + (s1.origin[1] - s0.origin[1]) * t,
        s0.origin[2] + (s1.origin[2] - s0.origin[2]) * t,
    ];

    EntitySnapshot {
        entity_id: s0.entity_id,
        timestamp_micros: target_micros,
        origin,
        angles: s1.angles, // Discrete viewangles
        mins: s0.mins,
        maxs: s0.maxs,
    }
}

/// Tests whether a continuous ray intersects an axis-aligned bounding box (Slab method).
///
/// Returns `Some(hit_distance)` if hit, or `None` if missed.
pub fn ray_intersects_aabb(
    ray_origin: [f32; 3],
    ray_dir: [f32; 3],
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
) -> Option<f32> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;

    for i in 0..3 {
        if ray_dir[i].abs() < 1e-8 {
            // Ray parallel to slab: if origin is outside slab, no intersection
            if ray_origin[i] < aabb_min[i] || ray_origin[i] > aabb_max[i] {
                return None;
            }
        } else {
            let inv_d = 1.0 / ray_dir[i];
            let mut t1 = (aabb_min[i] - ray_origin[i]) * inv_d;
            let mut t2 = (aabb_max[i] - ray_origin[i]) * inv_d;

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }

            tmin = tmin.max(t1);
            tmax = tmax.min(t2);

            if tmin > tmax || tmax < 0.0 {
                return None;
            }
        }
    }

    if tmin >= 0.0 { Some(tmin) } else { Some(tmax) }
}
