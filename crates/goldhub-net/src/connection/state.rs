//! Connection state machine.

/// Discrete stages in the client-server connection lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    /// Requesting challenge number (`getchallenge`).
    Challenging,
    /// Sending connection request with challenge token (`connect`).
    Connecting,
    /// Connected to server; establishing signon state.
    Connected,
    /// Active gameplay session.
    Active,
}
