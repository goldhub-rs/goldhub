//! Tournament CS 1.6 Player Movement Engine (ReGameDLL standard).
//!
//! Implements deterministic 128 Hz player physics with full preservation of
//! Bunnyhop, Air Strafing, SGS, Double Duck, and step-climbing.

#![allow(clippy::float_cmp)]

use crate::constants::{
    BUNNYJUMP_MAX_SPEED_FACTOR, HULL_STANDING, IN_DUCK, IN_JUMP, IN_SPEED,
    JUMP_IMPULSE, STOP_EPSILON, TIME_TO_DUCK, VEC_VIEW_DUCKING, VEC_VIEW_STANDING,
};
use crate::constants::MoveVars;
use crate::player::{PlayerCmd, PlayerState};
use crate::trace::WorldTrace;

#[inline]
pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub(crate) fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[inline]
pub(crate) fn normalize(v: [f32; 3]) -> ([f32; 3], f32) {
    let len = length(v);
    if len > 0.0 {
        let inv = 1.0 / len;
        ([v[0] * inv, v[1] * inv, v[2] * inv], len)
    } else {
        ([0.0; 3], 0.0)
    }
}

/// Converts GoldSrc player angles [Pitch, Yaw, Roll] into directional unit vectors (Forward, Right, Up).
#[must_use]
pub fn angle_vectors(angles: [f32; 3]) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let to_rad = std::f32::consts::PI / 180.0;
    let sy = (angles[1] * to_rad).sin();
    let cy = (angles[1] * to_rad).cos();
    let sp = (angles[0] * to_rad).sin();
    let cp = (angles[0] * to_rad).cos();
    let sr = (angles[2] * to_rad).sin();
    let cr = (angles[2] * to_rad).cos();

    let forward = [cp * cy, cp * sy, -sp];
    let right = [
        -sr * sp * cy + cr * sy,
        -sr * sp * sy - cr * cy,
        -sr * cp,
    ];
    let up = [
        cr * sp * cy + sr * sy,
        cr * sp * sy - sr * cy,
        cr * cp,
    ];

    (forward, right, up)
}

/// Reflects incoming velocity along a collision surface plane normal (`PM_ClipVelocity`).
#[must_use]
pub fn pm_clip_velocity(in_vel: [f32; 3], normal: [f32; 3], overbounce: f32) -> ([f32; 3], u32) {
    let mut blocked = 0u32;
    if normal[2] > 0.0 {
        blocked |= 0x01; // floor
    }
    if normal[2].abs() < 1e-4 {
        blocked |= 0x02; // step or wall
    }

    let backoff = dot(in_vel, normal) * overbounce;
    let mut out = [0.0; 3];

    for i in 0..3 {
        let change = in_vel[i] - normal[i] * backoff;
        if change > -STOP_EPSILON && change < STOP_EPSILON {
            out[i] = 0.0;
        } else {
            out[i] = change;
        }
    }

    (out, blocked)
}

/// Applies surface friction to player velocity when touching the ground (`PM_Friction`).
pub fn pm_friction(state: &mut PlayerState, vars: &MoveVars, dt: f32) {
    let speed = length(state.velocity);
    if speed < 0.1 {
        return;
    }

    let control = if speed < vars.stopspeed {
        vars.stopspeed
    } else {
        speed
    };

    let drop = vars.friction * control * dt;
    let mut newspeed = speed - drop;
    if newspeed < 0.0 {
        newspeed = 0.0;
    }

    let scale = newspeed / speed;
    state.velocity[0] *= scale;
    state.velocity[1] *= scale;
    state.velocity[2] *= scale;
}

/// Canonical GoldSrc ground acceleration (`PM_Accelerate`).
pub fn pm_accelerate(
    velocity: &mut [f32; 3],
    wishdir: [f32; 3],
    wishspeed: f32,
    accel: f32,
    dt: f32,
) {
    let currentspeed = dot(*velocity, wishdir);
    let addspeed = wishspeed - currentspeed;

    if addspeed <= 0.0 {
        return;
    }

    let mut accelspeed = accel * dt * wishspeed;
    if accelspeed > addspeed {
        accelspeed = addspeed;
    }

    velocity[0] += accelspeed * wishdir[0];
    velocity[1] += accelspeed * wishdir[1];
    velocity[2] += accelspeed * wishdir[2];
}

