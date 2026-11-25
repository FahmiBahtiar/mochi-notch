//! Pure task list, natural time parser, Google Tasks merge, and reminder formatting.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use regex::Regex;
use serde_json::{Value, json};

static TRAILING_TIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(.*?)\s+(?:(?:(?:at|pukul|jam)\s+)?(\d{1,2})(?:(?::|h)(\d{2})|(?::(\d{2}))?\s?([ap])\.?m\.?)|(?:pukul|jam)\s+(\d{1,2})(?:\.(\d{2}))?h?)$",
    )
    .expect("valid regex")
});

fn parse_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .ok()
}

fn format_dt_o(dt: NaiveDateTime) -> String {
    let base = dt.format("%Y-%m-%dT%H:%M:%S");
    let ticks = dt.and_utc().timestamp_subsec_nanos() / 100;
    format!("{base}.{ticks:07}")
}

#[derive(Debug, Clone)]
struct TaskItem {
    text: String,
    done: bool,
    due: Option<NaiveDateTime>,
    due_all_day: bool,
    google_id: Option<String>,
    dirty: bool,
}

impl TaskItem {
    fn to_json(&self) -> Value {
        json!({
            "Text": self.text,
            "Done": self.done,
            "DueIso": self.due.map(format_dt_o),
            "DueAllDay": self.due_all_day,
            "GoogleId": self.google_id,
            "Dirty": self.dirty,
        })
    }
}

fn time_of_day(caps: &regex::Captures<'_>) -> Option<NaiveTime> {
    if let Some(m6) = caps.get(6) {
        let whole: u32 = m6.as_str().parse().ok()?;
        let minute: u32 = match caps.get(7) {
            Some(m7) => m7.as_str().parse().ok()?,
            None => 0,
        };
        return if whole < 24 && minute < 60 {
            NaiveTime::from_hms_opt(whole, minute, 0)
        } else {
            None
        };
    }
    let hour: u32 = caps.get(2)?.as_str().parse().ok()?;
    let Some(m5) = caps.get(5) else {
        let minute: u32 = caps.get(3)?.as_str().parse().ok()?;
        return if hour < 24 && minute < 60 {
            NaiveTime::from_hms_opt(hour, minute, 0)
        } else {
            None
        };
    };
    let minutes: u32 = match caps.get(4) {
        Some(m4) => m4.as_str().parse().ok()?,
        None => 0,
    };
    if !(1..=12).contains(&hour) || minutes >= 60 {
        return None;
    }
    let pm = m5.as_str().as_bytes()[0].eq_ignore_ascii_case(&b'p');
    let h24 = (hour % 12) + if pm { 12 } else { 0 };
    NaiveTime::from_hms_opt(h24, minutes, 0)
}

/// Port of `SimulateTaskList` (`tasks_core/TasksCoreUnit.cs:34`).
pub fn simulate_task_list(steps: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
    let now = NaiveDate::from_ymd_opt(2026, 10, 26)
        .unwrap()
        .and_hms_opt(14, 0, 0)
        .unwrap();
    let mut items: Vec<TaskItem> = Vec::new();

    if let Some(steps) = steps {
        for step in steps {
            let op = step.get("Op").and_then(Value::as_str).unwrap_or("");
            let idx = step.get("Index").and_then(Value::as_i64).unwrap_or(-1);
            match op {
                "add" => {
                    let Some(raw) = step.get("Text").and_then(Value::as_str) else {
                        continue;
                    };
                    let mut text = raw.trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    let mut due = None;
                    if let Some(caps) = TRAILING_TIME_RE.captures(&text)
                        && let Some(time) = time_of_day(&caps)
                    {
                        text = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
                        let at = now.date().and_time(time);
                        due = Some(if at > now { at } else { at + Duration::days(1) });
                    }
                    let insert_at = items.iter().take_while(|t| !t.done).count();
                    items.insert(
                        insert_at,
                        TaskItem {
                            text,
                            done: false,
                            due,
                            due_all_day: false,
                            google_id: None,
                            dirty: false,
                        },
                    );
                }
                "toggle" if idx >= 0 && (idx as usize) < items.len() => {
                    let mut item = items.remove(idx as usize);
                    item.done = !item.done;
                    if item.google_id.is_some() {
                        item.dirty = true;
                    }
                    let insert_at = if item.done {
                        items.len()
                    } else {
                        items.iter().take_while(|t| !t.done).count()
                    };
                    items.insert(insert_at, item);
                }
                "remove" if idx >= 0 && (idx as usize) < items.len() => {
                    items.remove(idx as usize);
                }
                _ => {}
            }
        }
    }

    Ok(items.iter().map(TaskItem::to_json).collect())
}

