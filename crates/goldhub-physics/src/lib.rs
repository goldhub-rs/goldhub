//! Tournament CS 1.6 tournament-grade deterministic player physics engine.
//!
//! Strict ReGameDLL `pm_shared` port featuring a fixed 128 Hz sub-tick simulation pipeline,
//! sub-frame render interpolation, and full mechanical preservation of Bunnyhop, SGS,
//! Air Strafing, and Double Duck.

pub mod constants;
pub mod controller;
pub mod movement;
pub mod player;
pub mod trace;

pub use constants::*;
pub use controller::FixedSubtickSimulator;
pub use movement::{angle_vectors, pm_air_accelerate, pm_clip_velocity, pm_friction, pm_player_move};
pub use player::{PlayerCmd, PlayerState};
pub use trace::{BspWorldCollision, TraceResult, WorldTrace};

#[cfg(test)]
mod tests {
    use super::*;

    struct FlatWorld;

    impl WorldTrace for FlatWorld {
        fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], _hull_idx: u32) -> TraceResult {
            const DIST_EPSILON: f32 = 0.03125;
            // Flat floor at Z = 0 (plane dist = 0, normal = [0, 0, 1])
            if end[2] <= 0.0 {
                let frac = if start[2] > 0.0 {
                    (start[2] / (start[2] - end[2])).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                TraceResult {
                    startsolid: start[2] < 0.0,
                    allsolid: false,
                    fraction: frac,
                    endpos: [
                        start[0] + (end[0] - start[0]) * frac,
                        start[1] + (end[1] - start[1]) * frac,
                        DIST_EPSILON,
                    ],
                    plane_normal: [0.0, 0.0, 1.0],
                    plane_dist: 0.0,
                    hit_entity: None,
                }
            } else {
                TraceResult {
                    startsolid: false,
                    allsolid: false,
                    fraction: 1.0,
                    endpos: end,
                    plane_normal: [0.0, 0.0, 1.0],
                    plane_dist: 0.0,
                    hit_entity: None,
                }
            }
        }
    }

    #[test]
    fn test_air_accelerate_orthogonal_gain() {
        let mut vel = [250.0, 0.0, 0.0];
        let wishdir = [0.0, 1.0, 0.0]; // 90 degree air strafe
        let wishspeed = 250.0;
        let airaccel = 10.0;
        let dt = 1.0 / 128.0;

        pm_air_accelerate(&mut vel, wishdir, wishspeed, airaccel, dt);

        assert_eq!(vel[0], 250.0, "Forward velocity must remain unaffected by pure orthogonal strafe");
        assert!(vel[1] > 0.0, "Lateral velocity must increase");
        let new_speed = vel[0].hypot(vel[1]);
        assert!(new_speed > 250.0, "Total speed must increase during air strafe");
    }

    #[test]
    fn test_ground_friction_decay() {
        let mut state = PlayerState {
            velocity: [200.0, 0.0, 0.0],
            on_ground: true,
            ..Default::default()
        };
        let vars = MoveVars::default();
        let dt = 1.0 / 128.0;

        pm_friction(&mut state, &vars, dt);

        assert!(state.velocity[0] < 200.0, "Ground friction must decelerate player");
        assert!(state.velocity[0] > 0.0, "Single tick of friction should not reverse velocity");
    }

    #[test]
    fn test_jump_impulse_and_air_state() {
        let mut state = PlayerState {
            origin: [0.0, 0.0, 0.0],
            velocity: [0.0, 0.0, 0.0],
            on_ground: true,
            ..Default::default()
        };
        let cmd = PlayerCmd {
            buttons: IN_JUMP,
            ..Default::default()
        };
        let vars = MoveVars::default();

        movement::pm_jump(&mut state, &cmd, &vars);

        assert!(!state.on_ground, "Player must leave ground immediately upon jump");
        assert_eq!(state.velocity[2], JUMP_IMPULSE, "Vertical impulse must match JUMP_IMPULSE");
    }

    #[test]
    fn test_bunnyhop_speed_cap() {
        let mut vel = [450.0, 0.0, 0.0];
        let maxspeed = 320.0;

        movement::pm_prevent_mega_bunny_jumping(&mut vel, maxspeed);

        assert!(vel[0] < 450.0, "Mega bunny jumping must crop excess velocity");
        assert!(vel[0] > 0.0);
    }

    #[test]
    fn test_subtick_simulator_gravity_and_ground_landing() {
        let mut sim = FixedSubtickSimulator::new([0.0, 0.0, 50.0], 0.0);
        let world = FlatWorld;
        let cmd = PlayerCmd::default();

        // Advance 1 second across 60 frames (16.6 ms per frame)
        for _ in 0..60 {
            sim.advance(1.0 / 60.0, &cmd, &world);
        }

        assert!(sim.current.on_ground, "Player must land on flat ground at Z=0");
        assert!(
            (sim.current.origin[2] - 0.03125).abs() < 1e-4,
            "Player Z must rest on floor with DIST_EPSILON clearance, got {}",
            sim.current.origin[2]
        );
        assert_eq!(sim.current.velocity[2], 0.0, "Vertical velocity on ground must be zero");
    }

    #[test]
    fn test_step_climb_obstacle() {
        struct StepWorld;
        impl WorldTrace for StepWorld {
            fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], _hull_idx: u32) -> TraceResult {
                const DIST_EPSILON: f32 = 0.03125;
                // Ground at Z=0 for X < 50, Step of height 12 at Z=12 for X >= 50
                // Vertical wall at X = 50 from Z=0 to Z=12
                if start[0] < 50.0 && end[0] >= 50.0 && end[2] < 12.0 {
                    // Hit step wall at X = 50
                    let frac = ((50.0 - start[0]) / (end[0] - start[0])).clamp(0.0, 1.0);
                    return TraceResult {
                        startsolid: false,
                        allsolid: false,
                        fraction: frac,
                        endpos: [50.0 - DIST_EPSILON, start[1] + (end[1] - start[1]) * frac, start[2]],
                        plane_normal: [-1.0, 0.0, 0.0],
                        plane_dist: -50.0,
                        hit_entity: None,
                    };
                }
                let floor_z = if end[0] >= 50.0 { 12.0 } else { 0.0 };
                if end[2] <= floor_z {
                    let frac = if start[2] > floor_z {
                        ((start[2] - floor_z) / (start[2] - end[2])).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    return TraceResult {
                        startsolid: start[2] < floor_z,
                        allsolid: false,
                        fraction: frac,
                        endpos: [
                            start[0] + (end[0] - start[0]) * frac,
                            start[1] + (end[1] - start[1]) * frac,
                            floor_z + DIST_EPSILON,
                        ],
                        plane_normal: [0.0, 0.0, 1.0],
                        plane_dist: floor_z,
                        hit_entity: None,
                    };
                }
                TraceResult {
                    startsolid: false,
                    allsolid: false,
                    fraction: 1.0,
                    endpos: end,
                    plane_normal: [0.0, 0.0, 1.0],
                    plane_dist: 0.0,
                    hit_entity: None,
                }
            }
        }

        let mut sim = FixedSubtickSimulator::new([40.0, 0.0, 0.03125], 0.0);
        let world = StepWorld;
        let cmd = PlayerCmd {
            forward_move: 250.0,
            ..Default::default()
        };

        // Advance 1 second walking forward into the 12-unit step
        for _ in 0..60 {
            sim.advance(1.0 / 60.0, &cmd, &world);
        }

        assert!(sim.current.origin[0] > 50.0, "Player must step up and cross X=50 step");
        assert!(
            (sim.current.origin[2] - 12.03125).abs() < 0.1,
            "Player must rest on top of the step at Z=12, got {}",
            sim.current.origin[2]
        );
    }
}
