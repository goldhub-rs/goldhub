//! Planted C4 explosive state machine, gravity, defusal, and detonation logic.

use crate::entities::BombMapParams;
use bevy::prelude::*;

/// Planted C4 explosive entity in the world (`grenade` with `IsC4 = true`).
#[derive(Component, Debug, Clone)]
pub struct PlantedC4 {
    pub timer: f32,
    pub beep_timer: f32,
    pub blink_timer: f32,
    pub defuse_progress: f32,
    pub is_being_defused: bool,
    pub on_ground: bool,
    pub velocity: Vec3,
    pub defuser_entity: Option<Entity>,
}

impl PlantedC4 {
    /// Creates a newly planted C4 entity with canonical parameters.
    #[must_use]
    pub fn new(timer: f32) -> Self {
        Self {
            timer,
            beep_timer: 1.0,
            blink_timer: 1.0,
            defuse_progress: 0.0,
            is_being_defused: false,
            on_ground: true,
            velocity: Vec3::ZERO,
            defuser_entity: None,
        }
    }
}

/// Event emitted when C4 detonates.
#[derive(Event, Debug, Clone)]
pub struct C4ExplodeEvent {
    pub position: Vec3,
    pub damage_radius: f32,
    pub max_damage: f32,
}

/// Event emitted when CT successfully defuses the bomb.
#[derive(Event, Debug, Clone)]
pub struct C4DefusedEvent {
    pub defuser: Option<Entity>,
}

/// Updates planted C4 countdown, defusal progression, and gravity step.
#[allow(clippy::too_many_arguments)]
pub fn step_planted_c4_system(
    time: &Time,
    params: &BombMapParams,
    c4: &mut PlantedC4,
    defuse_held: bool,
    has_defuser_kit: bool,
    defuser: Option<Entity>,
    explode_events: &mut EventWriter<C4ExplodeEvent>,
    defused_events: &mut EventWriter<C4DefusedEvent>,
    pos: Vec3,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    c4.timer -= dt;
    c4.beep_timer -= dt;
    c4.blink_timer -= dt;

    if defuse_held {
        c4.is_being_defused = true;
        c4.defuser_entity = defuser;
        let required_time = if has_defuser_kit {
            params.defuse_time_with_kit
        } else {
            params.defuse_time_without_kit
        };
        c4.defuse_progress += dt / required_time;
        if c4.defuse_progress >= 1.0 {
            defused_events.send(C4DefusedEvent { defuser });
            return;
        }
    } else {
        c4.is_being_defused = false;
        c4.defuse_progress = 0.0;
        c4.defuser_entity = None;
    }

    if c4.timer <= 0.0 {
        explode_events.send(C4ExplodeEvent {
            position: pos,
            damage_radius: params.c4_damage_radius,
            max_damage: 500.0,
        });
    }
}
