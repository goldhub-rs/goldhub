//! Counter-Strike 1.6 GameRules FSM, team management, and round lifecycle.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use goldhub_engine_api::GameRules;

/// Player team assignment in Counter-Strike 1.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum CsTeam {
    #[default]
    Unassigned = 0,
    Terrorist = 1,
    CounterTerrorist = 2,
    Spectator = 3,
}

/// Round progression state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RoundState {
    #[default]
    Warmup,
    FreezeTime,
    RoundLive,
    RoundEnd,
}

/// Authoritative Counter-Strike 1.6 game rules state machine.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CsGameRules {
    pub state: RoundState,
    pub round_timer: f32,
    pub freeze_time: f32,
    pub round_time_limit: f32,
    pub buy_time: f32,
    pub t_score: u32,
    pub ct_score: u32,
    pub friendly_fire: bool,
}

impl Default for CsGameRules {
    fn default() -> Self {
        Self {
            state: RoundState::FreezeTime,
            round_timer: 3.0,
            freeze_time: 3.0,
            round_time_limit: 105.0, // 1:45 competitive standard
            buy_time: 15.0,
            t_score: 0,
            ct_score: 0,
            friendly_fire: false,
        }
    }
}

impl CsGameRules {
    /// Creates a new CS 1.6 game rules engine.
    #[must_use]
    pub fn new(freeze_time: f32, round_time_limit: f32, friendly_fire: bool) -> Self {
        Self {
            state: RoundState::FreezeTime,
            round_timer: freeze_time,
            freeze_time,
            round_time_limit,
            buy_time: 15.0,
            t_score: 0,
            ct_score: 0,
            friendly_fire,
        }
    }

    /// Advances the round state machine by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        match self.state {
            RoundState::Warmup => {}
            RoundState::FreezeTime => {
                self.round_timer -= dt;
                if self.round_timer <= 0.0 {
                    self.state = RoundState::RoundLive;
                    self.round_timer = self.round_time_limit;
                }
            }
            RoundState::RoundLive => {
                self.round_timer -= dt;
                if self.round_timer <= 0.0 {
                    // Time ran out -> CT victory on defusal maps
                    self.ct_score += 1;
                    self.state = RoundState::RoundEnd;
                    self.round_timer = 5.0; // 5-second end round intermission
                }
            }
            RoundState::RoundEnd => {
                self.round_timer -= dt;
                if self.round_timer <= 0.0 {
                    self.on_round_start();
                }
            }
        }
    }
}

impl GameRules for CsGameRules {
    fn game_name(&self) -> &'static str {
        "Counter-Strike 1.6"
    }

    fn on_round_start(&mut self) {
        self.state = RoundState::FreezeTime;
        self.round_timer = self.freeze_time;
    }

    fn update_round(&mut self, dt: f32) {
        self.step(dt);
    }

    fn is_friendly_fire_enabled(&self) -> bool {
        self.friendly_fire
    }

    fn can_player_buy(&self, _player_id: u32) -> bool {
        // Can buy during freeze time or within buy_time seconds of round live
        match self.state {
            RoundState::FreezeTime => true,
            RoundState::RoundLive => {
                let elapsed = self.round_time_limit - self.round_timer;
                elapsed <= self.buy_time
            }
            RoundState::Warmup | RoundState::RoundEnd => false,
        }
    }
}
