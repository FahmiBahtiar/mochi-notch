//! The Rust port of `activities/`.

#![forbid(unsafe_code)]

use serde_json::{Map, Value, json};

const MAX_BYTES: usize = 4096;
const MAX_TITLE: usize = 60;
const MAX_TEXT: usize = 120;
const MIN_TTL_SECS: i32 = 5;
const MAX_TTL_SECS: i32 = 24 * 3600;
const DEFAULT_TTL_SECS: i64 = 10 * 60;
const DONE_TTL_SECS: i64 = 12;
const WARNING_TTL_SECS: i64 = 3600;
const MAX_ACTIVITIES: usize = 8;
const AGENT_WORKING_TTL_SECS: i64 = 3600;

const PHRASES: &[&str] = &["working", "permission", "waiting", "done", "failed"];

const CLAUDE_MARK: &str = "mark:claude";
const CURSOR_MARK: &str = "mark:cursor";
const CODEX_MARK: &str = "mark:codex";
const COPILOT_MARK: &str = "mark:copilot";

const BADGE_DONE: &str = "\u{E73E}";
const BADGE_FAILED: &str = "\u{E783}";
const BADGE_ATTENTION: &str = "\u{E171}";

const ICON_NAMES: &[&str] = &[
    "activity", "code", "build", "download", "upload", "sync", "agent", "bell", "timer", "claude",
    "cursor", "codex", "copilot",
];

fn icon_glyph(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "activity" => "\u{E945}",
        "code" => "\u{E943}",
        "build" => "\u{E90F}",
        "download" => "\u{E896}",
        "upload" => "\u{E898}",
        "sync" => "\u{E895}",
        "agent" => "\u{E99A}",
        "bell" => "\u{EA8F}",
        "timer" => "\u{E916}",
        "claude" => CLAUDE_MARK,
        "cursor" => CURSOR_MARK,
        "codex" => CODEX_MARK,
        "copilot" => COPILOT_MARK,
        _ => "\u{E945}",
    }
}

fn is_known_icon(name: &str) -> bool {
    ICON_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name))
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

