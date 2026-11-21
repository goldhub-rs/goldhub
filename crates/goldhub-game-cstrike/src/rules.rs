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

/// Active map scenario type inferred from BSP entities (`CheckMapConditions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MapScenarioKind {
    #[default]
    Deathmatch,
    BombDefusal,
    HostageRescue,
    Assassination,
    PrisonEscape,
}

/// BSP map condition flags (`CHalfLifeMultiplay::CheckMapConditions`).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MapScenarioFlags {
    pub has_bomb_target: bool,
    pub has_bomb_zone: bool,
    pub has_rescue_zone: bool,
    pub has_vip_safety_zone: bool,
    pub has_escape_zone: bool,
    pub has_buy_zone: bool,
    pub ct_cant_buy: bool,
    pub t_cant_buy: bool,
    pub bomb_radius: f32,
    pub hostage_count: u32,
}

impl Default for MapScenarioFlags {
    fn default() -> Self {
        Self {
            has_bomb_target: false,
            has_bomb_zone: false,
            has_rescue_zone: false,
            has_vip_safety_zone: false,
            has_escape_zone: false,
            has_buy_zone: false,
            ct_cant_buy: false,
            t_cant_buy: false,
            bomb_radius: 500.0,
            hostage_count: 0,
        }
    }
}

impl MapScenarioFlags {
    /// Returns the primary active scenario mode for HUD/rules evaluation.
    #[must_use]
    pub const fn primary_kind(&self) -> MapScenarioKind {
        if self.has_bomb_target {
            MapScenarioKind::BombDefusal
        } else if self.hostage_count > 0 || self.has_rescue_zone {
            MapScenarioKind::HostageRescue
        } else if self.has_vip_safety_zone {
            MapScenarioKind::Assassination
        } else if self.has_escape_zone {
            MapScenarioKind::PrisonEscape
        } else {
            MapScenarioKind::Deathmatch
        }
    }
}

/// Canonical CS 1.6 round victory reason (`RoundEndEvent` / `WINSTATUS_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RoundEndReason {
    TargetBombed,
    BombDefused,
    TargetSaved,
    AllHostagesRescued,
    HostagesNotRescued,
    VipEscaped,
    VipAssassinated,
    VipNotEscaped,
    TerroristsEscaped,
    CTsPreventEscape,
    EscapingTerroristsNeutralized,
    TerroristsWin,
    CTsWin,
    RoundDraw,
}

/// Outcome metadata produced when a round terminates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RoundOutcome {
    pub winner: CsTeam,
    pub reason: RoundEndReason,
    pub t_reward: i32,
    pub ct_reward: i32,
}

impl RoundEndReason {
    /// Returns the CS 1.6 localization key (e.g. `"#Target_Bombed"`) for this round end reason.
    #[must_use]
    pub const fn localization_key(self) -> &'static str {
        match self {
            Self::TargetBombed => "#Target_Bombed",
            Self::BombDefused => "#Bomb_Defused",
            Self::TargetSaved => "#Target_Saved",
            Self::AllHostagesRescued => "#All_Hostages_Rescued",
            Self::HostagesNotRescued => "#Hostages_Not_Rescued",
            Self::VipEscaped => "#VIP_Escaped",
            Self::VipAssassinated => "#VIP_Assassinated",
            Self::VipNotEscaped => "#VIP_Not_Escaped",
            Self::TerroristsEscaped => "#Terrorists_Escaped",
            Self::CTsPreventEscape => "#CTs_PreventEscape",
            Self::EscapingTerroristsNeutralized => "#Escaping_Terrorists_Neutralized",
            Self::TerroristsWin => "#Terrorists_Win",
            Self::CTsWin => "#CTs_Win",
            Self::RoundDraw => "#Round_Draw",
        }
    }

    /// Returns the classic CS 1.6 victory/draw radio audio asset path.
    #[must_use]
    pub const fn radio_sound(self) -> &'static str {
        match self {
            Self::TargetBombed
            | Self::HostagesNotRescued
            | Self::VipAssassinated
            | Self::VipNotEscaped
            | Self::TerroristsEscaped
            | Self::TerroristsWin => "radio/terwin.wav",
            Self::BombDefused
            | Self::TargetSaved
            | Self::AllHostagesRescued
            | Self::VipEscaped
            | Self::CTsPreventEscape
            | Self::EscapingTerroristsNeutralized
            | Self::CTsWin => "radio/ctwin.wav",
            Self::RoundDraw => "radio/rounddraw.wav",
        }
    }
}

