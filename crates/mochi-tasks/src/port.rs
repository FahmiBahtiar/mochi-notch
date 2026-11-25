//! Pure task list, natural time parser, and Google Tasks merge logic.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;

static TRAILING_TIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(.*?)\s+(?:(?:(?:at|pukul|jam)\s+)?(\d{1,2})(?:(?::|h)(\d{2})|(?::(\d{2}))?\s?([ap])\.?m\.?)|(?:pukul|jam)\s+(\d{1,2})(?:\.(\d{2}))?h?)$",
    )
    .expect("valid regex")
});

pub fn parse_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTaskInput {
    pub text: String,
    pub due: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTaskInput {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub due: Option<NaiveDateTime>,
    pub due_all_day: bool,
    pub reminded: bool,
    pub google_id: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTaskInput {
    pub id: String,
    pub title: String,
    pub done: bool,
    pub due_date: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedTask {
    pub id: Option<String>,
    pub text: String,
    pub done: bool,
    pub due: Option<NaiveDateTime>,
    pub due_all_day: bool,
    pub reminded: bool,
    pub google_id: Option<String>,
    pub dirty: bool,
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

pub fn parse_task_input(raw: &str, now: NaiveDateTime) -> ParsedTaskInput {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ParsedTaskInput {
            text: String::new(),
            due: None,
        };
    }
    if let Some(caps) = TRAILING_TIME_RE.captures(trimmed)
        && let Some(time) = time_of_day(&caps)
    {
        let text = caps.get(1).map_or("", |m| m.as_str()).trim().to_string();
        let at = now.date().and_time(time);
        let due = Some(if at > now { at } else { at + Duration::days(1) });
        return ParsedTaskInput { text, due };
    }
    ParsedTaskInput {
        text: trimmed.to_string(),
        due: None,
    }
}

fn set_due_from_google(item: &mut MergedTask, date: Option<NaiveDateTime>) {
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

pub fn apply_remote_tasks(
    local: Vec<LocalTaskInput>,
    remote: &[RemoteTaskInput],
    pending_deletes: &[String],
) -> Vec<MergedTask> {
    let pending: HashSet<&str> = pending_deletes.iter().map(String::as_str).collect();
    let mut by_id: HashMap<&str, &RemoteTaskInput> = HashMap::new();
    for r in remote {
        by_id.entry(r.id.as_str()).or_insert(r);
    }

    let mut known: HashSet<String> = HashSet::new();
    let mut kept: Vec<MergedTask> = Vec::with_capacity(local.len());
    for item in local {
        let mut merged = MergedTask {
            id: Some(item.id),
            text: item.text,
            done: item.done,
            due: item.due,
            due_all_day: item.due_all_day,
            reminded: item.reminded,
            google_id: item.google_id,
            dirty: item.dirty,
        };
        let Some(gid) = merged.google_id.clone() else {
            kept.push(merged);
            continue;
        };
        known.insert(gid.clone());
        if merged.dirty {
            kept.push(merged);
            continue;
        }
        let Some(rt) = by_id.get(gid.as_str()) else {
            continue;
        };
        merged.text = rt.title.clone();
        merged.done = rt.done;
        set_due_from_google(&mut merged, rt.due_date);
        kept.push(merged);
    }

    for rt in remote {
        if known.contains(&rt.id) || pending.contains(rt.id.as_str()) {
            continue;
        }
        known.insert(rt.id.clone());
        let mut merged = MergedTask {
            id: None,
            text: rt.title.clone(),
            done: rt.done,
            due: None,
            due_all_day: false,
            reminded: false,
            google_id: Some(rt.id.clone()),
            dirty: false,
        };
        set_due_from_google(&mut merged, rt.due_date);
        kept.push(merged);
    }

    let mut ordered: Vec<MergedTask> = kept.iter().filter(|t| !t.done).cloned().collect();
    ordered.extend(kept.into_iter().filter(|t| t.done));
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_time_parses_id_and_en() {
        let now = NaiveDate::from_ymd_opt(2026, 10, 26)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        let r1 = parse_task_input("Rapat pukul 15:30", now);
        assert_eq!(r1.text, "Rapat");
        assert!(r1.due.is_some());

        let r2 = parse_task_input("Call Ana at 3pm", now);
        assert_eq!(r2.text, "Call Ana");
        assert!(r2.due.is_some());

        let r3 = parse_task_input("Rapat jam 9.30", now);
        assert_eq!(r3.text, "Rapat");
        assert!(r3.due.is_some());

        let r4 = parse_task_input("belajar 2h", now);
        assert_eq!(r4.text, "belajar 2h");
        assert!(r4.due.is_none());
    }

    #[test]
    fn parse_task_input_uses_supplied_now() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 25)
            .unwrap()
            .and_hms_opt(14, 0, 0)
            .unwrap();
        let later_today = parse_task_input("Call Ana at 3pm", now);
        assert_eq!(later_today.text, "Call Ana");
        assert_eq!(
            later_today.due,
            Some(
                NaiveDate::from_ymd_opt(2026, 11, 25)
                    .unwrap()
                    .and_hms_opt(15, 0, 0)
                    .unwrap()
            )
        );
        let tomorrow = parse_task_input("Standup at 9am", now);
        assert_eq!(tomorrow.text, "Standup");
        assert_eq!(
            tomorrow.due,
            Some(
                NaiveDate::from_ymd_opt(2026, 11, 26)
                    .unwrap()
                    .and_hms_opt(9, 0, 0)
                    .unwrap()
            )
        );
    }

    #[test]
    fn apply_remote_tasks_preserves_local_ids_and_skips_pending_deletes() {
        let local = vec![
            LocalTaskInput {
                id: "local-1".into(),
                text: "Local only".into(),
                done: false,
                due: None,
                due_all_day: false,
                reminded: false,
                google_id: None,
                dirty: false,
            },
            LocalTaskInput {
                id: "synced-1".into(),
                text: "Old title".into(),
                done: false,
                due: None,
                due_all_day: false,
                reminded: true,
                google_id: Some("g-1".into()),
                dirty: false,
            },
        ];
        let remote = vec![
            RemoteTaskInput {
                id: "g-1".into(),
                title: "New title".into(),
                done: false,
                due_date: None,
            },
            RemoteTaskInput {
                id: "g-del".into(),
                title: "Deleted locally".into(),
                done: false,
                due_date: None,
            },
            RemoteTaskInput {
                id: "g-new".into(),
                title: "Brand new".into(),
                done: false,
                due_date: None,
            },
        ];
        let merged = apply_remote_tasks(local, &remote, &["g-del".into()]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].id.as_deref(), Some("local-1"));
        assert_eq!(merged[1].id.as_deref(), Some("synced-1"));
        assert_eq!(merged[1].text, "New title");
        assert!(merged[1].reminded);
        assert_eq!(merged[2].id, None);
        assert_eq!(merged[2].google_id.as_deref(), Some("g-new"));
    }
}
