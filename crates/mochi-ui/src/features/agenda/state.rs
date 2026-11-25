use std::sync::Arc;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};

use mochi_winrt::calendar::{CalendarEvent, fetch_calendars_and_events};
use mochi_winrt::google::GoogleAuth;
use mochi_winrt::strings::{fmt1, fmt2};

use crate::d2d::{D2dFactories, FontKind};
use crate::state::{
    COMPACT_EVENT, COMPACT_EVENT_TIMER, COMPACT_IDLE, COMPACT_IDLE_TIMER, NotchApp,
};

#[derive(Debug, Clone)]
pub struct AgendaRow {
    pub time: String,
    pub title: String,
    pub detail: String,
    pub meeting_url: Option<String>,
    pub is_now: bool,
    pub is_past: bool,
    pub is_header: bool,
}

impl NotchApp {
    pub(crate) fn event_compact(&self, factories: Option<&D2dFactories>) -> (f64, f64) {
        let mut timer = 0.0;
        if self.focus.is_active() {
            let rem = self.focus.remaining();
            let t_str = format!("{:02}:{:02}", rem.as_secs() / 60, rem.as_secs() % 60);
            let tw = if let Some(f) = factories {
                f64::from(
                    f.measure_text(&t_str, FontKind::Ui, 12.0, true, true, 200.0, false)
                        .0,
                )
            } else {
                36.0
            };
            timer = 12.0 + 5.0 + tw + 8.0;
        }
        let text = self.upcoming_text.as_deref().unwrap_or("");
        let text_w = if let Some(f) = factories {
            f64::from(
                f.measure_text(text, FontKind::Ui, 13.0, true, false, 500.0, false)
                    .0,
            )
        } else {
            text.chars().count() as f64 * 7.5
        };
        let w = (28.0 + text_w + 2.0 * timer).ceil();
        if self.focus.is_active() {
            (
                w.clamp(COMPACT_IDLE_TIMER.0, COMPACT_EVENT_TIMER.0),
                COMPACT_EVENT.1,
            )
        } else {
            (w.clamp(COMPACT_IDLE.0, COMPACT_EVENT.0), COMPACT_EVENT.1)
        }
    }

    pub fn format_until(&self, start: NaiveDateTime, now: NaiveDateTime) -> String {
        let left = start - now;
        if left <= chrono::Duration::zero() {
            return self.lang.strings().calendar_now.to_string();
        }
        let mins = ((left.num_seconds() as f64) / 60.0).ceil() as i64;
        if mins < 60 {
            fmt1(self.lang.strings().calendar_in_minutes, &mins.to_string())
        } else {
            fmt2(
                self.lang.strings().calendar_in_hours,
                &(mins / 60).to_string(),
                &format!("{:02}", mins % 60),
            )
        }
    }

    pub fn build_agenda_rows(&self) -> Vec<AgendaRow> {
        let now = Local::now().naive_local();
        let today = now.date();
        let cutoff = now - chrono::Duration::hours(1);
        let mut visible: Vec<&CalendarEvent> = self
            .calendar_events
            .iter()
            .filter(|e| {
                if e.all_day {
                    e.end > today.and_hms_opt(0, 0, 0).unwrap()
                } else {
                    e.end > cutoff
                }
            })
            .collect();
        visible.sort_by_key(|e| e.start);

        let mut groups: Vec<(NaiveDate, Vec<&CalendarEvent>)> = Vec::new();
        for ev in visible {
            let gday = if ev.all_day || ev.start.date() < today {
                today
            } else {
                ev.start.date()
            };
            if let Some(last) = groups.last_mut()
                && last.0 == gday
            {
                last.1.push(ev);
            } else {
                groups.push((gday, vec![ev]));
            }
        }
        groups.sort_by_key(|g| g.0);

        let s = self.lang.strings();
        let mut rows = Vec::new();
        for (day, mut evs) in groups {
            if day != today {
                let title = if day == today + chrono::Duration::days(1) {
                    s.agenda_tomorrow.to_string()
                } else {
                    self.lang.format_weekday_date(day)
                };
                rows.push(AgendaRow {
                    time: String::new(),
                    title,
                    detail: String::new(),
                    meeting_url: None,
                    is_now: false,
                    is_past: false,
                    is_header: true,
                });
            }
            evs.sort_by_key(|e| (!e.all_day, e.start));
            for e in evs {
                let past = !e.all_day && e.end <= now;
                let ongoing = !e.all_day && e.start <= now && now < e.end;
                let end_hm = format!("{:02}:{:02}", e.end.hour(), e.end.minute());
                let detail = if e.all_day {
                    s.agenda_all_day.to_string()
                } else if ongoing {
                    s.agenda_now_until.replace("{0:HH:mm}", &end_hm)
                } else if past {
                    s.agenda_ended.replace("{0:HH:mm}", &end_hm)
                } else if e.start - now <= chrono::Duration::hours(2) && e.start.date() == today {
                    s.agenda_soon_until
                        .replace("{0}", &self.format_until(e.start, now))
                        .replace("{1:HH:mm}", &end_hm)
                } else {
                    s.agenda_until.replace("{0:HH:mm}", &end_hm)
                };
                rows.push(AgendaRow {
                    time: if e.all_day {
                        "—".to_string()
                    } else {
                        format!("{:02}:{:02}", e.start.hour(), e.start.minute())
                    },
                    title: e.title.clone(),
                    detail,
                    meeting_url: e.meeting_url.clone(),
                    is_now: ongoing,
                    is_past: past,
                    is_header: false,
                });
            }
        }
        rows
    }

