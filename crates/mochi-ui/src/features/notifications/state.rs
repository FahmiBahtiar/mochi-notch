use chrono::NaiveDateTime;

pub use mochi_winrt::mochi_notifications::port::NotificationItem;
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
        if let Some(wid) = win_id
            && !self.seen_win_notif_ids.insert(wid)
        {
            return;
        }
        self.next_notif_id += 1;
        self.notifications.insert(
            0,
            NotificationItem {
                id: self.next_notif_id,
                win_id,
                source,
                glyph,
                title,
                body,
                at,
                unread,
                warning,
            },
        );
        self.notifications.truncate(50);
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
                let is_new = !self.seen_win_notif_ids.contains(&sn.win_id);
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
                self.seen_win_notif_ids.remove(&win_id);
                let before = self.notifications.len();
                self.notifications.retain(|n| n.win_id != Some(win_id));
                if self.notifications.len() != before {
                    self.dirty = true;
                }
            }
        }
    }

    pub fn unread_notifications_count(&self) -> usize {
        self.notifications.iter().filter(|n| n.unread).count()
    }

    pub fn mark_all_notifications_read(&mut self) {
        for n in &mut self.notifications {
            n.unread = false;
        }
        self.dirty = true;
    }

    pub fn dismiss_notification(&mut self, idx: usize) {
        if idx < self.notifications.len() {
            let item = self.notifications.remove(idx);
            if let Some(wid) = item.win_id {
                self.seen_win_notif_ids.remove(&wid);
                remove_windows_notification(wid);
            }
            self.dirty = true;
        }
    }

    pub fn clear_notifications(&mut self) {
        let had_win = self.notifications.iter().any(|n| n.win_id.is_some());
        self.notifications.clear();
        self.seen_win_notif_ids.clear();
        self.notifications_scroll = 0.0;
        if had_win && self.win_notif_access == NotificationAccess::Allowed {
            clear_windows_notifications();
        }
        self.dirty = true;
    }
}