fn is_format_char(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

fn is_private_use(c: char) -> bool {
    let u = c as u32;
    matches!(u, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}

fn clean_text(text: Option<&str>, max: usize) -> Option<String> {
    let text = text?;
    let mut units: Vec<u16> = Vec::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() || c.is_control() {
            space = !units.is_empty();
            continue;
        }
        if is_format_char(c) || c == '\u{2028}' || c == '\u{2029}' || is_private_use(c) {
            continue;
        }
        if space {
            units.push(b' ' as u16);
        }
        space = false;
        let mut buf = [0u16; 2];
        units.extend_from_slice(c.encode_utf16(&mut buf));
    }
    if units.len() > max {
        let prev = units[max - 2];
        let cut = if (0xD800..=0xDBFF).contains(&prev) {
            max - 2
        } else {
            max - 1
        };
        units.truncate(cut);
        units.push(0x2026); // '…'
    }
    Some(String::from_utf16_lossy(&units))
}

fn json_depth_exceeds(v: &Value, depth: usize, max_depth: usize) -> bool {
    if depth > max_depth {
        return true;
    }
    match v {
        Value::Array(arr) => arr
            .iter()
            .any(|item| json_depth_exceeds(item, depth + 1, max_depth)),
        Value::Object(map) => map
            .values()
            .any(|val| json_depth_exceeds(val, depth + 1, max_depth)),
        _ => false,
    }
}

fn stj_escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' | '>' | '&' | '+' | '\'' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c if (c as u32) >= 0x7F => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn stj_serialize_value(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => stj_escape_str(s),
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(stj_serialize_value).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, val)| format!("{}:{}", stj_escape_str(k), stj_serialize_value(val)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityOp {
    Set,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityState {
    Running,
    Attention,
    Done,
    Failed,
}

impl ActivityState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Attention => "Attention",
            Self::Done => "Done",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedRequest {
    op: ActivityOp,
    id: String,
    title: Option<String>,
    text: Option<String>,
    progress: Option<f64>,
    state: ActivityState,
    icon: Option<String>,
    ttl_seconds: Option<f64>,
    phrase: Option<String>,
    window_handle: Option<i64>,
    window_pid: Option<i64>,
}

fn get_opt_str<'a>(
    map: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, &'static str> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err("a field has the wrong type"),
    }
}

fn parse_request_internal(json_str: &str) -> Result<ParsedRequest, String> {
    let _ = MAX_BYTES;
    let doc: Value = serde_json::from_str(json_str).map_err(|_| "not valid JSON".to_string())?;
    if json_depth_exceeds(&doc, 0, 4) {
        return Err("not valid JSON".to_string());
    }
    let Some(root) = doc.as_object() else {
        return Err("expected a JSON object".to_string());
    };

    let op = match get_opt_str(root, "op").map_err(str::to_string)? {
        Some("set") => ActivityOp::Set,
        Some("end") => ActivityOp::End,
        _ => return Err("op must be \"set\" or \"end\"".to_string()),
    };

    let id = get_opt_str(root, "id").map_err(str::to_string)?;
    let Some(id) = id.filter(|s| is_valid_id(s)) else {
        return Err("id must be 1-64 letters, digits or . _ : -".to_string());
    };

    if op == ActivityOp::End {
        return Ok(ParsedRequest {
            op,
            id: id.to_string(),
            title: None,
            text: None,
            progress: None,
            state: ActivityState::Running,
            icon: None,
            ttl_seconds: None,
            phrase: None,
            window_handle: None,
            window_pid: None,
        });
    }

    let state = match get_opt_str(root, "state").map_err(str::to_string)? {
        None | Some("running") => ActivityState::Running,
        Some("attention") => ActivityState::Attention,
        Some("done") => ActivityState::Done,
        Some("failed") => ActivityState::Failed,
        _ => return Err("state must be running, attention, done or failed".to_string()),
    };

    let icon = get_opt_str(root, "icon").map_err(str::to_string)?;
    if let Some(ic) = icon
        && !is_known_icon(ic)
    {
        return Err(format!(
            "unknown icon; use one of: {}",
            ICON_NAMES.join(", ")
        ));
    }

    let progress = match root.get("progress") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => {
            let Some(v) = n.as_f64() else {
                return Err("progress must be a number from 0 to 1".to_string());
            };
            if v.is_nan() || !(0.0..=1.0).contains(&v) {
                return Err("progress must be a number from 0 to 1".to_string());
            }
            Some(v)
        }
        Some(_) => return Err("progress must be a number from 0 to 1".to_string()),
    };

    let ttl_seconds = match root.get("ttl") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => {
            let Some(sec) = n.as_i64().and_then(|v| i32::try_from(v).ok()) else {
                return Err(format!(
                    "ttl must be {MIN_TTL_SECS} to {MAX_TTL_SECS} seconds"
                ));
            };
            if !(MIN_TTL_SECS..=MAX_TTL_SECS).contains(&sec) {
                return Err(format!(
                    "ttl must be {MIN_TTL_SECS} to {MAX_TTL_SECS} seconds"
                ));
            }
            Some(f64::from(sec))
        }
        Some(_) => {
            return Err(format!(
                "ttl must be {MIN_TTL_SECS} to {MAX_TTL_SECS} seconds"
            ));
        }
    };

    let phrase = get_opt_str(root, "phrase").map_err(str::to_string)?;
    if let Some(ph) = phrase
        && !PHRASES.contains(&ph)
    {
        return Err(format!("phrase must be one of: {}", PHRASES.join(", ")));
    }

    let (window_handle, window_pid) = match root.get("window") {
        None | Some(Value::Null) => (None, None),
        Some(Value::Object(w)) => {
            let h = w.get("handle").and_then(Value::as_i64).filter(|&v| v > 0);
            let p = w
                .get("pid")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .filter(|&v| v > 0);
            match (h, p) {
                (Some(hv), Some(pv)) => (Some(hv), Some(i64::from(pv))),
                _ => return Err("window must be {\"handle\": number, \"pid\": number}".to_string()),
            }
        }
        Some(_) => return Err("window must be {\"handle\": number, \"pid\": number}".to_string()),
    };

    let raw_title = get_opt_str(root, "title").map_err(str::to_string)?;
    let title = clean_text(raw_title, MAX_TITLE);
    if raw_title.is_some_and(|s| !s.is_empty()) && title.as_deref().is_none_or(str::is_empty) {
        return Err("title has no visible characters".to_string());
    }

    let raw_text = get_opt_str(root, "text").map_err(str::to_string)?;
    let text = clean_text(raw_text, MAX_TEXT);

    Ok(ParsedRequest {
        op,
        id: id.to_string(),
        title,
        text,
        progress,
        state,
        icon: icon.map(str::to_string),
        ttl_seconds,
        phrase: phrase.map(str::to_string),
        window_handle,
        window_pid,
    })
}

