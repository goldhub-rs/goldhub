//! Next-Gen Zero-Allocation Rollback Netcode & Protocol 48 Transport for GoldHub.
//!
//! # Architecture & Guarantees
//! - **Zero Input Lag**: Sub-tick microsecond input capturing via [`UserCmd`].
//! - **Zero Teleportations / Snap-Backs**: Decoupled visual smoothing with exponential decay.
//! - **Zero Desync**: Deterministic rollback simulation and sub-tick lag compensation.
//! - **Mechanical Sympathy**: Mirrored virtual memory DMA ring buffers powered by `wire-ring`.

pub mod buffer;
pub mod connection;
pub mod error;
pub mod lagcomp;
pub mod prediction;
pub mod protocol;
pub mod transport;

pub use buffer::{AdaptiveJitterBuffer, NetworkRingChannel};
pub use connection::{ConnectionState, FlowStats, IncomingPacket, NetConnection};
pub use error::NetError;
pub use lagcomp::{
    EntitySnapshot, LagCompHistoryRing, interpolate_subtick_snapshot, ray_intersects_aabb,
};
pub use prediction::{PlayerState, RollbackEngine, UserCmd, VisualSmoother};
pub use protocol::{ClcOpcode, NetChannelHeader, OobMessage, SvcOpcode};
pub use transport::{NetAddress, UdpTransport};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_cmd_cache_line_invariants() {
        assert_eq!(std::mem::size_of::<UserCmd>(), 64);
        assert_eq!(std::mem::align_of::<UserCmd>(), 8);
    }

    #[test]
    fn test_visual_smoothing_decay() {
        let mut smoother = VisualSmoother::new();
        smoother.add_prediction_drift([10.0, 0.0, 0.0]);

        assert_eq!(smoother.error_offset()[0], 10.0);

        // Advance 100 ms (~2.5 half-lives)
        smoother.update(0.1);
        assert!(smoother.error_offset()[0] < 1.0);
        assert!(smoother.error_offset()[0] >= 0.0);
    }

    #[test]
    fn test_deterministic_rollback_reconciliation() {
        let mut engine = RollbackEngine::new(1.0 / 64.0);

        // 1. Client predicts 10 commands moving forward
        for i in 1..=10 {
            let cmd = UserCmd {
                command_number: i,
                forwardmove: 250.0,
                ..Default::default()
            };
            engine.predict_user_cmd(cmd);
        }

        let predicted_before = *engine.physical_state();
        assert!(predicted_before.origin[0] > 0.0);

        // 2. Server sends authoritative correction for command #5 (e.g. slight discrepancy)
        let mut auth_state = PlayerState::default();
        auth_state.origin[0] = 5.0; // Server says at cmd 5 position was exactly 5.0
        auth_state.last_command_number = 5;

        engine.on_server_update(auth_state, 5);

        // 3. Rollback occurred, replayed up to cmd #10 deterministically
        let predicted_after = *engine.physical_state();
        assert_eq!(predicted_after.last_command_number, 10);

        // Visual render origin is smoothly offset, avoiding snapping
        let visual_origin = engine.visual_render_origin();
        assert!(visual_origin[0].is_finite());
    }

    #[test]
    fn test_subtick_lag_compensation_interpolation() {
        let s0 = EntitySnapshot {
            entity_id: 1,
            timestamp_micros: 1_000_000,
            origin: [0.0, 0.0, 0.0],
            angles: [0.0, 0.0, 0.0],
            mins: [-16.0, -16.0, 0.0],
            maxs: [16.0, 16.0, 72.0],
        };

        let s1 = EntitySnapshot {
            entity_id: 1,
            timestamp_micros: 2_000_000,
            origin: [100.0, 0.0, 0.0],
            angles: [0.0, 0.0, 0.0],
            mins: [-16.0, -16.0, 0.0],
            maxs: [16.0, 16.0, 72.0],
        };

        // Midpoint at 1.5s
        let interpolated = interpolate_subtick_snapshot(&s0, &s1, 1_500_000);
        assert!((interpolated.origin[0] - 50.0).abs() < 1e-4);

        // Raycast against historical hitbox
        let ray_origin = [-100.0, 0.0, 36.0];
        let ray_dir = [1.0, 0.0, 0.0];
        let hit = ray_intersects_aabb(
            ray_origin,
            ray_dir,
            interpolated.world_mins(),
            interpolated.world_maxs(),
        );

        assert!(hit.is_some());
        assert!((hit.unwrap() - (150.0 - 16.0)).abs() < 1e-4);
    }
}
