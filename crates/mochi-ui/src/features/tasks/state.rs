use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDateTime, Timelike};

use mochi_winrt::mochi_tasks;
use mochi_winrt::mochi_tasks::port::{LocalTaskInput, RemoteTaskInput, parse_dt};
use mochi_winrt::paths::TaskData;
use mochi_winrt::tasks::{RemoteTask, TaskListInfo};

use crate::features::shelf::state::{UndoKind, UndoState};
use crate::state::NotchApp;

static NEXT_TASK_SEQ: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_task_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let seq = NEXT_TASK_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:020x}-{seq:08x}")
}

impl NotchApp {
    pub fn add_task(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        let parsed = mochi_tasks::port::parse_task_input(trimmed, Local::now().naive_local());
        let text = if parsed.text.is_empty() {
            trimmed.to_string()
        } else {
            parsed.text
        };
        let due = parsed
            .due
            .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string());
        let idx = self.store.data.tasks.iter().take_while(|t| !t.done).count();
        let id = next_task_id();
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

    pub fn apply_remote_tasks_snapshot(
        &mut self,
        lists: Vec<TaskListInfo>,
        lid: String,
        remote: &[RemoteTask],
    ) {
        self.task_lists = lists;
        self.store.data.settings.tasks_list_id = Some(lid);
        let local_inputs: Vec<LocalTaskInput> = self
            .store
            .data
            .tasks
            .iter()
            .map(|t| LocalTaskInput {
                id: t.id.clone(),
                text: t.text.clone(),
                done: t.done,
                due: t.due.as_deref().and_then(parse_dt),
                due_all_day: t.due_all_day,
                reminded: t.reminded,
                google_id: t.google_id.clone(),
                dirty: t.dirty,
            })
            .collect();
        let remote_inputs: Vec<RemoteTaskInput> = remote
            .iter()
            .map(|r| RemoteTaskInput {
                id: r.id.clone(),
                title: r.title.clone(),
                done: r.done,
                due_date: r.due_date,
            })
            .collect();
        let merged = mochi_tasks::port::apply_remote_tasks(
            local_inputs,
            &remote_inputs,
            &self.store.data.task_deletes,
        );
        self.store.data.tasks = merged
            .into_iter()
            .map(|m| TaskData {
                id: m.id.unwrap_or_else(next_task_id),
                text: m.text,
                done: m.done,
                due: m.due.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                due_all_day: m.due_all_day,
                reminded: m.reminded,
                google_id: m.google_id,
                dirty: m.dirty,
            })
            .collect();
        self.tasks_last_synced = Some(Local::now().naive_local());
        self.tasks_error = None;
        self.dirty = true;
    }
}
