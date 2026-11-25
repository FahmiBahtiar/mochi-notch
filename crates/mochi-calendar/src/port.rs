//! The Rust port of `calendar/`.

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use percent_encoding::percent_decode_str;
use regex::Regex;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use url::Url;

const MEETING_HOSTS: &[&str] = &[
    "meet.google.com",
    "zoom.us",
    "zoomgov.com",
    "teams.microsoft.com",
    "teams.cloud.microsoft",
    "teams.live.com",
    "webex.com",
    "whereby.com",
    "meet.jit.si",
];

const SAFE_LINKS_HOST: &str = "safelinks.protection.outlook.com";
const UNTITLED: &str = "(No title)";

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)https://[^\s"'<>\\]+"#).expect("valid regex"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DtKind {
    Local,
    Utc,
    Unspecified,
}

#[derive(Debug, Clone, Copy)]
struct DotNetDateTime {
    naive: NaiveDateTime,
    kind: DtKind,
}

impl DotNetDateTime {
    fn format_o(self) -> String {
        let base = self.naive.format("%Y-%m-%dT%H:%M:%S%.f");
        // Ensure exactly 7 fractional digits like .NET "O"
        let s = self.naive.format("%Y-%m-%dT%H:%M:%S").to_string();
        let nanos = self.naive.and_utc().timestamp_subsec_nanos();
        let ticks_frac = nanos / 100;
        let _ = base;
        match self.kind {
            DtKind::Unspecified => format!("{s}.{ticks_frac:07}"),
            DtKind::Utc => format!("{s}.{ticks_frac:07}Z"),
            DtKind::Local => {
                let offset = Local
                    .from_local_datetime(&self.naive)
                    .earliest()
                    .map(|dt| dt.offset().local_minus_utc())
                    .unwrap_or(0);
                let sign = if offset >= 0 { '+' } else { '-' };
                let abs = offset.abs();
                let hh = abs / 3600;
                let mm = (abs % 3600) / 60;
                format!("{s}.{ticks_frac:07}{sign}{hh:02}:{mm:02}")
            }
        }
    }
}

