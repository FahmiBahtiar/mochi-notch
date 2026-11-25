//! Pure Pomodoro focus timer state machine and duration formatting.

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use mochi_store::port::SettingsData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl FocusPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::ShortBreak => "ShortBreak",
            Self::LongBreak => "LongBreak",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusState {
    Ready,
    Running,
    Paused,
}

impl FocusState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Paused => "Paused",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusNotice {
    BreakStarted { minutes: u64 },
    BackToFocus { minutes: u64 },
    PausedAsleep { minutes_left: i64 },
}

pub struct FocusController {
    pub phase: FocusPhase,
    pub state: FocusState,
    pub completed_sessions: i32,
    pub paused_remaining: Duration,
    pub run_from: Duration,
    pub ends_at: Instant,
    pub last_tick: Instant,
    pub simulated_now: Option<Instant>,
}

impl FocusController {
    pub fn new(settings: &SettingsData) -> Self {
        let len = Duration::from_secs((settings.focus_minutes.max(1) as u64) * 60);
        let now = Instant::now();
        Self {
            phase: FocusPhase::Focus,
            state: FocusState::Ready,
            completed_sessions: 0,
            paused_remaining: len,
            run_from: len,
            ends_at: now,
            last_tick: now,
            simulated_now: None,
        }
    }

    pub fn sessions_per_cycle(&self, settings: &SettingsData) -> i32 {
        settings.sessions_per_cycle.max(1)
    }

    fn phase_length(&self, settings: &SettingsData) -> Duration {
        let mins = match self.phase {
            FocusPhase::ShortBreak => settings.short_break_minutes,
            FocusPhase::LongBreak => settings.long_break_minutes,
            FocusPhase::Focus => settings.focus_minutes,
        };
        Duration::from_secs((mins.max(1) as u64) * 60)
    }

    pub fn is_running(&self) -> bool {
        self.state == FocusState::Running
    }

    pub fn is_paused(&self) -> bool {
        self.state == FocusState::Paused
    }

    pub fn is_active(&self) -> bool {
        self.state != FocusState::Ready
    }

    pub fn is_break(&self) -> bool {
        self.phase != FocusPhase::Focus
    }

    pub fn remaining(&self) -> Duration {
        if !self.is_running() {
            return self.paused_remaining;
        }
        let now = self.simulated_now.unwrap_or_else(Instant::now);
        self.ends_at.saturating_duration_since(now)
    }

    pub fn progress(&self, settings: &SettingsData) -> f64 {
        let len = self.phase_length(settings).as_secs_f64().max(1.0);
        1.0 - self.remaining().as_secs_f64() / len
    }

    pub fn current_run(&self) -> Duration {
        if self.state == FocusState::Running && self.phase == FocusPhase::Focus {
            let rem = self.remaining();
            self.run_from.saturating_sub(rem)
        } else {
            Duration::ZERO
        }
    }

    pub fn apply_settings(&mut self, settings: &SettingsData) {
        let len = self.phase_length(settings);
        if self.state == FocusState::Ready || self.paused_remaining > len {
            self.paused_remaining = len;
        }
        let spc = self.sessions_per_cycle(settings);
        if self.completed_sessions > spc {
            self.completed_sessions = spc;
        }
    }

    pub fn start(&mut self) {
        if self.state == FocusState::Ready {
            self.run();
        }
    }

    pub fn pause(&mut self) -> Option<Duration> {
        if self.state != FocusState::Running {
            return None;
        }
        let rem = self.remaining();
        self.freeze(rem)
    }

    pub fn resume(&mut self) {
        if self.state == FocusState::Paused {
            self.run();
        }
    }

    pub fn stop(&mut self, settings: &SettingsData) -> Option<Duration> {
        let focused = if self.state == FocusState::Running {
            self.end_run(self.remaining())
        } else {
            None
        };
        self.phase = FocusPhase::Focus;
        self.state = FocusState::Ready;
        self.paused_remaining = self.phase_length(settings);
        focused
    }

    pub fn skip_break(
        &mut self,
        settings: &SettingsData,
    ) -> (Option<Duration>, Option<FocusNotice>) {
        if self.is_break() {
            self.advance(settings, false)
        } else {
            (None, None)
        }
    }

    pub fn pause_after_idle(
        &mut self,
        unattended: Duration,
        settings: &SettingsData,
    ) -> (bool, Option<Duration>) {
        if self.state != FocusState::Running {
            return (false, None);
        }
        let len = self.phase_length(settings);
        let rem = (self.remaining() + unattended).min(len);
        let focused = self.freeze(rem);
        (true, focused)
    }