/// Port of `ParseActivityRequest` (`activities/ActivitiesUnit.cs:41`).
pub fn parse_activity_request(json_str: String) -> Result<Value, String> {
    match parse_request_internal(&json_str) {
        Ok(req) => Ok(json!({
            "Ok": true,
            "Error": Value::Null,
            "Op": match req.op { ActivityOp::Set => "Set", ActivityOp::End => "End" },
            "Id": req.id,
            "Title": req.title,
            "Text": req.text,
            "Progress": req.progress,
            "State": req.state.as_str(),
            "Icon": req.icon,
            "TtlSeconds": req.ttl_seconds,
            "Phrase": req.phrase,
            "WindowHandle": req.window_handle,
            "WindowPid": req.window_pid,
        })),
        Err(err) => Ok(json!({
            "Ok": false,
            "Error": err,
            "Op": Value::Null,
            "Id": Value::Null,
            "Title": Value::Null,
            "Text": Value::Null,
            "Progress": Value::Null,
            "State": Value::Null,
            "Icon": Value::Null,
            "TtlSeconds": Value::Null,
            "Phrase": Value::Null,
            "WindowHandle": Value::Null,
            "WindowPid": Value::Null,
        })),
    }
}

/// Port of `CleanActivityText` (`activities/ActivitiesUnit.cs:62`).
pub fn clean_activity_text(text: Option<String>, max: i64) -> Result<Option<String>, String> {
    let max = max.clamp(2, 500) as usize;
    Ok(clean_text(text.as_deref(), max))
}

/// Port of `ParseActivityProgress` (`activities/ActivitiesUnit.cs:77`).
pub fn parse_activity_progress(value: String) -> Result<Option<f64>, String> {
    let percent = value.ends_with('%');
    let slice = if percent {
        &value[..value.len() - 1]
    } else {
        &value
    };
    // C# NumberStyles.Float allows leading/trailing whitespace, sign, decimal, exponent,
    // but rejects NaN/Infinity unless parsed; wait: double.TryParse with NumberStyles.Float
    // rejects "NaN"/"Infinity" in InvariantCulture? Actually in .NET NumberStyles.Float
    // rejects NaN/Infinity! And rejects underscores or hex.
    let trimmed = slice.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("nan")
        || trimmed.eq_ignore_ascii_case("infinity")
        || trimmed.eq_ignore_ascii_case("+infinity")
        || trimmed.eq_ignore_ascii_case("-infinity")
        || trimmed.eq_ignore_ascii_case("inf")
        || trimmed.eq_ignore_ascii_case("+inf")
        || trimmed.eq_ignore_ascii_case("-inf")
    {
        return Ok(None);
    }
    let Ok(mut number) = trimmed.parse::<f64>() else {
        return Ok(None);
    };
    if percent {
        number /= 100.0;
    }
    if (0.0..=1.0).contains(&number) {
        Ok(Some(number))
    } else {
        Ok(None)
    }
}

