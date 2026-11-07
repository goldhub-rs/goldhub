//! Sub-tick simulation runner and render interpolation controller.

use crate::constants::{FIXED_DT, MoveVars};
use crate::movement::pm_player_move;
use crate::player::{PlayerCmd, PlayerState};
use crate::trace::WorldTrace;

/// Fixed-rate sub-tick simulator that decouples physics execution from rendering frame rate.
///
/// Runs at a strict 128 Hz tournament rate, eliminating FPS dependency while allowing
/// ultra-smooth rendering at 60 Hz, 144 Hz, 240 Hz, 500+ Hz via position interpolation.
#[derive(Debug, Clone)]
pub struct FixedSubtickSimulator {
    /// Active movement variables and tournament cvars.
    pub vars: MoveVars,
    /// Authoritative physical state at the current sub-tick.
    pub current: PlayerState,
    /// Physical state from the immediately preceding sub-tick (for interpolation).
    pub previous: PlayerState,
    /// Fractional time accumulator.
    pub accumulator: f32,
    /// Fixed delta time for each sub-tick (default 1/128s).
    pub dt: f32,
}

impl Default for FixedSubtickSimulator {
    fn default() -> Self {
        Self {
            vars: MoveVars::default(),
            current: PlayerState::default(),
            previous: PlayerState::default(),
            accumulator: 0.0,
            dt: FIXED_DT,
        }
    }
}

impl FixedSubtickSimulator {
    /// Creates a simulator initialized at the given spawn origin and yaw.
    #[must_use]
    pub fn new(spawn_origin: [f32; 3], yaw: f32) -> Self {
        let initial = PlayerState::new_at_spawn(spawn_origin, yaw);
        Self {
            vars: MoveVars::default(),
            current: initial,
            previous: initial,
            accumulator: 0.0,
            dt: FIXED_DT,
        }
    }

    /// Advances the simulation by `delta_seconds` of wall-clock time.
    ///
    /// Executes zero, one, or multiple fixed 128 Hz sub-ticks depending on accumulated time.
    #[allow(clippy::while_float)]
    pub fn advance(
        &mut self,
        delta_seconds: f32,
        cmd: &PlayerCmd,
        world: &impl WorldTrace,
    ) {
        // Prevent spiral of death on long frame hiccups (cap at 8 ticks = 62.5ms)
        const MAX_ACCUMULATED_TICKS: f32 = 8.0;
        self.accumulator += delta_seconds.min(self.dt * MAX_ACCUMULATED_TICKS);

        while self.accumulator >= self.dt {
            self.previous = self.current;
            pm_player_move(&mut self.current, cmd, &self.vars, self.dt, world);
            self.accumulator -= self.dt;
        }
    }

    /// Computes smoothly interpolated camera position and view height for current rendering frame.
    ///
    /// Returns `(render_origin, render_view_height)`.
    #[must_use]
    pub fn interpolate_for_render(&self) -> ([f32; 3], f32) {
        let alpha = (self.accumulator / self.dt).clamp(0.0, 1.0);

        let x = self.previous.origin[0] + (self.current.origin[0] - self.previous.origin[0]) * alpha;
        let y = self.previous.origin[1] + (self.current.origin[1] - self.previous.origin[1]) * alpha;
        let z = self.previous.origin[2] + (self.current.origin[2] - self.previous.origin[2]) * alpha;

        let view_h = self.previous.view_height
            + (self.current.view_height - self.previous.view_height) * alpha;

        ([x, y, z], view_h)
    }
}
