//! Deterministic player simulation state for client-side prediction.

use super::cmd::{
    IN_BACK, IN_FORWARD, IN_JUMP, IN_LEFT, IN_MOVELEFT, IN_MOVERIGHT, IN_RIGHT, UserCmd,
};

pub const FL_ONGROUND: u32 = 1 << 0;
pub const FL_DUCKING: u32 = 1 << 1;

/// Deterministic player state snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerState {
    pub origin: [f32; 3],
    pub velocity: [f32; 3],
    pub viewangles: [f32; 3],
    pub flags: u32,
    pub last_command_number: u32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            origin: [0.0; 3],
            velocity: [0.0; 3],
            viewangles: [0.0; 3],
            flags: FL_ONGROUND,
            last_command_number: 0,
        }
    }
}

impl PlayerState {
    /// Advances player physics state by one command tick.
    pub fn step_prediction(&mut self, cmd: &UserCmd, dt: f32) {
        self.viewangles = cmd.viewangles;
        self.last_command_number = cmd.command_number;

        let on_ground = (self.flags & FL_ONGROUND) != 0;

        // Apply friction when grounded
        if on_ground {
            let speed = self.velocity[0].hypot(self.velocity[1]);
            if speed > 0.1 {
                const FRICTION: f32 = 4.0;
                let drop = speed * FRICTION * dt;
                let new_speed = (speed - drop).max(0.0);
                self.velocity[0] *= new_speed / speed;
                self.velocity[1] *= new_speed / speed;
            } else {
                self.velocity[0] = 0.0;
                self.velocity[1] = 0.0;
            }
        }

        // Compute wish direction from view angles and command move
        let yaw_rad = cmd.viewangles[1].to_radians();
        let (sin_yaw, cos_yaw) = yaw_rad.sin_cos();

        let mut forward_intent = cmd.forwardmove;
        if (cmd.buttons & IN_FORWARD) != 0 {
            forward_intent += 250.0;
        }
        if (cmd.buttons & IN_BACK) != 0 {
            forward_intent -= 250.0;
        }

        let mut side_intent = cmd.sidemove;
        if (cmd.buttons & IN_MOVERIGHT) != 0 || (cmd.buttons & IN_RIGHT) != 0 {
            side_intent += 250.0;
        }
        if (cmd.buttons & IN_MOVELEFT) != 0 || (cmd.buttons & IN_LEFT) != 0 {
            side_intent -= 250.0;
        }

        let wish_dir_x = cos_yaw * forward_intent - sin_yaw * side_intent;
        let wish_dir_y = sin_yaw * forward_intent + cos_yaw * side_intent;

        let wish_speed = wish_dir_x.hypot(wish_dir_y);
        if wish_speed > 0.001 {
            let inv_speed = 1.0 / wish_speed;
            let dir_norm_x = wish_dir_x * inv_speed;
            let dir_norm_y = wish_dir_y * inv_speed;

            let current_speed = self.velocity[0] * dir_norm_x + self.velocity[1] * dir_norm_y;
            let max_accel_speed = wish_speed.min(250.0);
            let add_speed = max_accel_speed - current_speed;

            if add_speed > 0.0 {
                const ACCEL: f32 = 10.0;
                let accel_speed = (ACCEL * dt * wish_speed).min(add_speed);
                self.velocity[0] += accel_speed * dir_norm_x;
                self.velocity[1] += accel_speed * dir_norm_y;
            }
        }

        // Jump impulse
        if on_ground && (cmd.buttons & IN_JUMP) != 0 {
            self.velocity[2] = 268.0; // Standard GoldSrc jump velocity
            self.flags &= !FL_ONGROUND;
        } else if !on_ground {
            // Gravity
            const GRAVITY: f32 = 800.0;
            self.velocity[2] -= GRAVITY * dt;
        }

        // Integrate position
        self.origin[0] += self.velocity[0] * dt;
        self.origin[1] += self.velocity[1] * dt;
        self.origin[2] += self.velocity[2] * dt;

        // Ground plane clamp floor at z = 0.0 for basic prediction test
        if self.origin[2] <= 0.0 {
            self.origin[2] = 0.0;
            self.velocity[2] = 0.0;
            self.flags |= FL_ONGROUND;
        }
    }
}