/// Port of `BuildActivityCommand` (`activities/ActivitiesUnit.cs:68`).
pub fn build_activity_command(
    args: Vec<String>,
    window_handle: Option<i64>,
    window_pid: Option<i64>,
) -> Result<Value, String> {
    let fail = |reason: String| {
        Ok(json!({
            "Ok": false,
            "Json": Value::Null,
            "Error": reason,
        }))
    };

    if args.len() < 2 {
        return fail("missing the action and the id".to_string());
    }

    let state = match args[0].to_ascii_lowercase().as_str() {
        "set" => Some("running"),
        "attention" => Some("attention"),
        "done" => Some("done"),
        "fail" | "failed" => Some("failed"),
        "end" => None,
        _ => return fail(format!("unknown action \"{}\"", args[0])),
    };

    let mut map = Map::new();
    map.insert(
        "op".to_string(),
        Value::String(if state.is_none() { "end" } else { "set" }.to_string()),
    );
    map.insert("id".to_string(), Value::String(args[1].clone()));

    if let Some(st) = state {
        map.insert("state".to_string(), Value::String(st.to_string()));
        if let (Some(h), Some(p)) = (window_handle, window_pid)
            && h > 0
            && p > 0
            && i32::try_from(p).is_ok()
        {
            let mut win = Map::new();
            win.insert("handle".to_string(), json!(h));
            win.insert("pid".to_string(), json!(p));
            map.insert("window".to_string(), Value::Object(win));
        }
    }

    let mut i = 2;
    while i < args.len() {
        let option = &args[i];
        if i + 1 >= args.len() {
            return fail(format!("{option} needs a value"));
        }
        let value = &args[i + 1];
        i += 2;
        match option.as_str() {
            "--title" => {
                if clean_text(Some(value), MAX_TITLE)
                    .as_deref()
                    .is_none_or(str::is_empty)
                {
                    return fail("--title has no visible characters".to_string());
                }
                map.insert("title".to_string(), Value::String(value.clone()));
            }
            "--text" => {
                map.insert("text".to_string(), Value::String(value.clone()));
            }
            "--icon" => {
                if !is_known_icon(value) {
                    return fail(format!(
                        "unknown icon \"{value}\"; use one of: {}",
                        ICON_NAMES.join(", ")
                    ));
                }
                map.insert("icon".to_string(), Value::String(value.clone()));
            }
            "--progress" => {
                let Some(progress) = parse_activity_progress(value.clone())? else {
                    return fail("--progress takes 0 to 1 (0.4) or a percentage (40%)".to_string());
                };
                map.insert("progress".to_string(), json!(progress));
            }
            "--ttl" => {
                // NumberStyles.None: ASCII digits only, fits in i32
                if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                    return fail("--ttl takes whole seconds".to_string());
                }
                let Ok(seconds) = value.parse::<i32>() else {
                    return fail("--ttl takes whole seconds".to_string());
                };
                if !(MIN_TTL_SECS..=MAX_TTL_SECS).contains(&seconds) {
                    return fail(format!(
                        "--ttl takes {MIN_TTL_SECS} to {MAX_TTL_SECS} seconds"
                    ));
                }
                map.insert("ttl".to_string(), json!(seconds));
            }
            _ => return fail(format!("unknown option {option}")),
        }
    }

    let serialized = stj_serialize_value(&Value::Object(map));
    Ok(json!({
        "Ok": true,
        "Json": serialized,
        "Error": Value::Null,
    }))
}

#[derive(Debug, Clone, Copy)]
enum HookStatus {
    Working,
    Permission,
    Waiting,
    Done,
    Failed,
    Ended,
}

fn sanitize_session_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
        .collect()
}

fn hook_request(
    agent: &str,
    product: &str,
    session: Option<&str>,
    folder: Option<&str>,
    status: Option<HookStatus>,
    text: Option<&str>,
) -> Option<String> {
    let status = status?;
    let mut id = format!(
        "{agent}-{}",
        sanitize_session_id(session.unwrap_or("session"))
    );
    if id.len() > 64 {
        id.truncate(64);
    }
    if matches!(status, HookStatus::Ended) {
        let mut map = Map::new();
        map.insert("op".to_string(), Value::String("end".to_string()));
        map.insert("id".to_string(), Value::String(id));
        return Some(stj_serialize_value(&Value::Object(map)));
    }

    let project = project_name_internal(folder);
    let title = title_internal(product, &project, None);
    let state_str = match status {
        HookStatus::Permission | HookStatus::Waiting => "attention",
        HookStatus::Done => "done",
        HookStatus::Failed => "failed",
        HookStatus::Working | HookStatus::Ended => "running",
    };
    let phrase_str = match status {
        HookStatus::Permission => "permission",
        HookStatus::Waiting => "waiting",
        HookStatus::Done => "done",
        HookStatus::Failed => "failed",
        HookStatus::Working | HookStatus::Ended => "working",
    };

    let mut map = Map::new();
    map.insert("op".to_string(), Value::String("set".to_string()));
    map.insert("id".to_string(), Value::String(id));
    map.insert("title".to_string(), Value::String(title));
    if let Some(t) = clean_text(text, MAX_TEXT).filter(|s| !s.is_empty()) {
        map.insert("text".to_string(), Value::String(t));
    }
    map.insert("icon".to_string(), Value::String(agent.to_string()));
    map.insert("state".to_string(), Value::String(state_str.to_string()));
    map.insert("phrase".to_string(), Value::String(phrase_str.to_string()));
    if matches!(status, HookStatus::Working) {
        map.insert("ttl".to_string(), json!(AGENT_WORKING_TTL_SECS));
    }
    Some(stj_serialize_value(&Value::Object(map)))
}

