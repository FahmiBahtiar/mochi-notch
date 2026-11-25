use std::collections::VecDeque;
use std::time::{Duration, Instant};

use chrono::Local;

use mochi_winrt::audio::AudioEvent;
use mochi_winrt::mochi_presence::port::{AwayItem, AwayReport};
use mochi_winrt::native::Chime;
use mochi_winrt::paths::{Glyphs, HudKind, HudMessage};
use mochi_winrt::watchers::windows_wants_quiet;

use crate::d2d::{D2dFactories, FontKind};
use crate::state::{HUD_PADDING, HoverTarget, NotchApp};

#[derive(Debug, Clone, Default)]
pub struct HudQueue {
    waiting: VecDeque<HudMessage>,
    pub current: Option<HudMessage>,
}

impl HudQueue {
    pub fn offer(&mut self, msg: HudMessage, can_show: bool) -> Option<HudMessage> {
        if let Some(tag) = msg.tag.as_deref() {
            self.waiting.retain(|m| m.tag.as_deref() != Some(tag));
        }
        let replaces_current = msg.tag.is_some()
            && self
                .current
                .as_ref()
                .is_some_and(|c| c.tag.as_deref() == msg.tag.as_deref());

        if !can_show {
            if msg.important {
                self.enqueue(msg, false);
            }
            return None;
        }
        if replaces_current {
            self.current = Some(msg.clone());
            return Some(msg);
        }
        if let Some(cur) = self.current.clone()
            && cur.important
        {
            if msg.important {
                self.enqueue(msg, false);
                return None;
            }
            self.enqueue(cur, true);
        }
        self.current = Some(msg.clone());
        Some(msg)
    }

    pub fn finish(&mut self) -> Option<HudMessage> {
        self.current = None;
        self.next()
    }

    pub fn interrupt(&mut self) {
        if let Some(cur) = self.current.take()
            && cur.important
        {
            self.enqueue(cur, true);
        }
    }

    pub fn resume(&mut self) -> Option<HudMessage> {
        if self.current.is_none() {
            self.next()
        } else {
            None
        }
    }

    fn next(&mut self) -> Option<HudMessage> {
        let now = Local::now().naive_local();
        while let Some(first) = self.waiting.pop_front() {
            if first.expires.is_none_or(|exp| exp > now) {
                self.current = Some(first.clone());
                return Some(first);
            }
        }
        None
    }

    fn enqueue(&mut self, msg: HudMessage, first: bool) {
        if let Some(pos) = self
            .waiting
            .iter()
            .position(|m| m.kind == msg.kind && m.text == msg.text)
        {
            self.waiting.remove(pos);
        }
        if first {
            self.waiting.push_front(msg);
        } else {
            self.waiting.push_back(msg);
        }
        while self.waiting.len() > 5 {
            if first {
                self.waiting.remove(1);
            } else {
                self.waiting.pop_front();
            }
        }
    }
}

impl NotchApp {
    pub fn can_show_hud(&self) -> bool {
        self.is_visible()
            && !self.expanded
            && !self.news_active
            && self.hover != HoverTarget::Bubble
    }

    pub fn chime_allowed(&self) -> bool {
        self.store.data.settings.sounds_enabled
            && !(self.store.data.settings.quiet_during_calls && !self.mic_apps.is_empty())
            && !windows_wants_quiet()
    }

    pub fn show_hud(&mut self, msg: HudMessage, factories: Option<&D2dFactories>) {
        if !self.store.data.settings.shows(msg.kind) {
            return;
        }
        let should_record = matches!(
            msg.kind,
            HudKind::Reminder
                | HudKind::Activity
                | HudKind::Network
                | HudKind::Power
                | HudKind::Privacy
        ) && msg.tag.as_deref() != Some("win-notif");
        if should_record {
            let body = msg.value.clone().unwrap_or_default();
            self.push_notification(
                None,
                "Mochi".to_string(),
                msg.glyph.clone(),
                msg.text.clone(),
                body,
                Local::now().naive_local(),
                msg.warning,
                true,
            );
        }
        if self.last_idle_secs >= 60.0 {
            let text = match &msg.value {
                Some(v) => format!("{} · {v}", msg.text),
                None => msg.text.clone(),
            };
            self.away_items.push(AwayItem {
                glyph: msg.glyph.clone(),
                text,
                at: Local::now().naive_local(),
            });
        }
        let can = self.can_show_hud();
        if let Some(now_msg) = self.hud_queue.offer(msg, can) {
            self.display_hud(now_msg, factories);
        }
    }

    pub fn display_hud(&mut self, msg: HudMessage, factories: Option<&D2dFactories>) {
        self.hud_deadline =
            Some(Instant::now() + Duration::from_millis(u64::from(msg.duration_ms)));
        if msg.chime {
            let key = msg.tag.clone().unwrap_or_else(|| msg.text.clone());
            if self.chimed_tags.insert(key) && self.chime_allowed() {
                Chime::play();
            }
        }
        let caw = msg.important && !msg.celebrate && msg.kind == HudKind::Reminder;
        let raven = msg.celebrate || caw;
        self.hud_raven_land_at = None;
        if msg.celebrate && msg.glyph == Glyphs::WATER {
            self.hud_raven.shake();
        } else if msg.celebrate {
            self.hud_raven.fly();
            self.hud_raven_land_at = Some(Instant::now() + Duration::from_millis(1100));
        } else if caw {
            self.hud_raven.caw();
        } else {
            self.hud_raven.stop();
        }
        if let Some(lvl) = msg.level {
            self.hud_bar_target = lvl.clamp(0.0, 1.0) * 96.0;
        }
        self.hud_width = self.measure_hud(&msg, raven, factories) + 2.0 * HUD_PADDING;
        self.hud_active = true;
        self.update_targets(factories);
    }