impl RoundOutcome {
    /// Returns the localization key associated with this outcome's reason.
    #[must_use]
    pub const fn localization_key(self) -> &'static str {
        self.reason.localization_key()
    }

    /// Returns the radio sound associated with this outcome's reason.
    #[must_use]
    pub const fn radio_sound(self) -> &'static str {
        self.reason.radio_sound()
    }

    /// Builds canonical ReGameDLL win metadata and cash rewards for a round end reason.
    #[must_use]
    pub const fn from_reason(reason: RoundEndReason, has_bomb_target: bool) -> Self {
        match reason {
            RoundEndReason::TargetBombed => Self {
                winner: CsTeam::Terrorist,
                reason,
                t_reward: 3500,
                ct_reward: 1400,
            },
            RoundEndReason::BombDefused => Self {
                winner: CsTeam::CounterTerrorist,
                reason,
                t_reward: 800,
                ct_reward: 3250,
            },
            RoundEndReason::TargetSaved => Self {
                winner: CsTeam::CounterTerrorist,
                reason,
                t_reward: 1400,
                ct_reward: 3250,
            },
            RoundEndReason::AllHostagesRescued => Self {
                winner: CsTeam::CounterTerrorist,
                reason,
                t_reward: 1400,
                ct_reward: 2500,
            },
            RoundEndReason::HostagesNotRescued
            | RoundEndReason::VipAssassinated
            | RoundEndReason::VipNotEscaped => Self {
                winner: CsTeam::Terrorist,
                reason,
                t_reward: 3250,
                ct_reward: 1400,
            },
            RoundEndReason::VipEscaped => Self {
                winner: CsTeam::CounterTerrorist,
                reason,
                t_reward: 1400,
                ct_reward: 3500,
            },
            RoundEndReason::TerroristsEscaped => Self {
                winner: CsTeam::Terrorist,
                reason,
                t_reward: 3000,
                ct_reward: 1400,
            },
            RoundEndReason::CTsPreventEscape | RoundEndReason::EscapingTerroristsNeutralized => {
                Self {
                    winner: CsTeam::CounterTerrorist,
                    reason,
                    t_reward: 0,
                    ct_reward: 3250,
                }
            }
            RoundEndReason::TerroristsWin => Self {
                winner: CsTeam::Terrorist,
                reason,
                t_reward: if has_bomb_target { 3250 } else { 3000 },
                ct_reward: 1400,
            },
            RoundEndReason::CTsWin => Self {
                winner: CsTeam::CounterTerrorist,
                reason,
                t_reward: 1400,
                ct_reward: if has_bomb_target { 3250 } else { 3000 },
            },
            RoundEndReason::RoundDraw => Self {
                winner: CsTeam::Unassigned,
                reason,
                t_reward: 0,
                ct_reward: 0,
            },
        }
    }
}

/// Live entity & player snapshot consumed by `CsGameRules::check_win_conditions`.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ScenarioTickCounts {
    pub alive_t: u32,
    pub dead_t: u32,
    pub spawnable_t: u32,
    pub alive_ct: u32,
    pub dead_ct: u32,
    pub spawnable_ct: u32,
    pub bomb_planted: bool,
    pub target_bombed: bool,
    pub bomb_defused: bool,
    pub hostages_total: u32,
    pub hostages_alive: u32,
    pub hostages_rescued: u32,
    pub vip_present: bool,
    pub vip_escaped: bool,
    pub vip_dead: bool,
    pub escaped_t: u32,
    pub total_escapers_t: u32,
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
    pub round_restart_delay: f32,
    pub t_score: u32,
    pub ct_score: u32,
    pub total_rounds_played: u32,
    pub t_loser_streak: u32,
    pub ct_loser_streak: u32,
    pub friendly_fire: bool,
    pub map_flags: MapScenarioFlags,
    pub last_outcome: Option<RoundOutcome>,
    pub round_terminating: bool,
}

