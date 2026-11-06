//! Decoupled visual smoothing to eliminate rubberbanding, hitching, and snapping.

/// Exponential smoothing decay rate ($\lambda = 25.0 \implies \approx 28\text{ ms}$ half-life).
const SMOOTHING_DECAY_RATE: f32 = 25.0;

/// Maximum drift distance allowed before snapping (e.g., player respawn or teleport).
const MAX_SMOOTH_DISTANCE: f32 = 256.0;

/// Decoupled visual error smoother.
///
/// Ensures that while physics instantly rolls back and re-simulates authoritatively,
/// the eye of the player never observes a single frame of discrete teleportation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualSmoother {
    error_offset: [f32; 3],
}

impl Default for VisualSmoother {
    fn default() -> Self {
        Self::new()
    }
}

impl VisualSmoother {
    /// Creates a new visual smoother with zero error.
    pub const fn new() -> Self {
        Self {
            error_offset: [0.0; 3],
        }
    }

    /// Registers a misprediction error delta between predicted and authoritative state.
    pub fn add_prediction_drift(&mut self, drift: [f32; 3]) {
        let dist_sq = drift[0] * drift[0] + drift[1] * drift[1] + drift[2] * drift[2];

        // If drift is catastrophic (teleport/respawn), reset instantly without gliding
        if dist_sq > MAX_SMOOTH_DISTANCE * MAX_SMOOTH_DISTANCE {
            self.error_offset = [0.0; 3];
            return;
        }

        self.error_offset[0] += drift[0];
        self.error_offset[1] += drift[1];
        self.error_offset[2] += drift[2];
    }

    /// Advances visual smoothing by frame time `dt`, exponentially decaying error toward zero.
    pub fn update(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        let decay = (-SMOOTHING_DECAY_RATE * dt).exp();
        self.error_offset[0] *= decay;
        self.error_offset[1] *= decay;
        self.error_offset[2] *= decay;

        // Clean cutoff for micro-residuals to save CPU cycles
        if self.error_offset[0].abs() < 0.01
            && self.error_offset[1].abs() < 0.01
            && self.error_offset[2].abs() < 0.01
        {
            self.error_offset = [0.0; 3];
        }
    }

    /// Computes the visual origin to be passed to the rendering pipeline.
    ///
    /// `visual_origin = physical_origin + error_offset`.
    #[inline(always)]
    pub fn compute_render_origin(&self, physical_origin: [f32; 3]) -> [f32; 3] {
        [
            physical_origin[0] + self.error_offset[0],
            physical_origin[1] + self.error_offset[1],
            physical_origin[2] + self.error_offset[2],
        ]
    }

    /// Returns the current remaining visual error offset.
    #[inline(always)]
    pub const fn error_offset(&self) -> [f32; 3] {
        self.error_offset
    }

    /// Clears any existing visual error offset immediately.
    pub fn clear(&mut self) {
        self.error_offset = [0.0; 3];
    }
}
