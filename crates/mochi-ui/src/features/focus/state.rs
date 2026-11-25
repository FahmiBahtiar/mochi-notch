use chrono::Local;

pub use mochi_winrt::mochi_focus::port::{FocusController, FocusNotice, FocusPhase, FocusState};
use mochi_winrt::paths::{Glyphs, HudMessage, SettingsData};
use mochi_winrt::strings::{Language, fmt1};

use crate::state::NotchApp;

pub fn focus_notice_to_hud(
    notice: FocusNotice,
    settings: &SettingsData,
    lang: Language,
) -> HudMessage {
    let s = lang.strings();
    match notice {
        FocusNotice::BreakStarted { minutes } => {
            let mut m = HudMessage::new(Glyphs::TIMER, s.focus_break_time);
            m.value = Some(format!("{minutes} min"));
            m.duration_ms = 4000;
            m.important = true;
            m.celebrate = true;
            m.tag = Some("focus".to_string());
            m.chime = true;
            let break_secs = match settings.short_break_minutes.max(1) {
                _ if minutes == settings.long_break_minutes.max(1) as u64 => {
                    i64::from(settings.long_break_minutes.max(1)) * 60
                }
                _ => (minutes as i64) * 60,
            };
            m.expires = Some(Local::now().naive_local() + chrono::Duration::seconds(break_secs));
            m
        }
        FocusNotice::BackToFocus { minutes } => {
            let mut m = HudMessage::new(Glyphs::TIMER, s.focus_back_to_focus);
            m.value = Some(format!("{minutes} min"));
            m.duration_ms = 4000;
            m.important = true;
            m.tag = Some("focus".to_string());
            m.chime = true;
            m
        }
        FocusNotice::PausedAsleep { minutes_left } => {
            let mut m = HudMessage::new(Glyphs::TIMER, s.focus_paused_asleep);
            m.value = Some(fmt1(s.focus_minutes_left, &minutes_left.to_string()));
            m.duration_ms = 4000;
            m.important = true;
            m.tag = Some("focus".to_string());
            m
        }
    }
}

impl NotchApp {
    pub fn focus_today_seconds(&self) -> i64 {
        let key = Local::now().format("%Y-%m-%d").to_string();
        let stored = i64::from(self.store.data.focus.days.get(&key).copied().unwrap_or(0));
        stored + self.focus.current_run().as_secs() as i64
    }
}
