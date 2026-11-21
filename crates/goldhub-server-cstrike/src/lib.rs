//! Authoritative server plugin and systems for Counter-Strike 1.6.

use bevy::prelude::*;

pub mod c4;
pub mod entities;
pub mod transport;

pub use c4::*;
pub use entities::*;
pub use transport::*;

/// Authoritative server plugin configuring core entities and lifecycle events.
pub struct CsServerPlugin;

impl Plugin for CsServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScenarioMapData>();
        app.init_resource::<BombMapParams>();
        app.add_event::<FireTargetEvent>();
        app.add_event::<BreakEntityEvent>();
        app.add_event::<RoundResetEvent>();
        app.add_event::<C4ExplodeEvent>();
        app.add_event::<C4DefusedEvent>();
    }
}