    fn measure_hud(&self, msg: &HudMessage, raven: bool, factories: Option<&D2dFactories>) -> f64 {
        let mut w = if raven { 26.4 } else { 15.0 };
        let text_w = if let Some(f) = factories {
            f64::from(
                f.measure_text(&msg.text, FontKind::Ui, 13.0, true, false, 320.0, false)
                    .0,
            )
            .min(320.0)
        } else {
            (msg.text.chars().count() as f64 * 7.5).min(320.0)
        };
        w += 10.0 + text_w;
        if msg.level.is_some() {
            w += 14.0 + 96.0;
        }
        if let Some(ref val) = msg.value {
            let vw = if let Some(f) = factories {
                f64::from(
                    f.measure_text(val, FontKind::Ui, 12.0, false, true, 200.0, false)
                        .0,
                )
                .max(22.0)
            } else {
                (val.chars().count() as f64 * 7.0).max(22.0)
            };
            w += 10.0 + vw;
        }
        w
    }

    pub fn hide_hud(&mut self, factories: Option<&D2dFactories>) {
        self.hud_deadline = None;
        if let Some(next) = self.hud_queue.finish() {
            self.display_hud(next, factories);
            return;
        }
        self.hud_active = false;
        self.hud_raven_land_at = None;
        self.hud_raven.stop();
        self.update_targets(factories);
    }

    pub fn interrupt_hud(&mut self) {
        self.hud_queue.interrupt();
        self.hud_deadline = None;
        self.hud_active = false;
        self.hud_raven_land_at = None;
        self.hud_raven.stop();
    }

    pub fn show_next_hud(&mut self, factories: Option<&D2dFactories>) {
        if self.can_show_hud()
            && let Some(next) = self.hud_queue.resume()
        {
            self.display_hud(next, factories);
        }
    }

    pub fn show_news(&mut self, report: AwayReport, factories: Option<&D2dFactories>) {
        self.interrupt_hud();
        self.news_report = Some(report);
        self.news_active = true;
        self.news_deadline = Some(Instant::now() + Duration::from_secs(9));
        self.news_flight_t = if self.reduce_motion { 1.0 } else { 0.0 };
        self.news_raven.letter = true;
        self.news_raven.facing_left = true;
        if self.reduce_motion {
            self.news_raven.perch();
        } else {
            self.news_raven.fly();
        }
        self.update_targets(factories);
    }

    pub fn hide_news(&mut self, factories: Option<&D2dFactories>) {
        if !self.news_active {
            return;
        }
        self.news_active = false;
        self.news_deadline = None;
        self.news_raven.stop();
        self.news_raven.letter = false;
        self.update_targets(factories);
        self.show_next_hud(factories);
    }

    pub fn handle_audio_event(&mut self, ev: AudioEvent, factories: Option<&D2dFactories>) {
        match ev {
            AudioEvent::Hud(m) => self.show_hud(m, factories),
            AudioEvent::MicMuteChanged(muted) => {
                self.mic_muted = muted;
                let s = self.lang.strings();
                let mut m = HudMessage::new(
                    if muted {
                        Glyphs::MICROPHONE_OFF
                    } else {
                        Glyphs::MICROPHONE
                    },
                    if muted {
                        s.hud_microphone_muted
                    } else {
                        s.hud_microphone_on
                    },
                );
                m.kind = HudKind::Privacy;
                m.tag = Some("microphone-mute".to_string());
                self.show_hud(m, factories);
                self.update_targets(factories);
            }
            AudioEvent::MicAppsChanged(apps) => {
                let added: Vec<String> = apps
                    .iter()
                    .filter(|a| !self.mic_apps.contains(a))
                    .cloned()
                    .collect();
                self.mic_apps = apps;
                if !added.is_empty() {
                    let mut m =
                        HudMessage::new(Glyphs::MICROPHONE, self.lang.strings().hud_microphone);
                    m.value = Some(added.join(", "));
                    m.duration_ms = 3000;
                    m.kind = HudKind::Privacy;
                    self.show_hud(m, factories);
                }
                self.update_targets(factories);
            }
        }
    }

    pub fn handle_camera_apps(&mut self, apps: Vec<String>, factories: Option<&D2dFactories>) {
        let added: Vec<String> = apps
            .iter()
            .filter(|a| !self.camera_apps.contains(a))
            .cloned()
            .collect();
        self.camera_apps = apps;
        if !added.is_empty() {
            let mut m = HudMessage::new(Glyphs::CAMERA, self.lang.strings().hud_camera);
            m.value = Some(added.join(", "));
            m.duration_ms = 3000;
            m.kind = HudKind::Privacy;
            self.show_hud(m, factories);
        }
        self.update_targets(factories);
    }
}
