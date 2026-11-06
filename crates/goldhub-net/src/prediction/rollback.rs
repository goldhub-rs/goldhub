//! Deterministic rollback, replay engine, and misprediction reconciliation.

use super::cmd::{UNACKED_RING_SIZE, UnackedCmdRing, UserCmd};
use super::smoothing::VisualSmoother;
use super::state::PlayerState;

/// Tolerance threshold (in GoldSrc units / inches) before triggering a rollback replay.
const MISPREDICTION_THRESHOLD_SQ: f32 = 0.01;

/// Next-gen deterministic rollback prediction engine.
pub struct RollbackEngine {
    current_predicted: PlayerState,
    history_states: [PlayerState; UNACKED_RING_SIZE],
    unacked_cmds: UnackedCmdRing,
    smoother: VisualSmoother,
    fixed_dt: f32,
}

impl RollbackEngine {
    /// Creates a new rollback engine with the specified fixed simulation delta time (e.g., 1.0 / 64.0).
    pub fn new(fixed_dt: f32) -> Self {
        Self {
            current_predicted: PlayerState::default(),
            history_states: [PlayerState::default(); UNACKED_RING_SIZE],
            unacked_cmds: UnackedCmdRing::new(),
            smoother: VisualSmoother::new(),
            fixed_dt,
        }
    }

    /// Predicts a new user input command locally without any server round-trip delay (Zero Input Lag).
    pub fn predict_user_cmd(&mut self, cmd: UserCmd) -> PlayerState {
        self.unacked_cmds.push_cmd(cmd);
        self.current_predicted.step_prediction(&cmd, self.fixed_dt);

        let slot = (cmd.command_number as usize) & (UNACKED_RING_SIZE - 1);
        self.history_states[slot] = self.current_predicted;

        self.current_predicted
    }

    /// Reconciles local predicted state with an authoritative server state update.
    ///
    /// If drift is detected, triggers zero-allocation deterministic rollback & replay
    /// while feeding the visual smoother to eliminate snapping.
    pub fn on_server_update(&mut self, auth_state: PlayerState, acked_cmd_num: u32) {
        self.unacked_cmds.acknowledge(acked_cmd_num);

        let slot = (acked_cmd_num as usize) & (UNACKED_RING_SIZE - 1);
        let predicted_at_tick = self.history_states[slot];

        let drift = [
            predicted_at_tick.origin[0] - auth_state.origin[0],
            predicted_at_tick.origin[1] - auth_state.origin[1],
            predicted_at_tick.origin[2] - auth_state.origin[2],
        ];

        let drift_sq = drift[0] * drift[0] + drift[1] * drift[1] + drift[2] * drift[2];

        if drift_sq > MISPREDICTION_THRESHOLD_SQ {
            // Misprediction detected: register drift with visual smoother
            self.smoother.add_prediction_drift(drift);

            // 1. Rollback: restore authoritative physical state
            let mut state = auth_state;

            // 2. Replay: re-execute all subsequent unacknowledged commands
            let remaining_cmds = self
                .unacked_cmds
                .get_unacked_range(acked_cmd_num.wrapping_add(1));
            for cmd in remaining_cmds {
                state.step_prediction(&cmd, self.fixed_dt);
                let replay_slot = (cmd.command_number as usize) & (UNACKED_RING_SIZE - 1);
                self.history_states[replay_slot] = state;
            }

            self.current_predicted = state;
        }
    }

    /// Advances frame-rate dependent visual smoothing.
    pub fn update_render_smoothing(&mut self, render_dt: f32) {
        self.smoother.update(render_dt);
    }

    /// Computes the final visual render origin (with decoupled smoothing applied).
    #[inline(always)]
    pub fn visual_render_origin(&self) -> [f32; 3] {
        self.smoother
            .compute_render_origin(self.current_predicted.origin)
    }

    /// Returns the raw authoritative/predicted physical state.
    #[inline(always)]
    pub const fn physical_state(&self) -> &PlayerState {
        &self.current_predicted
    }

    /// Accesses the underlying visual smoother.
    #[inline(always)]
    pub const fn smoother(&self) -> &VisualSmoother {
        &self.smoother
    }
}