impl Default for CsGameRules {
    fn default() -> Self {
        Self {
            state: RoundState::FreezeTime,
            round_timer: 3.0,
            freeze_time: 3.0,
            round_time_limit: 105.0, // 1:45 competitive standard
            buy_time: 15.0,
            round_restart_delay: 5.0,
            t_score: 0,
            ct_score: 0,
            total_rounds_played: 0,
            t_loser_streak: 0,
            ct_loser_streak: 0,
            friendly_fire: false,
            map_flags: MapScenarioFlags::default(),
            last_outcome: None,
            round_terminating: false,
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
            round_restart_delay: 5.0,
            t_score: 0,
            ct_score: 0,
            total_rounds_played: 0,
            t_loser_streak: 0,
            ct_loser_streak: 0,
            friendly_fire,
            map_flags: MapScenarioFlags::default(),
            last_outcome: None,
            round_terminating: false,
        }
    }

    /// Evaluates ReGameDLL `CheckWinConditions` priority order; returns `Some(RoundOutcome)` when round terminates.
    pub fn evaluate_scenario(
        &mut self,
        dt: f32,
        counts: ScenarioTickCounts,
    ) -> Option<RoundOutcome> {
        match self.state {
            RoundState::Warmup => None,
            RoundState::FreezeTime => {
                self.round_timer -= dt;
                if self.round_timer <= 0.0 {
                    self.state = RoundState::RoundLive;
                    self.round_timer = self.round_time_limit;
                }
                None
            }
            RoundState::RoundEnd => {
                self.round_timer -= dt;
                if self.round_timer <= 0.0 {
                    self.on_round_start();
                }
                None
            }
            RoundState::RoundLive => {
                // While C4 is actively planted, round timer pauses and never expires (ReGameDLL HasRoundTimeExpired)
                if !counts.bomb_planted {
                    self.round_timer -= dt;
                }

                // 1. Assassination (as_)
                if self.map_flags.has_vip_safety_zone && counts.vip_present {
                    if counts.vip_escaped {
                        return Some(self.terminate_with(RoundEndReason::VipEscaped));
                    }
                    if counts.vip_dead {
                        return Some(self.terminate_with(RoundEndReason::VipAssassinated));
                    }
                }

                // 2. Prison Escape (es_)
                if self.map_flags.has_escape_zone && counts.total_escapers_t > 0 {
                    let ratio = (counts.escaped_t as f32) / (counts.total_escapers_t as f32);
                    if ratio >= 0.5 {
                        return Some(self.terminate_with(RoundEndReason::TerroristsEscaped));
                    }
                    if counts.alive_t == 0 {
                        if counts.dead_t > 0 && counts.spawnable_ct > 0 {
                            return Some(
                                self.terminate_with(RoundEndReason::EscapingTerroristsNeutralized),
                            );
                        }
                        return Some(self.terminate_with(RoundEndReason::CTsPreventEscape));
                    }
                }

                // 3. Bomb Defusal (de_)
                if self.map_flags.has_bomb_target {
                    if counts.target_bombed {
                        return Some(self.terminate_with(RoundEndReason::TargetBombed));
                    }
                    if counts.bomb_defused {
                        return Some(self.terminate_with(RoundEndReason::BombDefused));
                    }
                }

                // 4. Team Extermination (only when BOTH sides have spawnable fighters so solo practice never auto-loops!)
                if counts.spawnable_ct > 0 && counts.spawnable_t > 0 {
                    if counts.alive_t == 0 && counts.dead_t > 0 && counts.alive_ct > 0 {
                        if !counts.bomb_planted {
                            return Some(self.terminate_with(RoundEndReason::CTsWin));
                        }
                    } else if counts.alive_ct == 0 && counts.dead_ct > 0 {
                        return Some(self.terminate_with(RoundEndReason::TerroristsWin));
                    }
                } else if counts.dead_t > 0
                    && counts.alive_t == 0
                    && counts.spawnable_t > 0
                    && !counts.bomb_planted
                {
                    // Solo player death ends round and restarts after 5s
                    return Some(self.terminate_with(RoundEndReason::CTsWin));
                } else if counts.dead_ct > 0 && counts.alive_ct == 0 && counts.spawnable_ct > 0 {
                    return Some(self.terminate_with(RoundEndReason::TerroristsWin));
                }

                // 5. Hostage Rescue (cs_)
                if counts.hostages_total > 0 && counts.hostages_rescued >= counts.hostages_total {
                    return Some(self.terminate_with(RoundEndReason::AllHostagesRescued));
                }

                // 6. Round Timer Expiry (only when C4 is NOT planted)
                if !counts.bomb_planted && self.round_timer <= 0.0 {
                    if self.map_flags.has_bomb_target {
                        return Some(self.terminate_with(RoundEndReason::TargetSaved));
                    }
                    if counts.hostages_total > 0 || self.map_flags.has_rescue_zone {
                        return Some(self.terminate_with(RoundEndReason::HostagesNotRescued));
                    }
                    if self.map_flags.has_escape_zone {
                        return Some(self.terminate_with(RoundEndReason::CTsPreventEscape));
                    }
                    if self.map_flags.has_vip_safety_zone {
                        return Some(self.terminate_with(RoundEndReason::VipNotEscaped));
                    }
                    // Fallback generic time expiry
                    return Some(self.terminate_with(RoundEndReason::RoundDraw));
                }

                None
            }
        }
    }