/// Authentic John Carmack / GoldSrc air strafe acceleration (`PM_AirAccelerate`).
///
/// Limits wishspeed to 30.0 units/sec, generating orthogonal acceleration that
/// powers Bunnyhop and Air Strafing.
pub fn pm_air_accelerate(
    velocity: &mut [f32; 3],
    wishdir: [f32; 3],
    wishspeed: f32,
    airaccel: f32,
    dt: f32,
) {
    let wishspd = wishspeed.min(30.0);
    let currentspeed = dot(*velocity, wishdir);
    let addspeed = wishspd - currentspeed;

    if addspeed <= 0.0 {
        return;
    }

    let mut accelspeed = airaccel * wishspeed * dt;
    if accelspeed > addspeed {
        accelspeed = addspeed;
    }

    velocity[0] += accelspeed * wishdir[0];
    velocity[1] += accelspeed * wishdir[1];
    velocity[2] += accelspeed * wishdir[2];
}

/// Limits excessive velocity accumulation on landing/bunnyhopping (`PM_PreventMegaBunnyJumping`).
pub fn pm_prevent_mega_bunny_jumping(velocity: &mut [f32; 3], maxspeed: f32) {
    let maxscaledspeed = BUNNYJUMP_MAX_SPEED_FACTOR * maxspeed;
    if maxscaledspeed <= 0.0 {
        return;
    }

    let spd = length(*velocity);
    if spd > maxscaledspeed {
        let fraction = (maxscaledspeed / spd) * 0.8;
        velocity[0] *= fraction;
        velocity[1] *= fraction;
        velocity[2] *= fraction;
    }
}

/// Probes 2 units downward to categorize whether player is on a walkable floor (`PM_CategorizePosition`).
pub fn pm_categorize_position(state: &mut PlayerState, world: &impl WorldTrace) {
    // If shooting upward rapidly, definitely not on ground
    if state.velocity[2] > 180.0 {
        state.on_ground = false;
        return;
    }

    let point = [state.origin[0], state.origin[1], state.origin[2] - 2.0];
    let tr = world.trace_player_hull(state.origin, point, state.active_hull());

    if tr.fraction == 1.0 || tr.plane_normal[2] < 0.7 {
        // In the air or floor slope too steep (>45 degrees)
        state.on_ground = false;
    } else {
        state.on_ground = true;
        if !tr.startsolid && !tr.allsolid {
            state.origin = tr.endpos;
        }
    }
}

/// Multi-plane sliding movement over a single physical tick (`PM_FlyMove`).
pub fn pm_fly_move(
    origin: &mut [f32; 3],
    velocity: &mut [f32; 3],
    hull_idx: u32,
    dt: f32,
    world: &impl WorldTrace,
) -> u32 {
    const MAX_CLIP_PLANES: usize = 5;
    let mut planes = [[0.0f32; 3]; MAX_CLIP_PLANES];
    let mut numplanes = 0;
    let mut original_velocity = *velocity;
    let primal_velocity = *velocity;
    let mut time_left = dt;
    let mut blocked = 0u32;

    for _bump in 0..4 {
        if velocity[0] == 0.0 && velocity[1] == 0.0 && velocity[2] == 0.0 {
            break;
        }

        let end = [
            origin[0] + velocity[0] * time_left,
            origin[1] + velocity[1] * time_left,
            origin[2] + velocity[2] * time_left,
        ];

        let tr = world.trace_player_hull(*origin, end, hull_idx);

        if tr.allsolid {
            *velocity = [0.0; 3];
            return 4;
        }

        if tr.fraction > 0.0 {
            *origin = tr.endpos;
            original_velocity = *velocity;
            numplanes = 0;
        }

        if tr.fraction == 1.0 {
            break;
        }

        if tr.plane_normal[2] > 0.7 {
            blocked |= 0x01; // floor
        }
        if tr.plane_normal[2].abs() < 1e-4 {
            blocked |= 0x02; // wall
        }

        time_left -= time_left * tr.fraction;

        if numplanes >= MAX_CLIP_PLANES {
            *velocity = [0.0; 3];
            break;
        }

        planes[numplanes] = tr.plane_normal;
        numplanes += 1;

        let mut i = 0;
        while i < numplanes {
            let (clipped, _) = pm_clip_velocity(original_velocity, planes[i], 1.0);
            *velocity = clipped;

            let mut j = 0;
            while j < numplanes {
                if j != i && dot(*velocity, planes[j]) < 0.0 {
                    break;
                }
                j += 1;
            }

            if j == numplanes {
                break;
            }
            i += 1;
        }

        if i == numplanes {
            if numplanes != 2 {
                *velocity = [0.0; 3];
                break;
            }

            let dir = cross(planes[0], planes[1]);
            let d = dot(dir, *velocity);
            velocity[0] = dir[0] * d;
            velocity[1] = dir[1] * d;
            velocity[2] = dir[2] * d;
        }

        if dot(*velocity, primal_velocity) <= 0.0 {
            *velocity = [0.0; 3];
            break;
        }
    }

    blocked
}

