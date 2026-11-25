use std::collections::HashSet;
use std::sync::Mutex;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
pub use mochi_calendar::port::CalendarEvent;
use serde_json::Value;

use crate::google::GoogleAuth;
use crate::http::url_encode;

#[derive(Debug, Clone)]
pub struct CalendarInfo {
    pub id: String,
    pub name: String,
    pub selected_in_google: bool,
    pub primary: bool,
}

pub fn fetch_calendars_and_events(
    auth: &Mutex<GoogleAuth>,
    included_ids: Option<&[String]>,
) -> Result<(Vec<CalendarInfo>, Vec<CalendarEvent>), String> {
    let resp = GoogleAuth::send_api_shared(
        auth,
        "GET",
        "https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=250&fields=items(id,summary,summaryOverride,selected,primary)",
        None,
    )?;
    if !(200..300).contains(&resp.status) {
        return Err(format!("HTTP {}", resp.status));
    }
    let doc: Value = serde_json::from_str(&resp.body).map_err(|e| e.to_string())?;
    let mut calendars = Vec::new();
    if let Some(items) = doc.get("items").and_then(Value::as_array) {
        for item in items {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            let name = item
                .get("summaryOverride")
                .and_then(Value::as_str)
                .or_else(|| item.get("summary").and_then(Value::as_str))
                .unwrap_or(id)
                .to_string();
            calendars.push(CalendarInfo {
                id: id.to_string(),
                name,
                selected_in_google: item
                    .get("selected")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                primary: item
                    .get("primary")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
    }
    calendars.sort_by(|a, b| b.primary.cmp(&a.primary).then_with(|| a.name.cmp(&b.name)));

    let today = Local::now().date_naive();
    let from = today.and_hms_opt(0, 0, 0).unwrap();
    let to = (today + chrono::Duration::days(2))
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let time_min = Local::now().format("%Y-%m-%dT00:00:00%:z").to_string();
    let time_max = (Local::now() + chrono::Duration::days(2))
        .format("%Y-%m-%dT00:00:00%:z")
        .to_string();
    let _ = (from, to);

    let mut events = Vec::new();
    let mut seen_keys = HashSet::new();
    for cal in &calendars {
        let inc = match included_ids {
            Some(ids) => ids.iter().any(|x| x == &cal.id),
            None => cal.selected_in_google || cal.primary,
        };
        if !inc {
            continue;
        }
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events?singleEvents=true&orderBy=startTime&maxResults=100&timeMin={}&timeMax={}&fields=items(id,iCalUID,status,eventType,summary,start,end,hangoutLink,location,description,conferenceData(entryPoints(entryPointType,uri)),attendees(self,responseStatus))",
            url_encode(&cal.id),
            url_encode(&time_min),
            url_encode(&time_max),
        );
        let Ok(ev_resp) = GoogleAuth::send_api_shared(auth, "GET", &url, None) else {
            continue;
        };
        if !(200..300).contains(&ev_resp.status) {
            continue;
        }
        let Ok(ev_doc) = serde_json::from_str::<Value>(&ev_resp.body) else {
            continue;
        };
        if let Some(items) = ev_doc.get("items").and_then(Value::as_array) {
            for item in items {
                if let Some(ev) = mochi_calendar::port::parse_calendar_event(item)
                    && seen_keys.insert(ev.key.clone())
                {
                    events.push(ev);
                }
            }
        }
    }
    events.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.title.cmp(&b.title)));
    Ok((calendars, events))
}

pub(crate) fn parse_iso_naive(s: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_local());
    }
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .ok()
}