    /// Transitions immediately into `RoundEnd` with `outcome`, updating scores and streak bonuses.
    pub fn terminate_with(&mut self, reason: RoundEndReason) -> RoundOutcome {
        let mut outcome = RoundOutcome::from_reason(reason, self.map_flags.has_bomb_target);
        match outcome.winner {
            CsTeam::Terrorist => {
                self.t_score += 1;
                self.t_loser_streak = 0;
                self.ct_loser_streak = self.ct_loser_streak.saturating_add(1);
                if self.ct_loser_streak > 1 && outcome.ct_reward > 0 {
                    outcome.ct_reward =
                        (1500 + (self.ct_loser_streak - 2).cast_signed() * 500).min(3000);
                }
            }
            CsTeam::CounterTerrorist => {
                self.ct_score += 1;
                self.ct_loser_streak = 0;
                self.t_loser_streak = self.t_loser_streak.saturating_add(1);
                if self.t_loser_streak > 1 && reason != RoundEndReason::BombDefused {
                    outcome.t_reward =
                        (1500 + (self.t_loser_streak - 2).cast_signed() * 500).min(3000);
                }
            }
            _ => {}
        }
        self.total_rounds_played += 1;
        self.state = RoundState::RoundEnd;
        self.round_timer = self.round_restart_delay;
        self.last_outcome = Some(outcome);
        self.round_terminating = true;
        outcome
    }

    /// Returns whether `team` is currently allowed to purchase items (`func_buyzone` + `mp_buytime` + `info_map_parameters`).
    #[must_use]
    pub fn can_team_buy(&self, team: CsTeam, in_buy_zone: bool) -> bool {
        if !in_buy_zone {
            return false;
        }
        if matches!(team, CsTeam::CounterTerrorist) && self.map_flags.ct_cant_buy {
            return false;
        }
        if matches!(team, CsTeam::Terrorist) && self.map_flags.t_cant_buy {
            return false;
        }
        if matches!(team, CsTeam::Spectator | CsTeam::Unassigned) {
            return false;
        }
        self.can_player_buy(0)
    }

    /// Advances the round state machine by `dt` seconds (preserving minimal compatibility helper).
    pub fn step(&mut self, dt: f32) {
        let mut counts = ScenarioTickCounts::default();
        if self.map_flags.has_bomb_target {
            let _ = &mut counts;
        }
        // Keep simple step helper deterministic for legacy de_ timeout compatibility
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
                    let _ = self.terminate_with(RoundEndReason::TargetSaved);
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
        self.last_outcome = None;
        self.round_terminating = false;
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
