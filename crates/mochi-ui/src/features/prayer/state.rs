use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime};

use mochi_winrt::mochi_prayer::port::{PrayerMethod, build_prayer_schedule, format_hm};
use mochi_winrt::paths::{HudKind, HudMessage};
use mochi_winrt::strings::{fmt1, fmt2};
use mochi_winrt::weather::search_weather_places;

use crate::d2d::D2dFactories;
use crate::state::NotchApp;

impl NotchApp {
    pub fn pray_coords_and_place(&self) -> (f64, f64, String) {
        let s = &self.store.data.settings;
        if let (Some(lat), Some(lon)) = (s.pray_latitude, s.pray_longitude) {
            let place = s
                .pray_place
                .clone()
                .unwrap_or_else(|| format!("{lat:.2}, {lon:.2}"));
            return (lat, lon, place);
        }
        if let (Some(lat), Some(lon)) = (s.weather_latitude, s.weather_longitude) {
            let place = s
                .weather_place
                .clone()
                .unwrap_or_else(|| format!("{lat:.2}, {lon:.2}"));
            return (lat, lon, place);
        }
        (-6.2088, 106.8456, "Jakarta".to_string())
    }

    pub fn recompute_prayer_schedule(&mut self) {
        let now = Local::now().naive_local();
        let (lat, lon, _) = self.pray_coords_and_place();
        let method = PrayerMethod::from_str_lossy(&self.store.data.settings.pray_method);
        self.pray_schedule = Some(build_prayer_schedule(now, lat, lon, method));
        self.dirty = true;
    }

    pub fn is_prayer_muted(&self, name: &str) -> bool {
        self.store
            .data
            .settings
            .pray_muted
            .iter()
            .any(|m| m.eq_ignore_ascii_case(name))
    }

    pub fn toggle_pray_alerts(&mut self) {
        self.store.data.settings.pray_alerts_enabled =
            !self.store.data.settings.pray_alerts_enabled;
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn toggle_pray_pre_alert(&mut self) {
        self.store.data.settings.pray_pre_alert_minutes =
            if self.store.data.settings.pray_pre_alert_minutes > 0 {
                0
            } else {
                10
            };
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn cycle_pray_method(&mut self) {
        let cur = PrayerMethod::from_str_lossy(&self.store.data.settings.pray_method);
        self.store.data.settings.pray_method = cur.next().as_key().to_string();
        self.recompute_prayer_schedule();
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
    }

    pub fn toggle_pray_mute(&mut self, idx: usize) {
        let Some(ref sched) = self.pray_schedule else {
            return;
        };
        let Some(entry) = sched.prayers.get(idx) else {
            return;
        };
        let key = entry.name.clone();
        if let Some(pos) = self
            .store
            .data
            .settings
            .pray_muted
            .iter()
            .position(|m| m.eq_ignore_ascii_case(&key))
        {
            self.store.data.settings.pray_muted.remove(pos);
        } else {
            self.store.data.settings.pray_muted.push(key);
        }
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn search_pray_places(&mut self, query: &str) {
        self.pray_places = search_weather_places(query, self.lang);
        self.dirty = true;
    }

    pub fn check_prayer_notifications(
        &mut self,
        now: NaiveDateTime,
        factories: Option<&D2dFactories>,
    ) {
        let (lat, lon, place) = self.pray_coords_and_place();
        let method = PrayerMethod::from_str_lossy(&self.store.data.settings.pray_method);
        let needs_rebuild = self
            .pray_schedule
            .as_ref()
            .is_none_or(|s| s.date != now.date() || now >= s.next_time);
        if needs_rebuild {
            self.pray_schedule = Some(build_prayer_schedule(now, lat, lon, method));
        }
        if !self.store.data.settings.pray_alerts_enabled {
            return;
        }
        let Some(sched) = self.pray_schedule.clone() else {
            return;
        };
        let pre_mins = i64::from(self.store.data.settings.pray_pre_alert_minutes.max(0));
        let s = self.lang.strings();
        let date_key = now.date().format("%Y-%m-%d").to_string();

        for entry in &sched.prayers {
            if self.is_prayer_muted(&entry.name) {
                continue;
            }
            let diff_secs = (now - entry.time).num_seconds();
            if (0..90).contains(&diff_secs) {
                let tag = format!("{date_key}:{}:exact", entry.name);
                if self.pray_sent_tags.insert(tag) {
                    let headline = fmt1(s.pray_time_now, &entry.name);
                    let hm = format_hm(entry.time);
                    let mut hud = HudMessage::new("\u{E708}", headline);
                    hud.value = Some(format!("{place} · {hm}"));
                    hud.duration_ms = 6000;
                    hud.important = true;
                    hud.chime = true;
                    hud.kind = HudKind::Reminder;
                    hud.tag = Some(format!("pray-{}", entry.name));
                    self.show_hud(hud, factories);
                }
            } else if pre_mins > 0 {
                let pre_time = entry.time - chrono::Duration::minutes(pre_mins);
                let pre_diff = (now - pre_time).num_seconds();
                if (0..60).contains(&pre_diff) {
                    let tag = format!("{date_key}:{}:pre", entry.name);
                    if self.pray_sent_tags.insert(tag) {
                        let headline = fmt2(s.pray_time_soon, &entry.name, &pre_mins.to_string());
                        let hm = format_hm(entry.time);
                        let mut hud = HudMessage::new("\u{E708}", headline);
                        hud.value = Some(format!("{place} · {hm}"));
                        hud.duration_ms = 4500;
                        hud.chime = true;
                        hud.kind = HudKind::Reminder;
                        hud.tag = Some(format!("pray-pre-{}", entry.name));
                        self.show_hud(hud, factories);
                    }
                }
            }
        }
    }
}
