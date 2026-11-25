//! The Rust port of `media_logic/`.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{Value, json};

const KNOWN_PLAYERS: &[(&str, &str)] = &[
    ("spotify", "Spotify"),
    ("msedge", "Edge"),
    ("chrome", "Chrome"),
    ("firefox", "Firefox"),
    ("308046b0af4a39cb", "Firefox"),
    ("opera", "Opera"),
    ("brave", "Brave"),
    ("vivaldi", "Vivaldi"),
    ("zunemusic", "Media Player"),
    ("zunevideo", "Movies & TV"),
    ("vlc", "VLC"),
    ("foobar2000", "foobar2000"),
    ("applemusic", "Apple Music"),
    ("itunes", "iTunes"),
    ("deezer", "Deezer"),
    ("tidal", "TIDAL"),
];

#[derive(Debug, Clone, Copy)]
struct SessionCandidate {
    is_playing: bool,
    is_shown: bool,
    is_system_current: bool,
    is_untitled: bool,
}

/// Port of `PickMediaSession` (`media_logic/MediaLogicUnit.cs:33`).
pub fn pick_media_session(sessions: Option<Vec<Value>>) -> Result<i64, String> {
    let Some(raw) = sessions else {
        return Ok(-1);
    };
    if raw.is_empty() {
        return Ok(-1);
    }
    let parsed: Vec<SessionCandidate> = raw
        .iter()
        .map(|v| SessionCandidate {
            is_playing: v.get("IsPlaying").and_then(Value::as_bool).unwrap_or(false),
            is_shown: v.get("IsShown").and_then(Value::as_bool).unwrap_or(false),
            is_system_current: v
                .get("IsSystemCurrent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_untitled: v
                .get("IsUntitled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect();

    let any_titled = parsed.iter().any(|s| !s.is_untitled);
    let find = |pred: fn(&SessionCandidate) -> bool| -> Option<usize> {
        parsed
            .iter()
            .enumerate()
            .find(|(_, s)| (!any_titled || !s.is_untitled) && pred(s))
            .map(|(i, _)| i)
    };

    let rules: [fn(&SessionCandidate) -> bool; 6] = [
        |s| s.is_shown && s.is_playing,
        |s| s.is_system_current && s.is_playing,
        |s| s.is_playing,
        |s| s.is_shown,
        |s| s.is_system_current,
        |_| true,
    ];

    for rule in rules {
        if let Some(idx) = find(rule) {
            return Ok(idx as i64);
        }
    }
    Ok(-1)
}

fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

fn readable_name(name: &str) -> Option<String> {
    let units: Vec<u16> = name.encode_utf16().collect();
    if units.is_empty()
        || (units.len() >= 12
            && units
                .iter()
                .all(|&u| u <= 0x7F && (u as u8 as char).is_ascii_hexdigit()))
    {
        return None;
    }
    let first = units[0];
    let upper = if first <= 0x7F {
        u16::from((first as u8).to_ascii_uppercase())
    } else if first == 0x0131 {
        // .NET char.ToUpperInvariant('ı') leaves U+0131 unchanged (unlike Unicode simple uppercase 'I').
        first
    } else if let Some(c) = char::from_u32(u32::from(first)) {
        let mut it = c.to_uppercase();
        let u = it.next();
        if it.next().is_none()
            && let Some(uc) = u
            && (uc as u32) <= 0xFFFF
        {
            uc as u16
        } else {
            first
        }
    } else {
        first
    };
    let mut out = Vec::with_capacity(units.len());
    out.push(upper);
    out.extend_from_slice(&units[1..]);
    Some(String::from_utf16_lossy(&out))
}

/// Port of `FriendlyPlayerName` (`media_logic/MediaLogicUnit.cs:64`).
pub fn friendly_player_name(app_id: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = app_id else {
        return Ok(None);
    };
    let id = raw.trim();
    if id.is_empty() {
        return Ok(None);
    }
    for &(needle, name) in KNOWN_PLAYERS {
        if contains_ascii_ci(id, needle) {
            return Ok(Some(name.to_string()));
        }
    }
    if let Some(bang) = id.find('!') {
        let mut package = &id[..bang];
        if let Some(us) = package.find('_')
            && us > 0
        {
            package = &package[..us];
        }
        let last = package.rsplit('.').next().unwrap_or("");
        return Ok(readable_name(last));
    }
    let mut file = id.rsplit(['\\', '/']).next().unwrap_or("");
    if let Some(stripped) = file.get(..file.len().saturating_sub(4)).filter(|_| {
        file.len() >= 4 && file.as_bytes()[file.len() - 4..].eq_ignore_ascii_case(b".exe")
    }) {
        file = stripped;
    }
    Ok(readable_name(file))
}

#[derive(Default)]
struct PlayerChoice {
    playing: HashMap<String, bool>,
    removed: HashSet<String>,
    chosen: Option<String>,
}

impl PlayerChoice {
    fn is_removed(&self, id: &str) -> bool {
        self.removed.contains(id)
    }

    fn seen(&mut self, id: &str, playing: bool) -> bool {
        let started = playing && self.playing.get(id).copied() == Some(false);
        self.playing.insert(id.to_string(), playing);
        if !started {
            return false;
        }
        let mut changed = self.removed.remove(id);
        if self.chosen.as_deref().is_some_and(|c| c != id) {
            self.chosen = None;
            changed = true;
        }
        changed
    }

    fn choose(&mut self, id: &str) {
        self.chosen = Some(id.to_string());
        self.removed.remove(id);
    }

    fn remove(&mut self, id: &str) {
        self.removed.insert(id.to_string());
        if self.chosen.as_deref() == Some(id) {
            self.chosen = None;
        }
    }

    fn keep(&mut self, open: &HashSet<String>) {
        self.playing.retain(|k, _| open.contains(k));
        self.removed.retain(|k| open.contains(k));
        if self.chosen.as_deref().is_some_and(|c| !open.contains(c)) {
            self.chosen = None;
        }
    }
}

/// Port of `SimulatePlayerChoice` (`media_logic/MediaLogicUnit.cs:66`).
pub fn simulate_player_choice(steps: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
    let Some(steps) = steps else {
        return Ok(Vec::new());
    };
    let mut choice = PlayerChoice::default();
    let mut results = Vec::with_capacity(steps.len());
    for step in steps {
        let op = step.get("Op").and_then(Value::as_str).unwrap_or("");
        let id = step.get("Id").and_then(Value::as_str);
        let playing = step
            .get("Playing")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut changed = false;
        match (op, id) {
            ("seen", Some(id)) => changed = choice.seen(id, playing),
            ("choose", Some(id)) => choice.choose(id),
            ("remove", Some(id)) => choice.remove(id),
            ("keep", _) => {
                let open: HashSet<String> = step
                    .get("Open")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                choice.keep(&open);
            }
            _ => {}
        }
        let removed = id.is_some_and(|i| choice.is_removed(i));
        results.push(json!({
            "Changed": changed,
            "Chosen": choice.chosen,
            "IsRemoved": removed,
        }));
    }
    Ok(results)
}

const MAX_WAITING: usize = 5;

#[derive(Debug, Clone)]
struct HudMsg {
    text: String,
    value: Option<String>,
    important: bool,
    kind: String,
    tag: Option<String>,
    expires_ticks: Option<i64>,
}

#[derive(Default)]
struct HudQueue {
    waiting: VecDeque<HudMsg>,
    current: Option<HudMsg>,
}

impl HudQueue {
    fn forget(&mut self, tag: &str) {
        self.waiting.retain(|m| m.tag.as_deref() != Some(tag));
    }

    fn enqueue(&mut self, msg: HudMsg, first: bool) {
        if let Some(pos) = self
            .waiting
            .iter()
            .position(|m| m.kind == msg.kind && m.text == msg.text)
        {
            self.waiting.remove(pos);
        }
        if first {
            self.waiting.push_front(msg);
        } else {
            self.waiting.push_back(msg);
        }
        while self.waiting.len() > MAX_WAITING {
            if first {
                self.waiting.remove(1);
            } else {
                self.waiting.pop_front();
            }
        }
    }

    fn next(&mut self, now_ticks: i64) -> Option<HudMsg> {
        while let Some(first) = self.waiting.pop_front() {
            if first.expires_ticks.is_none_or(|exp| exp > now_ticks) {
                self.current = Some(first.clone());
                return Some(first);
            }
        }
        None
    }

    fn offer(&mut self, msg: HudMsg, can_show: bool) -> Option<HudMsg> {
        if let Some(tag) = msg.tag.as_deref() {
            self.forget(tag);
        }
        let replaces_current = msg.tag.is_some()
            && self
                .current
                .as_ref()
                .is_some_and(|c| c.tag.as_deref() == msg.tag.as_deref());

        if !can_show {
            if msg.important {
                self.enqueue(msg, false);
            }
            return None;
        }
        if replaces_current {
            self.current = Some(msg.clone());
            return Some(msg);
        }
        if let Some(cur) = self.current.clone()
            && cur.important
        {
            if msg.important {
                self.enqueue(msg, false);
                return None;
            }
            self.enqueue(cur, true);
        }
        self.current = Some(msg.clone());
        Some(msg)
    }

    fn finish(&mut self, now_ticks: i64) -> Option<HudMsg> {
        self.current = None;
        self.next(now_ticks)
    }

    fn interrupt(&mut self) {
        if let Some(cur) = self.current.take()
            && cur.important
        {
            self.enqueue(cur, true);
        }
    }

    fn resume(&mut self, now_ticks: i64) -> Option<HudMsg> {
        if self.current.is_none() {
            self.next(now_ticks)
        } else {
            None
        }
    }
}

fn parse_hud_kind(raw: &str) -> &'static str {
    match raw.to_ascii_lowercase().as_str() {
        "reminder" => "Reminder",
        "volume" => "Volume",
        "brightness" => "Brightness",
        "power" => "Power",
        "audiodevice" => "AudioDevice",
        "capslock" => "CapsLock",
        "keyboardlayout" => "KeyboardLayout",
        "network" => "Network",
        "privacy" => "Privacy",
        "weather" => "Weather",
        "activity" => "Activity",
        _ => "Reminder",
    }
}

/// Port of `SimulateHudQueue` (`media_logic/MediaLogicUnit.cs:96`).
pub fn simulate_hud_queue(ops: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
    let Some(ops) = ops else {
        return Ok(Vec::new());
    };
    let mut now_ticks: i64 = 0;
    let mut queue = HudQueue::default();
    let mut results = Vec::with_capacity(ops.len());

    for op in ops {
        if let Some(adv) = op.get("AdvanceSeconds").and_then(Value::as_f64)
            && adv > 0.0
            && !adv.is_nan()
            && !adv.is_infinite()
        {
            let clamped = adv.min(86400.0 * 365.0);
            now_ticks += (clamped * 10_000_000.0).round() as i64;
        }
        let kind_op = op.get("Op").and_then(Value::as_str).unwrap_or("");
        let shown = match kind_op {
            "offer" => {
                let kind = parse_hud_kind(op.get("Kind").and_then(Value::as_str).unwrap_or(""));
                let expires_ticks = op
                    .get("ExpiresOffsetSeconds")
                    .and_then(Value::as_f64)
                    .filter(|v| !v.is_nan() && !v.is_infinite())
                    .map(|off| {
                        let clamped = off.clamp(-86400.0, 86400.0);
                        now_ticks + (clamped * 10_000_000.0).round() as i64
                    });
                let msg = HudMsg {
                    text: op
                        .get("Text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    value: op.get("Value").and_then(Value::as_str).map(str::to_string),
                    important: op
                        .get("Important")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    kind: kind.to_string(),
                    tag: op.get("Tag").and_then(Value::as_str).map(str::to_string),
                    expires_ticks,
                };
                let can_show = op.get("CanShow").and_then(Value::as_bool).unwrap_or(false);
                queue.offer(msg, can_show)
            }
            "finish" => queue.finish(now_ticks),
            "interrupt" => {
                queue.interrupt();
                None
            }
            "resume" => queue.resume(now_ticks),
            _ => None,
        };
        results.push(json!({
            "ShownText": shown.as_ref().map(|m| m.text.as_str()),
            "ShownValue": shown.as_ref().and_then(|m| m.value.as_deref()),
            "CurrentText": queue.current.as_ref().map(|m| m.text.as_str()),
            "Waiting": queue.waiting.len() as i64,
        }));
    }
    Ok(results)
}

fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn channel(p: f64, q: f64, mut t: f64) -> u8 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    let v = if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    };
    (v.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
}

fn from_hsl(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        channel(p, q, h + 1.0 / 3.0),
        channel(p, q, h),
        channel(p, q, h - 1.0 / 3.0),
    )
}

/// Port of `ExtractAccentColor` (`media_logic/MediaLogicUnit.cs:134`).
pub fn extract_accent_color(bgra_pixels: Option<Vec<i64>>) -> Result<Value, String> {
    let default = json!({ "R": 0x8A, "G": 0xB4, "B": 0xFF });
    let Some(pixels) = bgra_pixels else {
        return Ok(default);
    };
    if pixels.len() < 4 {
        return Ok(default);
    }
    let count = pixels.len() - (pixels.len() % 4);
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let mut total = 0.0;
    for i in (0..count).step_by(4) {
        let pb = (pixels[i] as u8) as f64 / 255.0;
        let pg = (pixels[i + 1] as u8) as f64 / 255.0;
        let pr = (pixels[i + 2] as u8) as f64 / 255.0;
        let (_, s, l) = to_hsl(pr, pg, pb);
        let weight = 0.02 + s * (1.0 - (l - 0.5).abs() * 2.0);
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        total += weight;
    }
    if total <= 0.0 {
        return Ok(default);
    }
    let (hue, mut saturation, mut lightness) = to_hsl(r / total, g / total, b / total);
    if saturation < 0.08 {
        return Ok(default);
    }
    saturation = saturation.max(0.45);
    lightness = lightness.clamp(0.62, 0.74);
    let (rr, gg, bb) = from_hsl(hue, saturation, lightness);
    Ok(json!({ "R": rr, "G": gg, "B": bb }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_prefers_shown_playing() {
        let res = pick_media_session(Some(vec![
            json!({"IsPlaying": true, "IsShown": true, "IsSystemCurrent": false, "IsUntitled": false}),
            json!({"IsPlaying": true, "IsShown": false, "IsSystemCurrent": true, "IsUntitled": false}),
        ]))
        .unwrap();
        assert_eq!(res, 0);
    }
}
