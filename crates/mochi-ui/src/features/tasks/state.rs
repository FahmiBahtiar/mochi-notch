use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime, Timelike};
use serde_json::{Value, json};

use mochi_winrt::google::GoogleAuth;
use mochi_winrt::mochi_tasks;
use mochi_winrt::paths::TaskData;
use mochi_winrt::tasks::{fetch_google_task_lists, fetch_google_tasks};

use crate::features::shelf::state::{UndoKind, UndoState};
use crate::state::NotchApp;

impl NotchApp {
    pub fn add_task(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        let res = mochi_tasks::port::simulate_task_list(Some(vec![json!({
            "Op": "add",
            "Text": trimmed,
            "Index": 0,
        })]));
        let (text, due) = if let Ok(list) = res
            && let Some(first) = list.first()
        {
            let t = first
                .get("Text")
                .and_then(Value::as_str)
                .unwrap_or(trimmed)
                .to_string();
            let d = first
                .get("DueIso")
                .and_then(Value::as_str)
                .map(str::to_string);
            (t, d)
        } else {
            (trimmed.to_string(), None)
        };
        let idx = self.store.data.tasks.iter().take_while(|t| !t.done).count();
        let id = format!(
            "{:032x}",
            Instant::now().elapsed().as_nanos() ^ (self.store.data.tasks.len() as u128 + 1)
        );
        self.store.data.tasks.insert(
            idx,
            TaskData {
                id,
                text,
                done: false,
                due,
                due_all_day: false,
                reminded: false,
                google_id: None,
                dirty: false,
            },
        );
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn toggle_task(&mut self, idx: usize) {
        if idx >= self.store.data.tasks.len() {
            return;
        }
        let mut item = self.store.data.tasks.remove(idx);
        item.done = !item.done;
        if item.google_id.is_some() {
            item.dirty = true;
        }
        let insert_at = if item.done {
            self.store.data.tasks.len()
        } else {
            self.store.data.tasks.iter().take_while(|t| !t.done).count()
        };
        self.store.data.tasks.insert(insert_at, item);
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn remove_task(&mut self, idx: usize) {
        if idx >= self.store.data.tasks.len() {
            return;
        }
        let item = self.store.data.tasks.remove(idx);
        if let Some(ref gid) = item.google_id {
            self.store.data.task_deletes.push(gid.clone());
        }
        self.undo = Some(UndoState {
            message: self.lang.strings().undo_task_removed.to_string(),
            expires_at: Instant::now() + Duration::from_secs(6),
            kind: UndoKind::Task { item, index: idx },
        });
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn format_task_due(&self, task: &TaskData) -> String {
        let Some(ref due_s) = task.due else {
            return String::new();
        };
        let Ok(due) = NaiveDateTime::parse_from_str(due_s, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| NaiveDateTime::parse_from_str(due_s, "%Y-%m-%dT%H:%M:%S"))
        else {
            return String::new();
        };
        let today = Local::now().date_naive();
        let s = self.lang.strings();
        if task.due_all_day {
            if due.date() == today {
                s.due_today.to_string()
            } else if due.date() == today + chrono::Duration::days(1) {
                s.due_tomorrow.to_string()
            } else {
                self.lang.format_day_month(due.date())
            }
        } else if due.date() == today {
            format!("{:02}:{:02}", due.hour(), due.minute())
        } else if due.date() == today + chrono::Duration::days(1) {
            s.due_tomorrow_at.replace(
                "{0:HH:mm}",
                &format!("{:02}:{:02}", due.hour(), due.minute()),
            )
        } else {
            self.lang.format_day_month_time(due)
        }
    }

    pub fn refresh_google_tasks_sync(&mut self) {
        if self.tasks_syncing || !self.store.data.settings.tasks_sync_enabled {
            return;
        }
        self.tasks_syncing = true;
        let auth = Arc::clone(&self.google_auth);
        if let Ok(mut guard) = auth.lock()
            && guard.is_signed_in()
            && guard.has_scopes(&[GoogleAuth::TASKS_SCOPE])
            && let Ok(lists) = fetch_google_task_lists(&mut guard)
        {
            self.task_lists = lists;
            let list_id = self
                .store
                .data
                .settings
                .tasks_list_id
                .clone()
                .or_else(|| self.task_lists.first().map(|l| l.id.clone()));
            if let Some(lid) = list_id {
                self.store.data.settings.tasks_list_id = Some(lid.clone());
                if let Ok(remote) = fetch_google_tasks(&mut guard, &lid) {
                    let local_json: Vec<Value> = self
                        .store
                        .data
                        .tasks
                        .iter()
                        .map(|t| {
                            json!({
                                "Text": t.text,
                                "Done": t.done,
                                "DueIso": t.due,
                                "DueAllDay": t.due_all_day,
                                "GoogleId": t.google_id,
                                "Dirty": t.dirty,
                            })
                        })
                        .collect();
                    let remote_json: Vec<Value> = remote
                        .iter()
                        .map(|r| {
                            json!({
                                "Id": r.id,
                                "Title": r.title,
                                "Done": r.done,
                                "DueDateIso": r.due_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            })
                        })
                        .collect();
                    if let Ok(merged) = mochi_tasks::port::apply_remote_tasks(
                        Some(local_json),
                        Some(remote_json),
                        Some(self.store.data.task_deletes.clone()),
                    ) {
                        self.store.data.tasks = merged
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| TaskData {
                                id: format!("sync-{i}"),
                                text: v
                                    .get("Text")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string(),
                                done: v.get("Done").and_then(Value::as_bool).unwrap_or(false),
                                due: v.get("DueIso").and_then(Value::as_str).map(str::to_string),
                                due_all_day: v
                                    .get("DueAllDay")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                                reminded: false,
                                google_id: v
                                    .get("GoogleId")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                dirty: v.get("Dirty").and_then(Value::as_bool).unwrap_or(false),
                            })
                            .collect();
                        self.tasks_last_synced = Some(Local::now().naive_local());
                        self.tasks_error = None;
                    }
                }
            }
        }
        self.tasks_syncing = false;
    }
}