fn json_str<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    map.get(key).and_then(Value::as_str)
}

/// Port of `TranslateAgentHook` (`activities/ActivitiesUnit.cs:79`).
pub fn translate_agent_hook(agent: String, payload: Option<String>) -> Result<Value, String> {
    let raw = match payload.as_deref() {
        Some(s) if !s.trim().is_empty() => s,
        _ => "{}",
    };
    let Ok(doc) = serde_json::from_str::<Value>(raw) else {
        return Ok(json!({ "Request": Value::Null, "Output": Value::Null }));
    };
    let Some(root) = doc.as_object() else {
        return Ok(json!({ "Request": Value::Null, "Output": Value::Null }));
    };

    let (req, out): (Option<String>, Option<&str>) = match agent.as_str() {
        "claude" => {
            let status = match json_str(root, "hook_event_name") {
                Some("UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure") => {
                    Some(HookStatus::Working)
                }
                Some("PermissionRequest") => Some(HookStatus::Permission),
                Some("Notification") => match json_str(root, "notification_type") {
                    Some("permission_prompt") => Some(HookStatus::Permission),
                    Some("idle_prompt") => Some(HookStatus::Waiting),
                    _ => None,
                },
                Some("Stop") => Some(HookStatus::Done),
                Some("StopFailure") => Some(HookStatus::Failed),
                Some("SessionEnd") => Some(HookStatus::Ended),
                _ => None,
            };
            (
                hook_request(
                    "claude",
                    "Claude Code",
                    json_str(root, "session_id"),
                    json_str(root, "cwd"),
                    status,
                    None,
                ),
                None,
            )
        }
        "cursor" => {
            let name = json_str(root, "hook_event_name");
            let status = match name {
                Some(
                    "beforeSubmitPrompt"
                    | "postToolUse"
                    | "postToolUseFailure"
                    | "afterShellExecution"
                    | "afterMCPExecution"
                    | "afterFileEdit",
                ) => Some(HookStatus::Working),
                Some("stop") => match json_str(root, "status") {
                    Some("completed") => Some(HookStatus::Done),
                    Some("error") => Some(HookStatus::Failed),
                    _ => Some(HookStatus::Ended),
                },
                Some("sessionEnd") => Some(HookStatus::Ended),
                _ => None,
            };
            let folder = root
                .get("workspace_roots")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str);
            let output = match name {
                Some("beforeSubmitPrompt") => Some("{\"continue\":true}"),
                Some("stop") => Some("{}"),
                _ => None,
            };
            (
                hook_request(
                    "cursor",
                    "Cursor",
                    json_str(root, "conversation_id"),
                    folder,
                    status,
                    None,
                ),
                output,
            )
        }
        "codex" => {
            if json_str(root, "type") == Some("agent-turn-complete") {
                (
                    hook_request(
                        "codex",
                        "Codex",
                        json_str(root, "thread-id"),
                        json_str(root, "cwd"),
                        Some(HookStatus::Done),
                        None,
                    ),
                    None,
                )
            } else {
                let name = json_str(root, "hook_event_name");
                let status = match name {
                    Some("UserPromptSubmit" | "PostToolUse") => Some(HookStatus::Working),
                    Some("PermissionRequest") => Some(HookStatus::Permission),
                    Some("Stop") => Some(HookStatus::Done),
                    Some("Interrupt" | "SessionEnd") => Some(HookStatus::Ended),
                    _ => None,
                };
                let output = match name {
                    Some("Stop" | "Interrupt") => Some("{}"),
                    _ => None,
                };
                (
                    hook_request(
                        "codex",
                        "Codex",
                        json_str(root, "session_id"),
                        json_str(root, "cwd"),
                        status,
                        None,
                    ),
                    output,
                )
            }
        }
        "copilot" => {
            let name = json_str(root, "hook_event_name");
            let status = match name {
                Some(
                    "SessionStart"
                    | "sessionStart"
                    | "UserPromptSubmit"
                    | "userPromptSubmitted"
                    | "PreToolUse"
                    | "preToolUse"
                    | "PostToolUse"
                    | "postToolUse"
                    | "PreCompact"
                    | "SubagentStart"
                    | "SubagentStop"
                    | "subagentStop",
                ) => Some(HookStatus::Working),
                Some("Stop" | "agentStop") => Some(HookStatus::Done),
                Some("ErrorOccurred" | "errorOccurred") => Some(HookStatus::Failed),
                Some("SessionEnd" | "sessionEnd") => Some(HookStatus::Ended),
                _ => None,
            };
            let live_text: Option<String> = match name {
                Some("UserPromptSubmit" | "userPromptSubmitted") => json_str(root, "prompt")
                    .and_then(|p| clean_text(Some(p), 48))
                    .filter(|s| !s.is_empty())
                    .or_else(|| Some("Prompting".to_string())),
                Some("PreToolUse" | "preToolUse") => json_str(root, "tool_name")
                    .and_then(|t| clean_text(Some(t), 40))
                    .map(|t| format!("Using {t}")),
                Some("PostToolUse" | "postToolUse") => json_str(root, "tool_name")
                    .and_then(|t| clean_text(Some(t), 40))
                    .map(|t| format!("Ran {t}")),
                Some("SubagentStart") => Some("Running subagent".to_string()),
                Some("PreCompact") => Some("Compacting context".to_string()),
                Some("SessionStart" | "sessionStart") => Some("Session started".to_string()),
                _ => None,
            };
            let session = json_str(root, "session_id").or_else(|| json_str(root, "sessionId"));
            (
                hook_request(
                    "copilot",
                    "GitHub Copilot",
                    session,
                    json_str(root, "cwd"),
                    status,
                    live_text.as_deref(),
                ),
                None,
            )
        }
        _ => (None, None),
    };

    Ok(json!({
        "Request": req,
        "Output": out,
    }))
}

