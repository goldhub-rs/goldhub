//! Canonical GoldSrc spatial sound attenuation model and distance panning.
//!
//! HLSDK / ReHLDS / Xash3D attenuation curves and stereo panning (`S_SpatializeChannel`).

/// Standard attenuation constant: no attenuation over distance (`ATTN_NONE = 0.0`).
pub const ATTN_NONE: f32 = 0.0;
/// Standard attenuation constant: normal falloff curve (`ATTN_NORM = 0.8`).
pub const ATTN_NORM: f32 = 0.8;
/// Standard attenuation constant: static sounds falloff (`ATTN_STATIC = 1.25`).
pub const ATTN_STATIC: f32 = 1.25;
/// Standard attenuation constant: ambient/idle sounds falloff (`ATTN_IDLE = 2.0`).
pub const ATTN_IDLE: f32 = 2.0;

/// Default GoldSrc sound clipping distance threshold (1000 units).
pub const SND_CLIP_DISTANCE: f32 = 1000.0;

/// Spatial stereo sound channel volume pair (0..=255 integer range or 0.0..=1.0 float range).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialVolume {
    /// Left channel volume in 0..=255 range.
    pub left_255: u8,
    /// Right channel volume in 0..=255 range.
    pub right_255: u8,
    /// Normalized left channel volume (0.0 to 1.0).
    pub left_norm: f32,
    /// Normalized right channel volume (0.0 to 1.0).
    pub right_norm: f32,
}

/// Computes GoldSrc spatial attenuation and stereo panning identical to canonical `S_SpatializeChannel`.
///
/// Parameters:
/// - `master_vol`: Base volume in 0..=255 range (e.g. `(vol * 255.0).round()`).
/// - `dot`: Dot product between listener's right orientation vector and listener-to-sound unit vector (-1.0 to 1.0).
/// - `dist_scaled`: Effective scaled distance (`distance * (attenuation / SND_CLIP_DISTANCE)`).
///
/// Equations from GoldSrc / Xash3d:
/// ```c
/// scale = (1.0f - dist) * (1.0f + dot);
/// rvol = round(master_vol * scale);
/// scale = (1.0f - dist) * (1.0f - dot);
/// lvol = round(master_vol * scale);
/// *right_vol = (int)(bound(0.0f, rvol, 255.0f));
/// *left_vol = (int)(bound(0.0f, lvol, 255.0f));
/// ```
#[must_use]
pub fn spatialize_channel(master_vol: f32, dot: f32, dist_scaled: f32) -> SpatialVolume {
    let r_scale = (1.0 - dist_scaled) * (1.0 + dot);
    let l_scale = (1.0 - dist_scaled) * (1.0 - dot);

    let r_vol = (master_vol * r_scale).round().clamp(0.0, 255.0);
    let l_vol = (master_vol * l_scale).round().clamp(0.0, 255.0);

    let right_255 = r_vol as u8;
    let left_255 = l_vol as u8;

    SpatialVolume {
        left_255,
        right_255,
        left_norm: f32::from(left_255) / 255.0,
        right_norm: f32::from(right_255) / 255.0,
    }
}

/// Evaluates 3D spatialization for a sound emitted at `sound_origin` perceived by a listener.
///
/// Parameters:
/// - `listener_origin`: Listener 3D coordinates `[x, y, z]`.
/// - `listener_right`: Listener normalized right vector `[rx, ry, rz]`.
/// - `sound_origin`: Sound emitter coordinates `[sx, sy, sz]`.
/// - `volume_norm`: Base playback volume (0.0 to 1.0).
/// - `attenuation`: Attenuation coefficient (`ATTN_NORM`, `ATTN_STATIC`, `ATTN_IDLE`, or `ATTN_NONE`).
#[must_use]
pub fn spatialize_3d(
    listener_origin: [f32; 3],
    listener_right: [f32; 3],
    sound_origin: [f32; 3],
    volume_norm: f32,
    attenuation: f32,
) -> SpatialVolume {
    let dx = sound_origin[0] - listener_origin[0];
    let dy = sound_origin[1] - listener_origin[1];
    let dz = sound_origin[2] - listener_origin[2];

    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
    let master_vol = (volume_norm.clamp(0.0, 1.0) * 255.0).round();

    if dist <= 1e-4 {
        // At origin: centered full volume
        let vol_u8 = master_vol as u8;
        return SpatialVolume {
            left_255: vol_u8,
            right_255: vol_u8,
            left_norm: volume_norm,
            right_norm: volume_norm,
        };
    }

    let inv_dist = 1.0 / dist;
    let dir_x = dx * inv_dist;
    let dir_y = dy * inv_dist;
    let dir_z = dz * inv_dist;

    let mut dot = dir_x * listener_right[0] + dir_y * listener_right[1] + dir_z * listener_right[2];

    let dist_mult = attenuation / SND_CLIP_DISTANCE;
    if dist_mult <= 0.0 {
        // No attenuation: do not pan
        dot = 0.0;
    }

    let dist_scaled = dist * dist_mult;
    spatialize_channel(master_vol, dot, dist_scaled)
}
