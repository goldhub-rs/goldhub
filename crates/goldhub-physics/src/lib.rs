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
pub use movement::{
    HALF_HUMAN_HEIGHT, STUCK_MOVEDOWN, STUCK_MOVEUP, angle_vectors, pm_air_accelerate,
    pm_clip_velocity, pm_duck, pm_fix_player_crouch_stuck, pm_friction, pm_ladder_move,
    pm_player_move, pm_spline_fraction, pm_unduck,
};
pub use player::{PlayerCmd, PlayerState};
pub use trace::{BspWorldCollision, TraceResult, WorldTrace};

#[cfg(test)]
mod tests {
    use super::*;

    struct FlatWorld;

    impl WorldTrace for FlatWorld {
        fn trace_player_hull(&self, start: [f32; 3], end: [f32; 3], hull_idx: u32) -> TraceResult {
            const DIST_EPSILON: f32 = 0.03125;
            let floor_z = if hull_idx == crate::constants::HULL_DUCKING {
                -18.0
            } else {
                0.0
            };
            // Flat floor at floor_z (normal = [0, 0, 1])
            if end[2] <= floor_z {
                let frac = if start[2] > floor_z {
                    ((start[2] - floor_z) / (start[2] - end[2])).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                TraceResult {
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

        fn test_player_position(&self, pos: [f32; 3], hull_idx: u32) -> Option<u32> {
            let floor_z = if hull_idx == crate::constants::HULL_DUCKING {
                -18.0
            } else {
                0.0
            };
            if pos[2] < floor_z { Some(0) } else { None }
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

        assert_eq!(
            vel[0], 250.0,
            "Forward velocity must remain unaffected by pure orthogonal strafe"
        );
        assert!(vel[1] > 0.0, "Lateral velocity must increase");
        let new_speed = vel[0].hypot(vel[1]);
        assert!(
            new_speed > 250.0,
            "Total speed must increase during air strafe"
        );
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

        assert!(
            state.velocity[0] < 200.0,
            "Ground friction must decelerate player"
        );
        assert!(
            state.velocity[0] > 0.0,
            "Single tick of friction should not reverse velocity"
        );
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

        assert!(
            !state.on_ground,
            "Player must leave ground immediately upon jump"
        );
        assert_eq!(
            state.velocity[2], JUMP_IMPULSE,
            "Vertical impulse must match JUMP_IMPULSE"
        );
    }

    #[test]
    fn test_bunnyhop_speed_cap() {
        let mut vel = [450.0, 0.0, 0.0];
        let maxspeed = 320.0;

        movement::pm_prevent_mega_bunny_jumping(&mut vel, maxspeed);

        assert!(
            vel[0] < 450.0,
            "Mega bunny jumping must crop excess velocity"
        );
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

        assert!(
            sim.current.on_ground,
            "Player must land on flat ground at Z=0"
        );
        assert!(
            (sim.current.origin[2] - 0.03125).abs() < 1e-4,
            "Player Z must rest on floor with DIST_EPSILON clearance, got {}",
            sim.current.origin[2]
        );
        assert_eq!(
            sim.current.velocity[2], 0.0,
            "Vertical velocity on ground must be zero"
        );
    }

    #[test]
    fn test_step_climb_obstacle() {
        struct StepWorld;
        impl WorldTrace for StepWorld {
            fn trace_player_hull(
                &self,
                start: [f32; 3],
                end: [f32; 3],
                _hull_idx: u32,
            ) -> TraceResult {
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
                        endpos: [
                            50.0 - DIST_EPSILON,
                            start[1] + (end[1] - start[1]) * frac,
                            start[2],
                        ],
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

            fn test_player_position(&self, pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
                let floor_z = if pos[0] >= 50.0 { 12.0 } else { 0.0 };
                if pos[2] < floor_z { Some(0) } else { None }
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

        assert!(
            sim.current.origin[0] > 50.0,
            "Player must step up and cross X=50 step"
        );
        assert!(
            (sim.current.origin[2] - 12.03125).abs() < 0.1,
            "Player must rest on top of the step at Z=12, got {}",
            sim.current.origin[2]
        );
    }

    #[test]
    fn test_compound_duck_and_shift_speed() {
        let mut sim = FixedSubtickSimulator::new([0.0, 0.0, 1.0], 0.0);
        let world = FlatWorld;

        // 1. Duck + Speed (Shift) both pressed
        let cmd = PlayerCmd {
            forward_move: 250.0,
            buttons: IN_DUCK | IN_SPEED,
            ..Default::default()
        };

        // Advance 1 second
        for _ in 0..60 {
            sim.advance(1.0 / 60.0, &cmd, &world);
        }

        let speed = sim.current.horizontal_speed();
        // Compound duck (0.333) and walk (0.52) speed yields slow creep speed (~43.29 u/s).
        assert!(
            (speed - 43.29).abs() < 1.0,
            "Duck + Shift speed must compound to slow creep speed ~43.29 u/s (got {speed})"
        );

        // 2. Release both Duck and Speed - speed must promptly recover to full running speed!
        let run_cmd = PlayerCmd {
            forward_move: 250.0,
            buttons: 0,
            ..Default::default()
        };

        for _ in 0..60 {
            sim.advance(1.0 / 60.0, &run_cmd, &world);
        }

        assert!(
            !sim.current.is_ducking,
            "Player must not remain ducked after releasing duck"
        );
        assert!(
            !sim.current.in_duck,
            "Player in_duck flag must be cleared after releasing duck"
        );
        let recovered_speed = sim.current.horizontal_speed();
        assert!(
            recovered_speed > 240.0,
            "Speed must recover to normal running speed (>240 u/s), got {recovered_speed}"
        );
    }

    #[test]
    fn test_crouch_under_ceiling_remains_ducked() {
        // Ceiling at Z = 40 (standing hull height 72 exceeds 40, ducking hull height 36 fits under 40)
        struct ArchWorld;
        impl WorldTrace for ArchWorld {
            fn trace_player_hull(
                &self,
                start: [f32; 3],
                end: [f32; 3],
                hull_idx: u32,
            ) -> TraceResult {
                let is_standing = hull_idx == HULL_STANDING;
                if is_standing && end[2] + 36.0 >= 40.0 {
                    return TraceResult {
                        startsolid: start[2] + 36.0 >= 40.0,
                        allsolid: false,
                        fraction: 0.0,
                        endpos: start,
                        plane_normal: [0.0, 0.0, -1.0],
                        plane_dist: 40.0,
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

            fn test_player_position(&self, pos: [f32; 3], hull_idx: u32) -> Option<u32> {
                let is_standing = hull_idx == HULL_STANDING;
                if is_standing && pos[2] + 36.0 >= 40.0 {
                    Some(0)
                } else {
                    None
                }
            }
        }

        let mut state = PlayerState {
            origin: [0.0, 0.0, 0.0],
            is_ducking: true,
            view_height: VEC_VIEW_DUCKING,
            on_ground: true,
            ..Default::default()
        };
        let world = ArchWorld;

        // Try to unduck under arch
        pm_unduck(&mut state, &world);

        assert!(
            state.is_ducking,
            "Player must remain ducked under low ceiling/arch!"
        );
        assert_eq!(state.view_height, VEC_VIEW_DUCKING);
    }

    #[test]
    fn test_ladder_climb_and_idle_and_jump_off() {
        struct TestLadderWorld;

        impl WorldTrace for TestLadderWorld {
            fn trace_player_hull(
                &self,
                _start: [f32; 3],
                end: [f32; 3],
                _hull_idx: u32,
            ) -> TraceResult {
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

            fn test_player_position(&self, _pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
                None
            }

            fn find_ladder(
                &self,
                _origin: [f32; 3],
                _hull_idx: u32,
            ) -> Option<crate::trace::LadderContact> {
                Some(crate::trace::LadderContact {
                    normal: [0.0, 1.0, 0.0], // Ladder facing +Y
                    center: [0.0, 0.0, 100.0],
                    entity_id: 42,
                })
            }
        }

        let world = TestLadderWorld;
        let vars = MoveVars::default();
        let dt = 1.0 / 128.0;

        let mut state = PlayerState {
            origin: [0.0, 0.0, 50.0],
            velocity: [0.0, 0.0, 0.0],
            angles: [0.0, 270.0, 0.0], // Facing toward ladder (-Y)
            ..Default::default()
        };

        // 1. Idle on ladder: player must not fall from gravity, velocity must remain 0
        let idle_cmd = PlayerCmd {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: 0,
            view_angles: [0.0, 270.0, 0.0],
        };
        pm_player_move(&mut state, &idle_cmd, &vars, dt, &world);
        assert!(state.on_ladder, "Player must be flagged on ladder");
        assert_eq!(
            state.velocity,
            [0.0, 0.0, 0.0],
            "Idle on ladder must have zero velocity"
        );
        assert_eq!(
            state.origin[2], 50.0,
            "Player must cling to ladder and not drop from gravity"
        );

        // 2. Press Forward (moving into ladder): climbs UP (+Z)
        let climb_cmd = PlayerCmd {
            forward_move: 200.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_FORWARD,
            view_angles: [0.0, 270.0, 0.0],
        };
        pm_player_move(&mut state, &climb_cmd, &vars, dt, &world);
        assert!(state.on_ladder);
        assert!(
            state.velocity[2] > 150.0,
            "Climbing ladder must produce upward velocity (got {})",
            state.velocity[2]
        );
        assert!(state.origin[2] > 50.0, "Player must ascend on ladder");

        // 3. Press Jump: player leaps off the ladder along ladder normal [0, 1, 0] with 270 u/s
        let jump_cmd = PlayerCmd {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_JUMP,
            view_angles: [0.0, 270.0, 0.0],
        };
        pm_player_move(&mut state, &jump_cmd, &vars, dt, &world);
        assert!(!state.on_ladder, "Jumping must dismount player from ladder");
        assert_eq!(
            state.velocity[1], 270.0,
            "Jump must propel along ladder normal with 270 u/s"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_diagonal_fast_ladder_climb_speed() {
        struct TestLadderWorld;

        impl WorldTrace for TestLadderWorld {
            fn trace_player_hull(
                &self,
                _start: [f32; 3],
                end: [f32; 3],
                _hull_idx: u32,
            ) -> TraceResult {
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

            fn test_player_position(&self, _pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
                None
            }

            fn find_ladder(
                &self,
                _origin: [f32; 3],
                _hull_idx: u32,
            ) -> Option<crate::trace::LadderContact> {
                Some(crate::trace::LadderContact {
                    normal: [0.0, 1.0, 0.0], // Ladder surface facing +Y
                    center: [0.0, 0.0, 100.0],
                    entity_id: 42,
                })
            }
        }

        let world = TestLadderWorld;
        let vars = MoveVars::default();
        let dt = 1.0 / 128.0;

        // 1. Pure 'W' climb: player facing straight into ladder (-Y, yaw = 270 deg)
        let mut state_w = PlayerState {
            origin: [0.0, 0.0, 50.0],
            velocity: [0.0, 0.0, 0.0],
            angles: [0.0, 270.0, 0.0],
            ..Default::default()
        };
        let cmd_w = PlayerCmd {
            forward_move: 200.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_FORWARD,
            view_angles: [0.0, 270.0, 0.0],
        };
        pm_player_move(&mut state_w, &cmd_w, &vars, dt, &world);
        let vertical_speed_w = state_w.velocity[2];
        assert_eq!(
            vertical_speed_w, 200.0,
            "Pure W ladder climb must produce exactly 200 u/s vertical speed"
        );

        // 2. 'AW' diagonal fast climb: yaw 225 deg (45 deg left of -Y), W + A held
        let mut state_aw = PlayerState {
            origin: [0.0, 0.0, 50.0],
            velocity: [0.0, 0.0, 0.0],
            angles: [0.0, 225.0, 0.0],
            ..Default::default()
        };
        let cmd_aw = PlayerCmd {
            forward_move: 200.0,
            side_move: -200.0,
            up_move: 0.0,
            buttons: IN_FORWARD | IN_MOVELEFT,
            view_angles: [0.0, 225.0, 0.0],
        };
        pm_player_move(&mut state_aw, &cmd_aw, &vars, dt, &world);
        let vertical_speed_aw = state_aw.velocity[2];
        assert!(
            vertical_speed_aw > 280.0,
            "AW diagonal climb must produce ~282.84 u/s vertical speed (got {vertical_speed_aw})"
        );
        assert!(
            vertical_speed_aw > vertical_speed_w,
            "AW diagonal climb must be faster than pure W climb"
        );

        // 3. 'WD' diagonal fast climb: yaw 315 deg (45 deg right of -Y), W + D held
        let mut state_wd = PlayerState {
            origin: [0.0, 0.0, 50.0],
            velocity: [0.0, 0.0, 0.0],
            angles: [0.0, 315.0, 0.0],
            ..Default::default()
        };
        let cmd_wd = PlayerCmd {
            forward_move: 200.0,
            side_move: 200.0,
            up_move: 0.0,
            buttons: IN_FORWARD | IN_MOVERIGHT,
            view_angles: [0.0, 315.0, 0.0],
        };
        pm_player_move(&mut state_wd, &cmd_wd, &vars, dt, &world);
        let vertical_speed_wd = state_wd.velocity[2];
        assert!(
            vertical_speed_wd > 280.0,
            "WD diagonal climb must produce ~282.84 u/s vertical speed (got {vertical_speed_wd})"
        );
        assert!(
            vertical_speed_wd > vertical_speed_w,
            "WD diagonal climb must be faster than pure W climb"
        );

        // 4. Crouch climbing on ladder: speed scaled by PLAYER_DUCKING_MULTIPLIER (0.333)
        let mut state_duck = PlayerState {
            origin: [0.0, 0.0, 50.0],
            velocity: [0.0, 0.0, 0.0],
            angles: [0.0, 270.0, 0.0],
            is_ducking: true,
            ..Default::default()
        };
        let cmd_duck = PlayerCmd {
            forward_move: 200.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_FORWARD | IN_DUCK,
            view_angles: [0.0, 270.0, 0.0],
        };
        pm_player_move(&mut state_duck, &cmd_duck, &vars, dt, &world);
        let vertical_speed_duck = state_duck.velocity[2];
        let expected_duck_speed = 200.0 * PLAYER_DUCKING_MULTIPLIER;
        assert!(
            (vertical_speed_duck - expected_duck_speed).abs() < 0.1,
            "Ducked ladder climb must scale speed by 0.333 (expected {expected_duck_speed}, got {vertical_speed_duck})"
        );
    }

    #[test]
    fn test_ladder_jump_impulse_and_detachment() {
        struct TestLadderWorld;

        impl WorldTrace for TestLadderWorld {
            fn trace_player_hull(
                &self,
                _start: [f32; 3],
                end: [f32; 3],
                _hull_idx: u32,
            ) -> TraceResult {
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

            fn test_player_position(&self, _pos: [f32; 3], _hull_idx: u32) -> Option<u32> {
                None
            }

            fn find_ladder(
                &self,
                origin: [f32; 3],
                _hull_idx: u32,
            ) -> Option<crate::trace::LadderContact> {
                // Ladder plane at Y = 0 facing +Y. Detached once origin[1] > 1.0.
                if origin[1] <= 1.0 {
                    Some(crate::trace::LadderContact {
                        normal: [0.0, 1.0, 0.0],
                        center: [0.0, 0.0, 100.0],
                        entity_id: 42,
                    })
                } else {
                    None
                }
            }
        }

        let world = TestLadderWorld;
        let vars = MoveVars::default();
        let dt = 1.0 / 128.0;

        let mut state = PlayerState {
            origin: [0.0, 0.0, 50.0],
            velocity: [0.0, 0.0, 0.0],
            angles: [0.0, 270.0, 0.0],
            on_ladder: true,
            ..Default::default()
        };

        // Press Jump on ladder
        let jump_cmd = PlayerCmd {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_JUMP,
            view_angles: [0.0, 270.0, 0.0],
        };

        pm_player_move(&mut state, &jump_cmd, &vars, dt, &world);

        // Sub-tick verification:
        assert!(
            !state.on_ladder,
            "Player must be immediately detached from ladder upon jumping"
        );
        assert!(!state.on_ground, "Player must be in airborne state");
        assert_eq!(
            state.velocity[1], 270.0,
            "Jump impulse must be exactly normal * 270 along Y axis"
        );
        assert!(
            state.origin[1] > 2.0,
            "Player origin must move away from ladder surface in this tick (got {})",
            state.origin[1]
        );

        // Next tick: jump held or released, player must remain in air and not re-attach
        let next_cmd = PlayerCmd {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_JUMP,
            view_angles: [0.0, 270.0, 0.0],
        };
        pm_player_move(&mut state, &next_cmd, &vars, dt, &world);
        assert!(
            !state.on_ladder,
            "Player must remain disconnected from ladder on subsequent ticks"
        );
        assert!(
            state.origin[1] > 4.0,
            "Player must continue coasting away from ladder"
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_ducking_and_unducking_view_height_continuity() {
        let world = FlatWorld;
        let vars = MoveVars::default();
        let dt = 1.0 / 128.0;

        let mut state = PlayerState {
            origin: [0.0, 0.0, 0.03125],
            velocity: [0.0, 0.0, 0.0],
            view_height: VEC_VIEW_STANDING,
            on_ground: true,
            ..Default::default()
        };

        let duck_cmd = PlayerCmd {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: IN_DUCK,
            view_angles: [0.0, 0.0, 0.0],
        };

        let mut prev_eye_z = state.origin[2] + state.view_height;
        let mut duck_transition_verified = false;

        // 1. Ducking phase: hold IN_DUCK for 64 ticks (~500 ms > 400 ms TIME_TO_DUCK)
        for _ in 0..64 {
            let was_ducking = state.is_ducking;
            pm_player_move(&mut state, &duck_cmd, &vars, dt, &world);
            let eye_z = state.origin[2] + state.view_height;

            // Check continuity across the exact tick where duck finishes and origin drops by 18
            if !was_ducking && state.is_ducking {
                let delta = (eye_z - prev_eye_z).abs();
                assert!(
                    delta < 0.01,
                    "Eye height delta at duck completion tick must be continuous (<0.01 u)! Got delta = {delta}"
                );
                assert_eq!(
                    state.view_height, VEC_VIEW_DUCKING,
                    "View height upon duck completion must snap to VEC_VIEW_DUCKING (12.0)"
                );
                duck_transition_verified = true;
            } else {
                let step_delta = (eye_z - prev_eye_z).abs();
                assert!(
                    step_delta < 1.0,
                    "Sub-tick eye height step delta during ducking must be smooth (<1.0 u/tick), got {step_delta}"
                );
            }

            prev_eye_z = eye_z;
        }

        assert!(
            duck_transition_verified,
            "Duck transition must have finished and been verified"
        );
        assert!(state.is_ducking, "Player must be fully ducked");

        // 2. Unducking phase: release IN_DUCK for 40 ticks (~312 ms > 200 ms TIME_TO_UNDUCK)
        let unduck_cmd = PlayerCmd {
            forward_move: 0.0,
            side_move: 0.0,
            up_move: 0.0,
            buttons: 0,
            view_angles: [0.0, 0.0, 0.0],
        };

        let mut unduck_transition_verified = false;

        for _ in 0..40 {
            let was_ducking = state.is_ducking;
            pm_player_move(&mut state, &unduck_cmd, &vars, dt, &world);
            let eye_z = state.origin[2] + state.view_height;

            // Check continuity at tick 0 of unducking where origin rises by 18
            if was_ducking && !state.is_ducking {
                let delta = (eye_z - prev_eye_z).abs();
                assert!(
                    delta < 1e-4,
                    "Eye height delta at unduck initiation tick must be continuous! Got delta = {delta}"
                );
                unduck_transition_verified = true;
            } else {
                let step_delta = (eye_z - prev_eye_z).abs();
                assert!(
                    step_delta < 1.5,
                    "Sub-tick eye height step delta during unducking must be smooth (<1.5 u/tick), got {step_delta}"
                );
            }

            prev_eye_z = eye_z;
        }

        assert!(
            unduck_transition_verified,
            "Unduck transition initiation must have occurred"
        );
        assert!(
            !state.is_ducking,
            "Player must be fully standing after unducking"
        );
        assert_eq!(
            state.view_height, VEC_VIEW_STANDING,
            "View height must have returned to VEC_VIEW_STANDING (17.0)"
        );

        // 3. Quick tap cancel: release duck before finishing (100 ms < 400 ms)
        let mut tap_state = PlayerState {
            origin: [0.0, 0.0, 0.03125],
            velocity: [0.0, 0.0, 0.0],
            view_height: VEC_VIEW_STANDING,
            on_ground: true,
            ..Default::default()
        };

        // Duck for 12 ticks (~93.75 ms)
        for _ in 0..12 {
            pm_player_move(&mut tap_state, &duck_cmd, &vars, dt, &world);
        }
        assert!(
            tap_state.in_duck && !tap_state.is_ducking,
            "Player should be mid-duck transition"
        );
        assert_eq!(
            tap_state.origin[2], 0.03125,
            "Origin must not have dropped during transition"
        );

        // Release duck immediately
        pm_player_move(&mut tap_state, &unduck_cmd, &vars, dt, &world);
        assert!(
            !tap_state.in_duck && !tap_state.is_ducking,
            "Quick tap release must instantly cancel ducking"
        );
        assert_eq!(
            tap_state.view_height, VEC_VIEW_STANDING,
            "View height must instantly reset to VEC_VIEW_STANDING without jumping"
        );
        assert_eq!(tap_state.origin[2], 0.03125, "Origin must remain untouched");
    }
}