/// Port of `AgentHookSetup` (`activities/ActivitiesUnit.cs:85`).
pub fn agent_hook_setup(agent: String) -> Result<Option<String>, String> {
    let text = match agent.as_str() {
        "claude" => concat!(
            "Add to %USERPROFILE%\\.claude\\settings.json (merge with what's there), or install the plugin instead:\r\n",
            "/plugin marketplace add mochi/mochi, then /plugin install mochi@mochi\r\n",
            "{\r\n",
            "  \"hooks\": {\r\n",
            "    \"UserPromptSubmit\":  [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"PostToolUse\":       [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"PostToolUseFailure\": [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"PermissionRequest\": [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"Notification\":      [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"Stop\":              [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"StopFailure\":       [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 5 }] }],\r\n",
            "    \"SessionEnd\":        [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook claude\", \"timeout\": 1 }] }]\r\n",
            "  }\r\n",
            "}"
        ),
        "cursor" => concat!(
            "Save as %USERPROFILE%\\.cursor\\hooks.json (merge with what's there):\r\n",
            "{\r\n",
            "  \"version\": 1,\r\n",
            "  \"hooks\": {\r\n",
            "    \"beforeSubmitPrompt\":  [{ \"command\": \"mochi-cli hook cursor\", \"timeout\": 5 }],\r\n",
            "    \"postToolUse\":         [{ \"command\": \"mochi-cli hook cursor\", \"timeout\": 5 }],\r\n",
            "    \"afterShellExecution\": [{ \"command\": \"mochi-cli hook cursor\", \"timeout\": 5 }],\r\n",
            "    \"afterFileEdit\":       [{ \"command\": \"mochi-cli hook cursor\", \"timeout\": 5 }],\r\n",
            "    \"stop\":                [{ \"command\": \"mochi-cli hook cursor\", \"timeout\": 5 }],\r\n",
            "    \"sessionEnd\":          [{ \"command\": \"mochi-cli hook cursor\", \"timeout\": 5 }]\r\n",
            "  }\r\n",
            "}\r\n",
            "Cursor has no event for \"waiting for your approval\", so that state isn't shown."
        ),
        "codex" => concat!(
            "Save as %USERPROFILE%\\.codex\\hooks.json (merge with what's there), then review and trust it with /hooks:\r\n",
            "{\r\n",
            "  \"hooks\": {\r\n",
            "    \"UserPromptSubmit\":  [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook codex\", \"timeout\": 5 }] }],\r\n",
            "    \"PostToolUse\":       [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook codex\", \"timeout\": 5 }] }],\r\n",
            "    \"PermissionRequest\": [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook codex\", \"timeout\": 5 }] }],\r\n",
            "    \"Stop\":              [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook codex\", \"timeout\": 5 }] }],\r\n",
            "    \"Interrupt\":         [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook codex\", \"timeout\": 2 }] }],\r\n",
            "    \"SessionEnd\":        [{ \"hooks\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook codex\", \"timeout\": 2 }] }]\r\n",
            "  }\r\n",
            "}\r\n",
            "Older Codex without hooks: put notify = [\"mochi-cli\", \"hook\", \"codex\"] at the top of\r\n",
            "%USERPROFILE%\\.codex\\config.toml (only \"done\" is shown then)."
        ),
        "copilot" => concat!(
            "Save as %USERPROFILE%\\.copilot\\hooks\\mochi.json (or .github\\hooks\\mochi.json in a repo):\r\n",
            "{\r\n",
            "  \"hooks\": {\r\n",
            "    \"SessionStart\":     [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"UserPromptSubmit\": [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"PreToolUse\":       [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"PostToolUse\":      [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"SubagentStart\":    [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"SubagentStop\":     [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"Stop\":             [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"ErrorOccurred\":    [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 5 }],\r\n",
            "    \"SessionEnd\":       [{ \"type\": \"command\", \"command\": \"mochi-cli hook copilot\", \"timeout\": 2 }]\r\n",
            "  }\r\n",
            "}"
        ),
        _ => return Ok(None),
    };
    Ok(Some(text.to_string()))
}

