use chrono::NaiveDateTime;

pub use mochi_winrt::mochi_notifications::port::{NotificationCenter, NotificationItem};
use mochi_winrt::notifications::{
    NotificationAccess, NotificationEvent, clear_windows_notifications, remove_windows_notification,
};
use mochi_winrt::paths::{Glyphs, HudKind, HudMessage};

use crate::d2d::D2dFactories;
use crate::state::NotchApp;

impl NotchApp {
    #[expect(clippy::too_many_arguments, reason = "records notification item")]
    pub fn push_notification(
        &mut self,
        win_id: Option<u32>,
        source: String,
        glyph: String,
        title: String,
        body: String,
        at: NaiveDateTime,
        warning: bool,
        unread: bool,
    ) {
        if !self
            .notifications
            .push(win_id, source, glyph, title, body, at, warning, unread)
        {
            return;
        }
        if unread && !self.reduce_motion {
            self.notif_pulse_t = 1.0;
        }
        self.dirty = true;
    }

    pub fn handle_notification_event(
        &mut self,
        ev: NotificationEvent,
        factories: Option<&D2dFactories>,
    ) {
        match ev {
            NotificationEvent::Access(acc) => {
                self.win_notif_access = acc;
                self.dirty = true;
            }
            NotificationEvent::Snapshot(list) => {
                for sn in list.into_iter().rev() {
                    self.push_notification(
                        Some(sn.win_id),
                        sn.app_name,
                        Glyphs::BELL.to_string(),
                        sn.title,
                        sn.body,
                        sn.timestamp,
                        false,
                        true,
                    );
                }
            }
            NotificationEvent::Added(sn) => {
                let is_new = !self.notifications.is_seen(sn.win_id);
                self.push_notification(
                    Some(sn.win_id),
                    sn.app_name.clone(),
                    Glyphs::BELL.to_string(),
                    sn.title.clone(),
                    sn.body.clone(),
                    sn.timestamp,
                    false,
                    true,
                );
                if is_new {
                    let headline = if sn.title.eq_ignore_ascii_case(&sn.app_name) {
                        sn.app_name
                    } else {
                        format!("{}: {}", sn.app_name, sn.title)
                    };
                    let mut hud = HudMessage::new(Glyphs::BELL, headline);
                    if !sn.body.is_empty() {
                        hud.value = Some(sn.body);
                    }
                    hud.duration_ms = 4500;
                    hud.kind = HudKind::Reminder;
                    hud.tag = Some("win-notif".to_string());
                    self.show_hud(hud, factories);
                }
            }
            NotificationEvent::Removed(win_id) => {
                if self.notifications.remove_by_win_id(win_id) {
                    self.dirty = true;
                }
            }
        }
    }

    pub fn unread_notifications_count(&self) -> usize {
        self.notifications.unread_count()
    }

    pub fn mark_all_notifications_read(&mut self) {
        self.notifications.mark_all_read();
        self.dirty = true;
    }

    pub fn dismiss_notification(&mut self, idx: usize) {
        if let Some(item) = self.notifications.remove_at(idx) {
            if let Some(wid) = item.win_id {
                remove_windows_notification(wid);
            }
            self.dirty = true;
        }
    }

    pub fn clear_notifications(&mut self) {
        let had_win = self.notifications.clear();
        self.notifications_scroll = 0.0;
        if had_win && self.win_notif_access == NotificationAccess::Allowed {
            clear_windows_notifications();
        }
        self.dirty = true;
    }
}