    pub fn step_calendar_month(&mut self, delta: i32) {
        let total = self.calendar_view_year * 12 + (self.calendar_view_month as i32 - 1) + delta;
        self.calendar_view_year = total.div_euclid(12);
        self.calendar_view_month = (total.rem_euclid(12) + 1) as u32;
        self.dirty = true;
    }

    pub fn select_calendar_today(&mut self) {
        let today = Local::now().date_naive();
        self.calendar_view_year = today.year();
        self.calendar_view_month = today.month();
        self.calendar_selected_date = today;
        self.agenda_scroll = 0.0;
        self.dirty = true;
    }

    pub fn calendar_has_event_on(&self, date: NaiveDate) -> bool {
        self.calendar_events.iter().any(|e| {
            if e.all_day {
                e.start.date() <= date && e.end.date() > date
            } else {
                e.start.date() <= date && e.end.date() >= date
            }
        })
    }

    pub fn build_agenda_rows_for_selected(&self) -> Vec<AgendaRow> {
        let now = Local::now().naive_local();
        let today = now.date();
        if self.calendar_selected_date == today {
            return self.build_agenda_rows();
        }
        let target = self.calendar_selected_date;
        let mut evs: Vec<&CalendarEvent> = self
            .calendar_events
            .iter()
            .filter(|e| {
                if e.all_day {
                    e.start.date() <= target && e.end.date() > target
                } else {
                    e.start.date() <= target && e.end.date() >= target
                }
            })
            .collect();
        evs.sort_by_key(|e| (!e.all_day, e.start));
        let s = self.lang.strings();
        evs.into_iter()
            .map(|e| {
                let past = !e.all_day && e.end <= now;
                let ongoing = !e.all_day && e.start <= now && now < e.end;
                let end_hm = format!("{:02}:{:02}", e.end.hour(), e.end.minute());
                let detail = if e.all_day {
                    s.agenda_all_day.to_string()
                } else if ongoing {
                    s.agenda_now_until.replace("{0:HH:mm}", &end_hm)
                } else if past {
                    s.agenda_ended.replace("{0:HH:mm}", &end_hm)
                } else {
                    s.agenda_until.replace("{0:HH:mm}", &end_hm)
                };
                AgendaRow {
                    time: if e.all_day {
                        "—".to_string()
                    } else {
                        format!("{:02}:{:02}", e.start.hour(), e.start.minute())
                    },
                    title: e.title.clone(),
                    detail,
                    meeting_url: e.meeting_url.clone(),
                    is_now: ongoing,
                    is_past: past,
                    is_header: false,
                }
            })
            .collect()
    }

    pub fn refresh_google_calendar_sync(&mut self) {
        if self.calendar_fetching {
            return;
        }
        self.calendar_fetching = true;
        let auth = Arc::clone(&self.google_auth);
        let ids = self.store.data.settings.calendar_ids.clone();
        if let Ok(mut guard) = auth.lock()
            && guard.is_signed_in()
            && guard.has_scopes(&[GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE])
        {
            match fetch_calendars_and_events(&mut guard, ids.as_deref()) {
                Ok((cals, evs)) => {
                    self.calendars = cals;
                    self.calendar_events = evs;
                    self.calendar_last_fetched = Some(Local::now().naive_local());
                    self.calendar_error = None;
                }
                Err(e) => {
                    self.calendar_error = Some(e);
                }
            }
        }
        self.calendar_fetching = false;
    }
}