/// Executes ground walking and automatic step-up / step-down climbing (`PM_WalkMove`).
pub fn pm_walk_move(
    state: &mut PlayerState,
    cmd: &PlayerCmd,
    vars: &MoveVars,
    forward: [f32; 3],
    right: [f32; 3],
    dt: f32,
    world: &impl WorldTrace,
) {
    // Jump stamina penalty (fuser2) slows horizontal speed
    if state.jump_penalty_ms > 0.0 {
        let ratio = (100.0 - state.jump_penalty_ms * 0.001 * 19.0) * 0.01;
        state.velocity[0] *= ratio;
        state.velocity[1] *= ratio;
    }

    // Zero out Z components of forward and right vectors for ground movement
    let (f, _) = normalize([forward[0], forward[1], 0.0]);
    let (r, _) = normalize([right[0], right[1], 0.0]);

    let wishvel = [
        f[0] * cmd.forward_move + r[0] * cmd.side_move,
        f[1] * cmd.forward_move + r[1] * cmd.side_move,
        0.0,
    ];

    let (wishdir, mut wishspeed) = normalize(wishvel);

    let mut max_speed = vars.maxspeed;
    if state.is_ducking {
        max_speed *= vars.ducking_speed_multiplier;
    } else if (cmd.buttons & IN_SPEED) != 0 {
        max_speed *= vars.walking_speed_multiplier;
    }

    if wishspeed > max_speed {
        wishspeed = max_speed;
    }

    state.velocity[2] = 0.0;
    pm_accelerate(&mut state.velocity, wishdir, wishspeed, vars.accelerate, dt);
    state.velocity[2] = 0.0;

    let spd = length(state.velocity);
    if spd < 1.0 {
        state.velocity = [0.0; 3];
        return;
    }

    let original = state.origin;
    let original_vel = state.velocity;

    // First try moving directly to destination horizontally
    let dest = [
        state.origin[0] + state.velocity[0] * dt,
        state.origin[1] + state.velocity[1] * dt,
        state.origin[2],
    ];

    let trace = world.trace_player_hull(state.origin, dest, state.active_hull());
    if trace.fraction == 1.0 {
        state.origin = trace.endpos;
        return;
    }

    // Direct move hit an obstacle. Try step slide move:
    // Option A: Slide on the ground
    let mut down = original;
    let mut down_vel = original_vel;
    pm_fly_move(&mut down, &mut down_vel, state.active_hull(), dt, world);

    // Option B: Step up by stepsize, slide forward, then step down
    state.origin = original;
    state.velocity = original_vel;

    let step_dest = [state.origin[0], state.origin[1], state.origin[2] + vars.stepsize];
    let step_tr = world.trace_player_hull(state.origin, step_dest, state.active_hull());
    if !step_tr.startsolid && !step_tr.allsolid {
        state.origin = step_tr.endpos;
    }

    let active_hull = state.active_hull();
    pm_fly_move(
        &mut state.origin,
        &mut state.velocity,
        active_hull,
        dt,
        world,
    );

    let down_dest = [state.origin[0], state.origin[1], state.origin[2] - vars.stepsize];
    let down_tr = world.trace_player_hull(state.origin, down_dest, state.active_hull());

    let mut use_down = false;
    if down_tr.fraction == 1.0 || down_tr.plane_normal[2] < 0.7 {
        use_down = true;
    } else if !down_tr.startsolid && !down_tr.allsolid {
        state.origin = down_tr.endpos;
    }

    // Compare distance moved: take whichever path went farther horizontally
    let down_dist = (down[0] - original[0]).powi(2) + (down[1] - original[1]).powi(2);
    let up_dist = (state.origin[0] - original[0]).powi(2) + (state.origin[1] - original[1]).powi(2);

    if use_down || down_dist > up_dist {
        state.origin = down;
        state.velocity = down_vel;
    }
}

