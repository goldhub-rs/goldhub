//! Sub-tick input capture, deterministic rollback, and decoupled visual smoothing.

pub mod cmd;
pub mod rollback;
pub mod smoothing;
pub mod state;

pub use cmd::{
    IN_ATTACK, IN_ATTACK2, IN_BACK, IN_CANCEL, IN_DUCK, IN_FORWARD, IN_JUMP, IN_LEFT, IN_MOVELEFT,
    IN_MOVERIGHT, IN_RELOAD, IN_RIGHT, IN_SPEED, IN_USE, UNACKED_RING_SIZE, UnackedCmdRing,
    UserCmd,
};
pub use rollback::RollbackEngine;
pub use smoothing::VisualSmoother;
pub use state::{FL_DUCKING, FL_ONGROUND, PlayerState};