fn title_internal(product: &str, project: &str, host: Option<&str>) -> String {
    match (host, !project.is_empty()) {
        (Some(h), true) => format!("{h} · {project}"),
        (Some(h), false) => h.to_string(),
        (None, true) => project.to_string(),
        (None, false) => product.to_string(),
    }
}

/// Port of `AgentHookTitle` (`activities/ActivitiesUnit.cs:87`).
pub fn agent_hook_title(
    product: String,
    project: String,
    host: Option<String>,
) -> Result<String, String> {
    Ok(title_internal(&product, &project, host.as_deref()))
}

fn project_name_internal(folder: Option<&str>) -> String {
    let Some(folder) = folder else {
        return String::new();
    };
    if folder.trim().is_empty() {
        return String::new();
    }
    let trimmed = folder.trim_end_matches(['\\', '/']);
    match trimmed.rfind(['\\', '/']) {
        Some(cut) => trimmed[cut + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// Port of `AgentHookProjectName` (`activities/ActivitiesUnit.cs:90`).
pub fn agent_hook_project_name(folder: Option<String>) -> Result<String, String> {
    Ok(project_name_internal(folder.as_deref()))
}

#[derive(Debug, Clone)]
struct BoardActivity {
    id: String,
    title: String,
    text: Option<String>,
    progress: Option<f64>,
    state: ActivityState,
    icon: String,
    _updated: i64,
    _expires: i64,
    window: Option<(i64, i64)>,
}

impl BoardActivity {
    fn glyph(&self) -> &'static str {
        icon_glyph(&self.icon)
    }

    fn badge(&self) -> Option<&'static str> {
        match self.state {
            ActivityState::Done => Some(BADGE_DONE),
            ActivityState::Failed => Some(BADGE_FAILED),
            ActivityState::Attention => Some(BADGE_ATTENTION),
            ActivityState::Running => None,
        }
    }

    fn warning(&self) -> bool {
        matches!(self.state, ActivityState::Attention | ActivityState::Failed)
    }

    fn line(&self) -> String {
        match &self.text {
            Some(t) if !t.is_empty() => format!("{} · {t}", self.title),
            _ => self.title.clone(),
        }
    }
}

/// Port of `SimulateActivityBoard` (`activities/ActivitiesUnit.cs:92`).
pub fn simulate_activity_board(json_requests: Vec<String>) -> Result<Vec<Value>, String> {
    let mut items: Vec<BoardActivity> = Vec::new();
    let mut now: i64 = 0;
    let mut results = Vec::new();

    for json_str in json_requests {
        now += 1;
        let Ok(req) = parse_request_internal(&json_str) else {
            continue;
        };
        let index = items.iter().position(|a| a.id == req.id);
        if req.op == ActivityOp::End {
            if let Some(idx) = index {
                let ended = items.remove(idx);
                results.push(board_step_json("Ended", Some(&ended)));
            } else {
                results.push(board_step_json("Unknown", None));
            }
            continue;
        }

        let previous = index.map(|idx| items[idx].clone());
        if previous.is_none() && items.len() >= MAX_ACTIVITIES {
            results.push(board_step_json("Full", None));
            continue;
        }
        let title = req
            .title
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| previous.as_ref().map(|p| p.title.clone()));
        let Some(title) = title else {
            results.push(board_step_json("NeedsTitle", None));
            continue;
        };

        let ttl = match req.state {
            ActivityState::Done => DONE_TTL_SECS,
            ActivityState::Attention | ActivityState::Failed => WARNING_TTL_SECS,
            ActivityState::Running => req
                .ttl_seconds
                .map(|v| v as i64)
                .unwrap_or(DEFAULT_TTL_SECS),
        };
        let req_win = req
            .window_handle
            .zip(req.window_pid)
            .or_else(|| previous.as_ref().and_then(|p| p.window));
        let activity = BoardActivity {
            id: req.id,
            title,
            text: req
                .text
                .or_else(|| previous.as_ref().and_then(|p| p.text.clone())),
            progress: if req.state == ActivityState::Done {
                None
            } else {
                req.progress
                    .or_else(|| previous.as_ref().and_then(|p| p.progress))
            },
            state: req.state,
            icon: req
                .icon
                .or_else(|| previous.as_ref().map(|p| p.icon.clone()))
                .unwrap_or_else(|| "activity".to_string()),
            _updated: now,
            _expires: now + ttl,
            window: req_win,
        };

        let change = match (index, &previous) {
            (None, _) => {
                items.push(activity.clone());
                "Added"
            }
            (Some(idx), Some(prev)) => {
                let ch = if prev.state != activity.state {
                    "StateChanged"
                } else {
                    "Updated"
                };
                items[idx] = activity.clone();
                ch
            }
            (Some(_), None) => "Added",
        };
        results.push(board_step_json(change, Some(&activity)));
    }

    Ok(results)
}