    pub fn tick(&mut self, settings: &SettingsData) -> (Option<Duration>, Option<FocusNotice>) {
        if self.state != FocusState::Running {
            return (None, None);
        }
        let now = self.simulated_now.unwrap_or_else(Instant::now);
        let last = self.last_tick;
        self.last_tick = now;
        if now.saturating_duration_since(last) > Duration::from_secs(30) {
            let left = self.ends_at.saturating_duration_since(last);
            if self.phase == FocusPhase::Focus && !left.is_zero() {
                let len = self.phase_length(settings);
                let focused = self.freeze(left.min(len));
                let mins = (left.as_secs_f64() / 60.0).ceil() as i64;
                return (
                    focused,
                    Some(FocusNotice::PausedAsleep { minutes_left: mins }),
                );
            } else if self.phase != FocusPhase::Focus && now >= self.ends_at {
                return self.advance(settings, false);
            }
        }
        if self.remaining().is_zero() {
            return self.advance(settings, true);
        }
        (None, None)
    }

    fn run(&mut self) {
        self.run_from = self.paused_remaining;
        let now = self.simulated_now.unwrap_or_else(Instant::now);
        self.last_tick = now;
        self.ends_at = now + self.paused_remaining;
        self.state = FocusState::Running;
    }

    fn freeze(&mut self, remaining: Duration) -> Option<Duration> {
        let focused = if self.state == FocusState::Running {
            self.end_run(remaining)
        } else {
            None
        };
        self.paused_remaining = remaining;
        self.state = FocusState::Paused;
        focused
    }

    fn advance(
        &mut self,
        settings: &SettingsData,
        notify: bool,
    ) -> (Option<Duration>, Option<FocusNotice>) {
        if self.phase == FocusPhase::Focus {
            let focused = if self.state == FocusState::Running {
                self.end_run(Duration::ZERO)
            } else {
                None
            };
            self.completed_sessions += 1;
            self.phase = if self.completed_sessions >= self.sessions_per_cycle(settings) {
                FocusPhase::LongBreak
            } else {
                FocusPhase::ShortBreak
            };
            self.paused_remaining = self.phase_length(settings);
            let notice = notify.then(|| FocusNotice::BreakStarted {
                minutes: self.phase_length(settings).as_secs() / 60,
            });
            self.run();
            (focused, notice)
        } else {
            if self.phase == FocusPhase::LongBreak {
                self.completed_sessions = 0;
            }
            self.phase = FocusPhase::Focus;
            self.state = FocusState::Ready;
            self.paused_remaining = self.phase_length(settings);
            let notice = notify.then(|| FocusNotice::BackToFocus {
                minutes: self.phase_length(settings).as_secs() / 60,
            });
            (None, notice)
        }
    }

    fn end_run(&mut self, remaining: Duration) -> Option<Duration> {
        let ran = if self.phase == FocusPhase::Focus {
            let d = self.run_from.saturating_sub(remaining);
            if d > Duration::ZERO { Some(d) } else { None }
        } else {
            None
        };
        self.run_from = remaining;
        ran
    }
}

pub fn format_focus_duration(minutes: i64) -> String {
    let m = minutes.clamp(0, 100_000);
    if m < 60 {
        format!("{m} min")
    } else if m % 60 == 0 {
        format!("{}h", m / 60)
    } else {
        format!("{}h {:02}", m / 60, m % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_matches_examples() {
        assert_eq!(format_focus_duration(25), "25 min");
        assert_eq!(format_focus_duration(100), "1h 40");
        assert_eq!(format_focus_duration(120), "2h");
    }

    #[test]
    fn focus_timer_transitions_to_break_after_completion() {
        let settings = SettingsData {
            focus_minutes: 5,
            ..SettingsData::default()
        };
        let mut fc = FocusController::new(&settings);
        let t0 = Instant::now();
        fc.simulated_now = Some(t0);
        fc.start();
        fc.simulated_now = Some(t0 + Duration::from_secs(300));
        fc.last_tick = t0 + Duration::from_secs(299);
        let (focused, notice) = fc.tick(&settings);
        assert_eq!(focused, Some(Duration::from_secs(300)));
        assert_eq!(notice, Some(FocusNotice::BreakStarted { minutes: 5 }));
        assert_eq!(fc.phase, FocusPhase::ShortBreak);
        assert_eq!(fc.state, FocusState::Running);
        assert_eq!(fc.completed_sessions, 1);
    }
}
