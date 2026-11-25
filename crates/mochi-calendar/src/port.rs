//! Pure Google Calendar event parsing, SafeLinks unwrapping, and OAuth helpers.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};
use percent_encoding::percent_decode_str;
use regex::Regex;
use serde_json::Value;
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

#[derive(Debug, Clone)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub all_day: bool,
    pub meeting_url: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DtKind {
    Local,
    Unspecified,
}

#[derive(Debug, Clone, Copy)]
struct DotNetDateTime {
    naive: NaiveDateTime,
    kind: DtKind,
}

impl DotNetDateTime {
    fn format_o(self) -> String {
        let s = self.naive.format("%Y-%m-%dT%H:%M:%S").to_string();
        let nanos = self.naive.and_utc().timestamp_subsec_nanos();
        let ticks_frac = nanos / 100;
        match self.kind {
            DtKind::Unspecified => format!("{s}.{ticks_frac:07}"),
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

pub fn parse_calendar_event(item: &Value) -> Option<CalendarEvent> {
    if !item.is_object() {
        return None;
    }
    if json_str(item, "status") == Some("cancelled")
        || json_str(item, "eventType") == Some("workingLocation")
        || declined_by_self(item)
    {
        return None;
    }

    let start = item.get("start")?;
    let end = item.get("end")?;

    let (from, to, all_day) =
        if let (Some(st), Some(et)) = (json_str(start, "dateTime"), json_str(end, "dateTime")) {
            let (Some(f), Some(t)) = (parse_dto_to_local(st), parse_dto_to_local(et)) else {
                return None;
            };
            (f, t, false)
        } else if let (Some(sd), Some(ed)) = (json_str(start, "date"), json_str(end, "date")) {
            let (Ok(fd), Ok(ed)) = (
                NaiveDate::parse_from_str(sd, "%Y-%m-%d"),
                NaiveDate::parse_from_str(ed, "%Y-%m-%d"),
            ) else {
                return None;
            };
            let (Some(fnv), Some(env)) = (fd.and_hms_opt(0, 0, 0), ed.and_hms_opt(0, 0, 0)) else {
                return None;
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
            return None;
        };

    let title = json_str(item, "summary")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(UNTITLED)
        .to_string();
    let id = json_str(item, "iCalUID")
        .or_else(|| json_str(item, "id"))
        .unwrap_or(&title)
        .to_string();
    let key = format!("{id}|{}", from.format_o());
    let meeting_url = find_meeting_url_val(item);

    Some(CalendarEvent {
        id,
        title,
        start: from.naive,
        end: to.naive,
        all_day,
        meeting_url,
        key,
    })
}

pub fn pkce_challenge(verifier: &str) -> String {
    // .NET Encoding.ASCII.GetBytes maps each UTF-16 code unit > 0x7F to b'?' (0x3F).
    let ascii_bytes: Vec<u8> = verifier
        .encode_utf16()
        .map(|u| if u <= 0x7F { u as u8 } else { b'?' })
        .collect();
    let hash = Sha256::digest(&ascii_bytes);
    URL_SAFE_NO_PAD.encode(hash)
}

pub fn parse_o_auth_redirect(target: &str) -> Option<HashMap<String, String>> {
    let q = target.find('?');
    let path = match q {
        Some(idx) => &target[..idx],
        None => target,
    };
    if path != "/" {
        return None;
    }
    let mut map = HashMap::new();
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
            map.insert(decode(raw_k), decode(raw_v));
        }
    }
    Some(map)
}

pub fn has_o_auth_scopes(granted: &str, required: &[&str]) -> bool {
    let set: HashSet<&str> = granted.split(' ').filter(|s| !s.is_empty()).collect();
    required.iter().all(|r| set.contains(r))
}

pub fn email_from_id_token(id_token: Option<&str>) -> Option<String> {
    let token = id_token?;
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let b64 = parts[1].replace('-', "+").replace('_', "/");
    let pad = (4 - (b64.len() % 4)) % 4;
    let padded = format!("{b64}{}", "=".repeat(pad));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded)
        .ok()?;
    let doc = serde_json::from_slice::<Value>(&bytes).ok()?;
    doc.get("email").and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