/// Executes mid-air physics and strafing (`PM_AirMove`).
pub fn pm_air_move(
    state: &mut PlayerState,
    cmd: &PlayerCmd,
    vars: &MoveVars,
    forward: [f32; 3],
    right: [f32; 3],
    dt: f32,
    world: &impl WorldTrace,
) {
    let (f, _) = normalize([forward[0], forward[1], 0.0]);
    let (r, _) = normalize([right[0], right[1], 0.0]);

    let wishvel = [
        f[0] * cmd.forward_move + r[0] * cmd.side_move,
        f[1] * cmd.forward_move + r[1] * cmd.side_move,
        0.0,
    ];

    let (wishdir, mut wishspeed) = normalize(wishvel);
    if wishspeed > vars.maxspeed {
        wishspeed = vars.maxspeed;
    }

    pm_air_accelerate(
        &mut state.velocity,
        wishdir,
        wishspeed,
        vars.airaccelerate,
        dt,
    );

    let active_hull = state.active_hull();
    pm_fly_move(
        &mut state.origin,
        &mut state.velocity,
        active_hull,
        dt,
        world,
    );
}

/// Handles jump impulse initiation with Bunnyhop ground-window preservation (`PM_Jump`).
pub fn pm_jump(state: &mut PlayerState, _cmd: &PlayerCmd, vars: &MoveVars) {
    if state.dead {
        state.old_buttons |= IN_JUMP;
        return;
    }

    if !state.on_ground {
        state.old_buttons |= IN_JUMP;
        return;
    }

    // Prevent pogo-sticking: require jump release between jumps unless bunnyhopping
    if (state.old_buttons & IN_JUMP) != 0 {
        return;
    }

    // Leave the ground immediately
    state.on_ground = false;

    // Apply tournament bunnyhop speed cap
    pm_prevent_mega_bunny_jumping(&mut state.velocity, vars.maxspeed);

    // Apply canonical jump impulse
    let mut jump_z = JUMP_IMPULSE;
    if state.jump_penalty_ms > 0.0 {
        let ratio = (100.0 - state.jump_penalty_ms * 0.001 * 19.0) * 0.01;
        jump_z *= ratio;
    }
    state.velocity[2] = jump_z;

    // Add stamina cooldown
    state.jump_penalty_ms = 1315.7895;
    state.old_buttons |= IN_JUMP;
}

/// Evaluates Hermite cubic ease-in/ease-out spline interpolation (`PM_SplineFraction`).
#[inline]
#[must_use]
pub fn pm_spline_fraction(value: f32, scale: f32) -> f32 {
    let v = (value * scale).clamp(0.0, 1.0);
    let v_sq = v * v;
    3.0 * v_sq - 2.0 * v_sq * v
}