fn set_due_from_google(item: &mut TaskItem, date: Option<NaiveDateTime>) {
    match date {
        None => {
            item.due = None;
            item.due_all_day = false;
        }
        Some(day) => {
            if item.due.map(|d| d.date()) != Some(day.date()) {
                item.due = Some(day.date().and_hms_opt(0, 0, 0).unwrap());
                item.due_all_day = true;
            }
        }
    }
}

/// Port of `ApplyRemoteTasks` (`tasks_core/TasksCoreUnit.cs:65`).
pub fn apply_remote_tasks(
    local: Option<Vec<Value>>,
    remote: Option<Vec<Value>>,
    pending_deletes: Option<Vec<String>>,
) -> Result<Vec<Value>, String> {
    let pending: HashSet<String> = pending_deletes.unwrap_or_default().into_iter().collect();
    let mut items: Vec<TaskItem> = local
        .unwrap_or_default()
        .into_iter()
        .map(|v| TaskItem {
            text: v
                .get("Text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            done: v.get("Done").and_then(Value::as_bool).unwrap_or(false),
            due: v.get("DueIso").and_then(Value::as_str).and_then(parse_dt),
            due_all_day: v.get("DueAllDay").and_then(Value::as_bool).unwrap_or(false),
            google_id: v
                .get("GoogleId")
                .and_then(Value::as_str)
                .map(str::to_string),
            dirty: v.get("Dirty").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect();

    struct RemoteTask {
        id: String,
        title: String,
        done: bool,
        due_date: Option<NaiveDateTime>,
    }

    let remote_list: Vec<RemoteTask> = remote
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            Some(RemoteTask {
                id: v.get("Id")?.as_str()?.to_string(),
                title: v
                    .get("Title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                done: v.get("Done").and_then(Value::as_bool).unwrap_or(false),
                due_date: v
                    .get("DueDateIso")
                    .and_then(Value::as_str)
                    .and_then(parse_dt),
            })
        })
        .collect();

    let mut by_id: HashMap<String, &RemoteTask> = HashMap::new();
    for r in &remote_list {
        by_id.entry(r.id.clone()).or_insert(r);
    }

    let mut known: HashSet<String> = HashSet::new();
    let mut kept: Vec<TaskItem> = Vec::with_capacity(items.len());
    for mut item in items.drain(..) {
        let Some(gid) = item.google_id.clone() else {
            kept.push(item);
            continue;
        };
        known.insert(gid.clone());
        if item.dirty {
            kept.push(item);
            continue;
        }
        let Some(rt) = by_id.get(&gid) else {
            continue;
        };
        item.text = rt.title.clone();
        item.done = rt.done;
        set_due_from_google(&mut item, rt.due_date);
        kept.push(item);
    }

    for rt in &remote_list {
        if known.contains(&rt.id) || pending.contains(&rt.id) {
            continue;
        }
        known.insert(rt.id.clone());
        let mut item = TaskItem {
            text: rt.title.clone(),
            done: rt.done,
            due: None,
            due_all_day: false,
            google_id: Some(rt.id.clone()),
            dirty: false,
        };
        set_due_from_google(&mut item, rt.due_date);
        kept.push(item);
    }

    let mut ordered: Vec<TaskItem> = kept.iter().filter(|t| !t.done).cloned().collect();
    ordered.extend(kept.into_iter().filter(|t| t.done));
    Ok(ordered.iter().map(TaskItem::to_json).collect())
}

/// Port of `TaskReminderValue` (`tasks_core/TasksCoreUnit.cs:105`).
pub fn task_reminder_value(due_iso: String, now_iso: String) -> Result<Option<String>, String> {
    let (Some(due), Some(now)) = (parse_dt(&due_iso), parse_dt(&now_iso)) else {
        return Ok(None);
    };
    let time = format!("{:02}:{:02}", due.hour(), due.minute());
    let late = now - due;
    if late < Duration::minutes(5) {
        return Ok(Some(time));
    }
    let by = if late < Duration::hours(1) {
        format!("{} min", late.num_minutes())
    } else {
        format!("{} h", late.num_hours())
    };
    Ok(Some(format!("{time} · {by} late")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_time_parses_id_and_en() {
        let res = simulate_task_list(Some(vec![
            json!({"Op": "add", "Text": "Rapat pukul 15:30", "Index": 0}),
            json!({"Op": "add", "Text": "Call Ana at 3pm", "Index": 0}),
            json!({"Op": "add", "Text": "Rapat jam 9.30", "Index": 0}),
            json!({"Op": "add", "Text": "belajar 2h", "Index": 0}),
        ]))
        .unwrap();
        assert_eq!(res[0]["Text"], "Rapat");
        assert!(res[0]["DueIso"].is_string());
        assert_eq!(res[1]["Text"], "Call Ana");
        assert!(res[1]["DueIso"].is_string());
        assert_eq!(res[2]["Text"], "Rapat");
        assert!(res[2]["DueIso"].is_string());
        assert_eq!(res[3]["Text"], "belajar 2h");
        assert!(res[3]["DueIso"].is_null());
    }
}
