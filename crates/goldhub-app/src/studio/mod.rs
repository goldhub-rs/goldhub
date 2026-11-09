//! Unified GoldSrc StudioModel (.mdl) 3D rendering and skeletal animation pipeline.
//!
//! Powers both World entities (hostages, chickens, map props) and first-person
//! ViewModel weapon rendering with 144+ Hz SLERP interpolation and Hor+ FOV.

pub mod animation;
pub mod store;
pub mod viewmodel;
pub mod world_entity;

pub use animation::{StudioAnimatedModel, studio_animation_system};
pub use store::{StudioModelData, StudioModelStore, SubmeshTemplate};
pub use viewmodel::{
    HasViewModel, ViewModelWeapon, spawn_viewmodel_system, viewmodel_bob_and_sway_system,
    viewmodel_input_system, widescreen_horplus_fov_system,
};
pub use world_entity::{
    BspEntitiesLump, EntityCollisionBounds, HostageEntity, HostageState, WorldStudioEntity,
    hostage_interaction_and_ai_system, spawn_map_studio_entities, spawn_studio_entity,
};

use bevy::prelude::*;

pub struct StudioModelPlugin;

impl Plugin for StudioModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StudioModelStore>();
        app.init_resource::<BspEntitiesLump>();
        app.add_systems(
            Update,
            (
                spawn_viewmodel_system,
                viewmodel_input_system,
                viewmodel_bob_and_sway_system,
                widescreen_horplus_fov_system,
                studio_animation_system,
                hostage_interaction_and_ai_system,
            ),
        );
    }
}
