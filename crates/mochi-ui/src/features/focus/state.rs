use std::time::{Duration, Instant};

use chrono::Local;

pub use mochi_winrt::mochi_focus::port::{FocusPhase, FocusState};
use mochi_winrt::paths::{Glyphs, HudMessage, SettingsData};
use mochi_winrt::strings::{Language, fmt1};

use crate::state::NotchApp;

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

    pub fn phase_length(&self, settings: &SettingsData) -> Duration {
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
    ) -> (Option<Duration>, Option<HudMessage>) {
        if self.is_break() {
            self.advance(settings, false, Language::En)
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

    pub fn tick(
        &mut self,
        settings: &SettingsData,
        lang: Language,
    ) -> (Option<Duration>, Option<HudMessage>) {
        if self.state != FocusState::Running {
            return (None, None);
        }
        let now = Instant::now();
        let last = self.last_tick;
        self.last_tick = now;
        if now.saturating_duration_since(last) > Duration::from_secs(30) {
            let left = self.ends_at.saturating_duration_since(last);
            if self.phase == FocusPhase::Focus && !left.is_zero() {
                let len = self.phase_length(settings);
                let focused = self.freeze(left.min(len));
                let mins = (left.as_secs_f64() / 60.0).ceil() as i64;
                let mut msg = HudMessage::new(Glyphs::TIMER, lang.strings().focus_paused_asleep);
                msg.value = Some(fmt1(lang.strings().focus_minutes_left, &mins.to_string()));
                msg.duration_ms = 4000;
                msg.important = true;
                msg.tag = Some("focus".to_string());
                return (focused, Some(msg));
            } else if self.phase != FocusPhase::Focus && now >= self.ends_at {
                return self.advance(settings, false, lang);
            }
        }
        if self.remaining().is_zero() {
            return self.advance(settings, true, lang);
        }
        (None, None)
    }

    fn run(&mut self) {
        self.run_from = self.paused_remaining;
        let now = Instant::now();
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
        lang: Language,
    ) -> (Option<Duration>, Option<HudMessage>) {
        let s = lang.strings();
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
            let msg = if notify {
                let mins = self.phase_length(settings).as_secs() / 60;
                let mut m = HudMessage::new(Glyphs::TIMER, s.focus_break_time);
                m.value = Some(format!("{mins} min"));
                m.duration_ms = 4000;
                m.important = true;
                m.celebrate = true;
                m.tag = Some("focus".to_string());
                m.chime = true;
                m.expires = Some(
                    Local::now().naive_local()
                        + chrono::Duration::seconds(self.phase_length(settings).as_secs() as i64),
                );
                Some(m)
            } else {
                None
            };
            self.run();
            (focused, msg)
        } else {
            if self.phase == FocusPhase::LongBreak {
                self.completed_sessions = 0;
            }
            self.phase = FocusPhase::Focus;
            self.state = FocusState::Ready;
            self.paused_remaining = self.phase_length(settings);
            let msg = if notify {
                let mins = self.phase_length(settings).as_secs() / 60;
                let mut m = HudMessage::new(Glyphs::TIMER, s.focus_back_to_focus);
                m.value = Some(format!("{mins} min"));
                m.duration_ms = 4000;
                m.important = true;
                m.tag = Some("focus".to_string());
                m.chime = true;
                Some(m)
            } else {
                None
            };
            (None, msg)
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

impl NotchApp {
    pub fn focus_today_seconds(&self) -> i64 {
        let key = Local::now().format("%Y-%m-%d").to_string();
        let stored = i64::from(self.store.data.focus.days.get(&key).copied().unwrap_or(0));
        stored + self.focus.current_run().as_secs() as i64
    }
}
