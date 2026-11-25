//! Pure notification center state, deduplication, and unread tracking.

#![forbid(unsafe_code)]

use std::collections::HashSet;

use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct NotificationItem {
    pub id: u64,
    pub win_id: Option<u32>,
    pub source: String,
    pub glyph: String,
    pub title: String,
    pub body: String,
    pub at: NaiveDateTime,
    pub unread: bool,
    pub warning: bool,
}

#[derive(Debug, Default)]
pub struct NotificationCenter {
    pub items: Vec<NotificationItem>,
    pub seen_win_ids: HashSet<u32>,
    next_id: u64,
}

impl NotificationCenter {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            seen_win_ids: HashSet::new(),
            next_id: 1,
        }
    }

    #[expect(clippy::too_many_arguments, reason = "notification item fields")]
    pub fn push(
        &mut self,
        win_id: Option<u32>,
        source: String,
        glyph: String,
        title: String,
        body: String,
        at: NaiveDateTime,
        warning: bool,
        unread: bool,
    ) -> bool {
        if let Some(wid) = win_id
            && !self.seen_win_ids.insert(wid)
        {
            return false;
        }
        self.next_id += 1;
        self.items.insert(
            0,
            NotificationItem {
                id: self.next_id,
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
        self.items.truncate(50);
        true
    }

    pub fn remove_at(&mut self, idx: usize) -> Option<NotificationItem> {
        if idx >= self.items.len() {
            return None;
        }
        let item = self.items.remove(idx);
        if let Some(wid) = item.win_id {
            self.seen_win_ids.remove(&wid);
        }
        Some(item)
    }

    pub fn remove_by_win_id(&mut self, win_id: u32) -> bool {
        self.seen_win_ids.remove(&win_id);
        let before = self.items.len();
        self.items.retain(|n| n.win_id != Some(win_id));
        self.items.len() != before
    }

    pub fn unread_count(&self) -> usize {
        self.items.iter().filter(|n| n.unread).count()
    }

    pub fn mark_all_read(&mut self) {
        for n in &mut self.items {
            n.unread = false;
        }
    }

    pub fn clear(&mut self) -> bool {
        let had_win = self.items.iter().any(|n| n.win_id.is_some());
        self.items.clear();
        self.seen_win_ids.clear();
        had_win
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn deduplicates_windows_ids_and_tracks_unread() {
        let mut nc = NotificationCenter::new();
        let dt = NaiveDate::from_ymd_opt(2026, 11, 24)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert!(nc.push(
            Some(7),
            "App".into(),
            "B".into(),
            "Hi".into(),
            "Body".into(),
            dt,
            false,
            true
        ));
        assert!(!nc.push(
            Some(7),
            "App".into(),
            "B".into(),
            "Hi".into(),
            "Body".into(),
            dt,
            false,
            true
        ));
        assert_eq!(nc.unread_count(), 1);
        nc.mark_all_read();
        assert_eq!(nc.unread_count(), 0);
        assert!(nc.remove_by_win_id(7));
        assert!(nc.items.is_empty());
    }
}