fn board_step_json(change: &str, act: Option<&BoardActivity>) -> Value {
    match act {
        Some(a) => json!({
            "Change": change,
            "Id": a.id,
            "Title": a.title,
            "Text": a.text,
            "Progress": a.progress,
            "State": a.state.as_str(),
            "Icon": a.icon,
            "Glyph": a.glyph(),
            "Badge": a.badge(),
            "Warning": a.warning(),
            "Line": a.line(),
        }),
        None => json!({
            "Change": change,
            "Id": Value::Null,
            "Title": Value::Null,
            "Text": Value::Null,
            "Progress": Value::Null,
            "State": Value::Null,
            "Icon": Value::Null,
            "Glyph": Value::Null,
            "Badge": Value::Null,
            "Warning": false,
            "Line": Value::Null,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_never_splits_surrogate() {
        let text = format!("{}😀😀", "a".repeat(58));
        let cleaned = clean_activity_text(Some(text), 60).unwrap().unwrap();
        assert_eq!(cleaned.encode_utf16().count(), 59);
        assert!(cleaned.ends_with('…'));
    }

    #[test]
    fn build_command_matches_cli() {
        let res = build_activity_command(
            vec![
                "set".into(),
                "build".into(),
                "--title".into(),
                "Build".into(),
                "--progress".into(),
                "40%".into(),
            ],
            None,
            None,
        )
        .unwrap();
        assert_eq!(res["Ok"], true);
    }

    #[test]
    fn copilot_hook_translates_prompt_tool_and_stop() {
        let prompt_res = translate_agent_hook(
            "copilot".into(),
            Some(
                r#"{"hook_event_name":"UserPromptSubmit","session_id":"vs1","cwd":"C:\\src\\Mochi","prompt":"fix the build"}"#
                    .into(),
            ),
        )
        .unwrap();
        let req = prompt_res["Request"].as_str().unwrap();
        assert!(req.contains("\"icon\":\"copilot\""));
        assert!(req.contains("\"state\":\"running\""));
        assert!(req.contains("fix the build"));

        let tool_res = translate_agent_hook(
            "copilot".into(),
            Some(
                r#"{"hook_event_name":"PreToolUse","session_id":"vs1","cwd":"C:\\src\\Mochi","tool_name":"run_in_terminal"}"#
                    .into(),
            ),
        )
        .unwrap();
        assert!(
            tool_res["Request"]
                .as_str()
                .unwrap()
                .contains("Using run_in_terminal")
        );

        let stop_res = translate_agent_hook(
            "copilot".into(),
            Some(r#"{"hook_event_name":"Stop","session_id":"vs1","cwd":"C:\\src\\Mochi"}"#.into()),
        )
        .unwrap();
        assert!(
            stop_res["Request"]
                .as_str()
                .unwrap()
                .contains("\"state\":\"done\"")
        );
    }
}