/// Manages ducking transitions and instant Double Duck / Duck Tap mechanics (`PM_Duck`).
pub fn pm_duck(
    state: &mut PlayerState,
    cmd: &PlayerCmd,
    _vars: &MoveVars,
    _dt: f32,
    world: &impl WorldTrace,
) {
    let buttons_changed = state.old_buttons ^ cmd.buttons;
    let button_pressed = buttons_changed & cmd.buttons;

    if (cmd.buttons & IN_DUCK) != 0 {
        state.old_buttons |= IN_DUCK;
    } else {
        state.old_buttons &= !IN_DUCK;
    }

    if state.dead || ((cmd.buttons & IN_DUCK) == 0 && !state.in_duck && !state.is_ducking) {
        return;
    }

    if (cmd.buttons & IN_DUCK) != 0 {
        if (button_pressed & IN_DUCK) != 0 && !state.is_ducking {
            state.duck_time_ms = 1000.0;
            state.in_duck = true;
        }

        if state.in_duck {
            // Duck transition finished or performed in air
            if (state.duck_time_ms / 1000.0) <= (1.0 - TIME_TO_DUCK) || !state.on_ground {
                state.is_ducking = true;
                state.in_duck = false;
                state.view_height = VEC_VIEW_DUCKING;

                // Adjust origin for duck hull difference if on ground
                if state.on_ground {
                    state.origin[2] -= 18.0;
                    pm_categorize_position(state, world);
                }
            } else {
                const F_MORE: f32 = 18.0;
                // Smooth cubic spline duck transition matching ReGameDLL:
                // View height interpolates from VEC_VIEW_STANDING (17.0) down to (VEC_VIEW_DUCKING - 18.0) = -6.0
                // When duck finishes and origin[2] drops by 18 units, net camera delta is exactly zero!
                let time = 1.0 - state.duck_time_ms / 1000.0;
                let fraction = if time >= 0.0 {
                    pm_spline_fraction(time, 1.0 / TIME_TO_DUCK)
                } else {
                    0.0
                };
                state.view_height =
                    (VEC_VIEW_DUCKING - F_MORE) * fraction + VEC_VIEW_STANDING * (1.0 - fraction);
            }
        }
    } else {
        pm_unduck(state, world);
    }
}

/// Attempts to stand up from a crouch (`PM_UnDuck`).
pub fn pm_unduck(state: &mut PlayerState, world: &impl WorldTrace) {
    if state.in_duck || !state.is_ducking {
        state.is_ducking = false;
        state.in_duck = false;
        state.view_height = VEC_VIEW_STANDING;
        state.duck_time_ms = 0.0;
        return;
    }

    let mut new_origin = state.origin;
    if state.on_ground {
        new_origin[2] += 18.0;
    }

    let trace = world.trace_player_hull(new_origin, new_origin, HULL_STANDING);
    if !trace.startsolid {
        state.is_ducking = false;
        state.in_duck = false;
        state.view_height = VEC_VIEW_STANDING;
        state.duck_time_ms = 0.0;
        state.origin = new_origin;
        pm_categorize_position(state, world);
    }
}

/// Master single-tick player physics update (128 Hz sub-tick step, ReGameDLL standard).
pub fn pm_player_move(
    state: &mut PlayerState,
    cmd: &PlayerCmd,
    vars: &MoveVars,
    dt: f32,
    world: &impl WorldTrace,
) {
    if state.dead {
        return;
    }

    // 1. Decrement timers
    let dt_ms = dt * 1000.0;
    if state.duck_time_ms > 0.0 {
        state.duck_time_ms = (state.duck_time_ms - dt_ms).max(0.0);
    }
    if state.jump_penalty_ms > 0.0 {
        state.jump_penalty_ms = (state.jump_penalty_ms - dt_ms).max(0.0);
    }
    if state.step_sound_time_ms > 0.0 {
        state.step_sound_time_ms = (state.step_sound_time_ms - dt_ms).max(0.0);
    }

    // 2. View direction vectors
    state.angles = cmd.view_angles;
    let (forward, right, _up) = angle_vectors(state.angles);

    // 3. Ground detection
    pm_categorize_position(state, world);

    if !state.on_ground {
        state.fall_velocity = -state.velocity[2];
    }

    // 4. Ducking logic
    pm_duck(state, cmd, vars, dt, world);

    // 5. Half-step Velocity Verlet gravity
    state.velocity[2] -= vars.gravity * 0.5 * dt;

    // 6. Jump input
    if (cmd.buttons & IN_JUMP) != 0 {
        pm_jump(state, cmd, vars);
    } else {
        state.old_buttons &= !IN_JUMP;
    }

    // 7. Friction & Movement
    if state.on_ground {
        state.velocity[2] = 0.0;
        pm_friction(state, vars, dt);
        pm_walk_move(state, cmd, vars, forward, right, dt, world);
    } else {
        pm_air_move(state, cmd, vars, forward, right, dt, world);
    }

    // 8. Re-categorize position after move
    pm_categorize_position(state, world);

    // 9. Second half-step Velocity Verlet gravity
    state.velocity[2] -= vars.gravity * 0.5 * dt;

    if state.on_ground {
        state.velocity[2] = 0.0;
    }

    // 10. Update buttons
    state.old_buttons = cmd.buttons;
}