fn parse_dto_to_local(s: &str) -> Option<DotNetDateTime> {
    if let Ok(dt) = DateTime::<FixedOffset>::parse_from_rfc3339(s) {
        let local = dt.with_timezone(&Local);
        return Some(DotNetDateTime {
            naive: local.naive_local(),
            kind: DtKind::Local,
        });
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
    {
        return Some(DotNetDateTime {
            naive,
            kind: DtKind::Local,
        });
    }
    None
}

fn parse_roundtrip_dt(s: &str) -> Option<DotNetDateTime> {
    if s.ends_with('Z') || s.ends_with('z') {
        if let Ok(dt) = DateTime::<FixedOffset>::parse_from_rfc3339(s) {
            return Some(DotNetDateTime {
                naive: dt.with_timezone(&Utc).naive_utc(),
                kind: DtKind::Utc,
            });
        }
    } else if let Ok(dt) = DateTime::<FixedOffset>::parse_from_rfc3339(s) {
        let local = dt.with_timezone(&Local);
        return Some(DotNetDateTime {
            naive: local.naive_local(),
            kind: DtKind::Local,
        });
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
    {
        return Some(DotNetDateTime {
            naive,
            kind: DtKind::Unspecified,
        });
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(DotNetDateTime {
            naive: d.and_hms_opt(0, 0, 0)?,
            kind: DtKind::Unspecified,
        });
    }
    None
}

fn json_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.as_object()?.get(key)?.as_str()
}

fn declined_by_self(item: &Value) -> bool {
    let Some(attendees) = item.get("attendees").and_then(Value::as_array) else {
        return false;
    };
    attendees.iter().any(|a| {
        a.get("self").and_then(Value::as_bool) == Some(true)
            && json_str(a, "responseStatus") == Some("declined")
    })
}

fn safe_url(candidate: Option<&str>, any_host: bool) -> Option<String> {
    let candidate = candidate?;
    let uri = Url::parse(candidate).ok()?;
    if uri.scheme() != "https" {
        return None;
    }
    let host = uri.host_str()?;
    let host_lower = host.to_ascii_lowercase();
    if host_lower.ends_with(&format!(".{SAFE_LINKS_HOST}"))
        && let Some((_, wrapped)) = uri.query_pairs().find(|(k, _)| k == "url")
    {
        return safe_url(Some(wrapped.as_ref()), any_host);
    }
    if !any_host
        && !MEETING_HOSTS
            .iter()
            .any(|h| host_lower == *h || host_lower.ends_with(&format!(".{h}")))
    {
        return None;
    }
    Some(uri.as_str().to_string())
}

fn find_meeting_url_val(item: &Value) -> Option<String> {
    if let Some(entry_points) = item
        .get("conferenceData")
        .and_then(|c| c.get("entryPoints"))
        .and_then(Value::as_array)
    {
        for entry in entry_points {
            if json_str(entry, "entryPointType") == Some("video")
                && let Some(uri) = safe_url(json_str(entry, "uri"), true)
            {
                return Some(uri);
            }
        }
    }

    if let Some(hangout) = safe_url(json_str(item, "hangoutLink"), true) {
        return Some(hangout);
    }

    for field in ["location", "description"] {
        let Some(text) = json_str(item, field) else {
            continue;
        };
        for m in URL_RE.find_iter(text) {
            let trimmed = m.as_str().trim_end_matches(['.', ',', ')', ';']);
            if let Some(url) = safe_url(Some(trimmed), false) {
                return Some(url);
            }
        }
    }
    None
}

/// Port of `ParseCalendarEvent` (`calendar/CalendarUnit.cs:31`).
pub fn parse_calendar_event(json_str_in: String) -> Result<Option<Value>, String> {
    let Ok(item) = serde_json::from_str::<Value>(&json_str_in) else {
        return Ok(None);
    };
    if !item.is_object() {
        return Ok(None);
    }
    if json_str(&item, "status") == Some("cancelled")
        || json_str(&item, "eventType") == Some("workingLocation")
        || declined_by_self(&item)
    {
        return Ok(None);
    }

    let Some(start) = item.get("start") else {
        return Ok(None);
    };
    let Some(end) = item.get("end") else {
        return Ok(None);
    };

    let (from, to, all_day) =
        if let (Some(st), Some(et)) = (json_str(start, "dateTime"), json_str(end, "dateTime")) {
            let (Some(f), Some(t)) = (parse_dto_to_local(st), parse_dto_to_local(et)) else {
                return Ok(None);
            };
            (f, t, false)
        } else if let (Some(sd), Some(ed)) = (json_str(start, "date"), json_str(end, "date")) {
            let (Ok(fd), Ok(ed)) = (
                NaiveDate::parse_from_str(sd, "%Y-%m-%d"),
                NaiveDate::parse_from_str(ed, "%Y-%m-%d"),
            ) else {
                return Ok(None);
            };
            let (Some(fnv), Some(env)) = (fd.and_hms_opt(0, 0, 0), ed.and_hms_opt(0, 0, 0)) else {
                return Ok(None);
            };
            (
                DotNetDateTime {
                    naive: fnv,
                    kind: DtKind::Unspecified,
                },
                DotNetDateTime {
                    naive: env,
                    kind: DtKind::Unspecified,
                },
                true,
            )
        } else {
            return Ok(None);
        };

    let title = json_str(&item, "summary")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(UNTITLED)
        .to_string();
    let id = json_str(&item, "iCalUID")
        .or_else(|| json_str(&item, "id"))
        .unwrap_or(&title)
        .to_string();
    let start_iso = from.format_o();
    let end_iso = to.format_o();
    let key = format!("{id}|{start_iso}");
    let meeting_url = find_meeting_url_val(&item);

    Ok(Some(json!({
        "Id": id,
        "Title": title,
        "StartIso": start_iso,
        "EndIso": end_iso,
        "AllDay": all_day,
        "MeetingUrl": meeting_url,
        "Key": key,
    })))
}

/// Port of `FindMeetingUrl` (`calendar/CalendarUnit.cs:54`).
pub fn find_meeting_url(json_str_in: String) -> Result<Option<String>, String> {
    let Ok(item) = serde_json::from_str::<Value>(&json_str_in) else {
        return Ok(None);
    };
    Ok(find_meeting_url_val(&item))
}

#[derive(Debug, Clone)]
struct ParsedEvent {
    id: String,
    title: String,
    start: DotNetDateTime,
    all_day: bool,
    key: String,
}

fn parse_event_input(v: &Value) -> Option<ParsedEvent> {
    let id = v.get("Id")?.as_str()?.to_string();
    let title = v.get("Title")?.as_str()?.to_string();
    let start = parse_roundtrip_dt(v.get("StartIso")?.as_str()?)?;
    let _end = parse_roundtrip_dt(v.get("EndIso")?.as_str()?)?;
    let all_day = v.get("AllDay")?.as_bool()?;
    let key = format!("{id}|{}", start.format_o());
    Some(ParsedEvent {
        id,
        title,
        start,
        all_day,
        key,
    })
}

/// Port of `CalendarRemindersDue` (`calendar/CalendarUnit.cs:75`).
pub fn calendar_reminders_due(
    events: Option<Vec<Value>>,
    now_iso: String,
    lead_minutes: i64,
    sent_keys: Option<Vec<String>>,
) -> Result<Vec<Value>, String> {
    let Some(events) = events else {
        return Ok(Vec::new());
    };
    let Some(now) = parse_roundtrip_dt(&now_iso) else {
        return Ok(Vec::new());
    };
    let lead = Duration::minutes(lead_minutes);
    let grace = Duration::minutes(2);
    let mut sent: HashSet<String> = sent_keys.unwrap_or_default().into_iter().collect();
    let mut due = Vec::new();

    for ev in events.iter().filter_map(parse_event_input) {
        if ev.all_day {
            continue;
        }
        if lead > Duration::zero()
            && now.naive >= ev.start.naive - lead
            && now.naive < ev.start.naive
            && sent.insert(format!("{}|soon", ev.key))
        {
            due.push(json!({
                "Id": ev.id,
                "Title": ev.title,
                "Starting": false,
                "Key": ev.key,
            }));
        }
        if now.naive >= ev.start.naive
            && now.naive < ev.start.naive + grace
            && sent.insert(format!("{}|start", ev.key))
        {
            due.push(json!({
                "Id": ev.id,
                "Title": ev.title,
                "Starting": true,
                "Key": ev.key,
            }));
        }
    }
    Ok(due)
}

/// Port of `CalendarNextWithin` (`calendar/CalendarUnit.cs:89`).
pub fn calendar_next_within(
    events: Option<Vec<Value>>,
    now_iso: String,
    window_minutes: i64,
) -> Result<Option<String>, String> {
    let Some(events) = events else {
        return Ok(None);
    };
    let Some(now) = parse_roundtrip_dt(&now_iso) else {
        return Ok(None);
    };
    let window = Duration::minutes(window_minutes);
    let best = events
        .iter()
        .filter_map(parse_event_input)
        .filter(|e| !e.all_day && e.start.naive > now.naive && e.start.naive - now.naive <= window)
        .min_by_key(|e| e.start.naive);
    Ok(best.map(|e| e.title))
}

/// Port of `CalendarUntil` (`calendar/CalendarUnit.cs:100`).
pub fn calendar_until(start_iso: String, now_iso: String) -> Result<Option<String>, String> {
    let (Some(start), Some(now)) = (parse_roundtrip_dt(&start_iso), parse_roundtrip_dt(&now_iso))
    else {
        return Ok(None);
    };
    let left = start.naive - now.naive;
    if left <= Duration::zero() {
        return Ok(Some("now".to_string()));
    }
    let total_mins = (left.num_nanoseconds().unwrap_or(0) as f64) / 60_000_000_000.0;
    let minutes = total_mins.ceil() as i64;
    if minutes < 60 {
        Ok(Some(format!("in {minutes} min")))
    } else {
        Ok(Some(format!("in {}h{:02}", minutes / 60, minutes % 60)))
    }
}

/// Port of `PkceChallenge` (`calendar/CalendarUnit.cs:108`).
pub fn pkce_challenge(verifier: String) -> Result<String, String> {
    // .NET Encoding.ASCII.GetBytes maps each UTF-16 code unit > 0x7F to b'?' (0x3F).
    let ascii_bytes: Vec<u8> = verifier
        .encode_utf16()
        .map(|u| if u <= 0x7F { u as u8 } else { b'?' })
        .collect();
    let hash = Sha256::digest(&ascii_bytes);
    Ok(URL_SAFE_NO_PAD.encode(hash))
}

/// Port of `ParseOAuthRedirect` (`calendar/CalendarUnit.cs:110`).
pub fn parse_o_auth_redirect(target: String) -> Result<Option<Value>, String> {
    let q = target.find('?');
    let path = match q {
        Some(idx) => &target[..idx],
        None => &target,
    };
    if path != "/" {
        return Ok(None);
    }
    let mut map = Map::new();
    if let Some(idx) = q {
        for pair in target[idx + 1..].split('&').filter(|s| !s.is_empty()) {
            let eq = pair.find('=');
            let (raw_k, raw_v) = match eq {
                Some(i) => (&pair[..i], &pair[i + 1..]),
                None => (pair, ""),
            };
            let decode = |s: &str| {
                percent_decode_str(&s.replace('+', " "))
                    .decode_utf8_lossy()
                    .into_owned()
            };
            map.insert(decode(raw_k), Value::String(decode(raw_v)));
        }
    }
    Ok(Some(Value::Object(map)))
}

/// Port of `HasOAuthScopes` (`calendar/CalendarUnit.cs:113`).
pub fn has_o_auth_scopes(granted: String, required: Vec<String>) -> Result<bool, String> {
    let set: HashSet<&str> = granted.split(' ').filter(|s| !s.is_empty()).collect();
    Ok(required.iter().all(|r| set.contains(r.as_str())))
}

/// Port of `EmailFromIdToken` (`calendar/CalendarUnit.cs:116`).
pub fn email_from_id_token(id_token: Option<String>) -> Result<Option<String>, String> {
    let Some(token) = id_token else {
        return Ok(None);
    };
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Ok(None);
    }
    let b64 = parts[1].replace('-', "+").replace('_', "/");
    let pad = (4 - (b64.len() % 4)) % 4;
    let padded = format!("{b64}{}", "=".repeat(pad));
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(padded) else {
        return Ok(None);
    };
    let Ok(doc) = serde_json::from_slice::<Value>(&bytes) else {
        return Ok(None);
    };
    Ok(doc.get("email").and_then(Value::as_str).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into()).unwrap(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
