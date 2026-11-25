//! Pure store JSON schema normalization and clamping rules.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SettingsData {
    #[serde(default)]
    language: Option<String>,
    #[serde(default = "default_true")]
    hide_in_fullscreen: bool,
    #[serde(default = "default_true")]
    hide_from_capture: bool,
    #[serde(default = "default_140")]
    hover_delay_ms: i32,
    #[serde(default)]
    calm_motion: bool,
    #[serde(default = "default_true")]
    use_artwork_accent: bool,
    #[serde(default)]
    monitor: Option<String>,
    #[serde(default = "default_true")]
    hotkey_enabled: bool,
    #[serde(default)]
    jump_hotkey_enabled: bool,
    #[serde(default)]
    mic_hotkey_enabled: bool,
    #[serde(default = "default_true")]
    lyrics_enabled: bool,
    #[serde(default)]
    update_check_enabled: bool,
    #[serde(default = "default_5")]
    calendar_reminder_minutes: i32,
    #[serde(default = "default_true")]
    calendar_in_compact: bool,
    #[serde(default)]
    calendar_ids: Option<Vec<String>>,
    #[serde(default)]
    tasks_sync_enabled: bool,
    #[serde(default)]
    tasks_list_id: Option<String>,
    #[serde(default)]
    tasks_list_name: Option<String>,
    #[serde(default)]
    weather_enabled: bool,
    #[serde(default)]
    weather_place: Option<String>,
    #[serde(default)]
    weather_latitude: Option<f64>,
    #[serde(default)]
    weather_longitude: Option<f64>,
    #[serde(default = "default_true")]
    weather_rain_alerts: bool,
    #[serde(default = "default_true")]
    pray_alerts_enabled: bool,
    #[serde(default = "default_10")]
    pray_pre_alert_minutes: i32,
    #[serde(default = "default_kemenag")]
    pray_method: String,
    #[serde(default)]
    pray_muted: Vec<String>,
    #[serde(default)]
    pray_place: Option<String>,
    #[serde(default)]
    pray_latitude: Option<f64>,
    #[serde(default)]
    pray_longitude: Option<f64>,
    #[serde(default = "default_true")]
    sounds_enabled: bool,
    #[serde(default = "default_true")]
    quiet_during_calls: bool,
    #[serde(default = "default_true")]
    hud_volume: bool,
    #[serde(default = "default_true")]
    hud_brightness: bool,
    #[serde(default = "default_true")]
    hud_power: bool,
    #[serde(default = "default_true")]
    hud_audio_device: bool,
    #[serde(default = "default_true")]
    hud_caps_lock: bool,
    #[serde(default = "default_true")]
    hud_keyboard_layout: bool,
    #[serde(default = "default_true")]
    hud_network: bool,
    #[serde(default = "default_true")]
    hud_privacy: bool,
    #[serde(default = "default_true")]
    activities_enabled: bool,
    #[serde(default)]
    copilot_hooks_initialized: bool,
    #[serde(default = "default_25")]
    focus_minutes: i32,
    #[serde(default = "default_5")]
    short_break_minutes: i32,
    #[serde(default = "default_15")]
    long_break_minutes: i32,
    #[serde(default = "default_4")]
    sessions_per_cycle: i32,
    #[serde(default = "default_true")]
    pause_focus_when_away: bool,
    #[serde(default = "default_120")]
    focus_goal_minutes: i32,
    #[serde(default = "default_true")]
    companion_enabled: bool,
    #[serde(default = "default_2")]
    companion_idle_minutes: i32,
    #[serde(default = "default_10")]
    companion_sleep_minutes: i32,
    #[serde(default = "default_true")]
    away_summary_enabled: bool,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            language: None,
            hide_in_fullscreen: true,
            hide_from_capture: true,
            hover_delay_ms: 140,
            calm_motion: false,
            use_artwork_accent: true,
            monitor: None,
            hotkey_enabled: true,
            jump_hotkey_enabled: false,
            mic_hotkey_enabled: false,
            lyrics_enabled: true,
            update_check_enabled: false,
            calendar_reminder_minutes: 5,
            calendar_in_compact: true,
            calendar_ids: None,
            tasks_sync_enabled: false,
            tasks_list_id: None,
            tasks_list_name: None,
            weather_enabled: false,
            weather_place: None,
            weather_latitude: None,
            weather_longitude: None,
            weather_rain_alerts: true,
            pray_alerts_enabled: true,
            pray_pre_alert_minutes: 10,
            pray_method: "Kemenag".to_string(),
            pray_muted: Vec::new(),
            pray_place: None,
            pray_latitude: None,
            pray_longitude: None,
            sounds_enabled: true,
            quiet_during_calls: true,
            hud_volume: true,
            hud_brightness: true,
            hud_power: true,
            hud_audio_device: true,
            hud_caps_lock: true,
            hud_keyboard_layout: true,
            hud_network: true,
            hud_privacy: true,
            activities_enabled: true,
            copilot_hooks_initialized: false,
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            sessions_per_cycle: 4,
            pause_focus_when_away: true,
            focus_goal_minutes: 120,
            companion_enabled: true,
            companion_idle_minutes: 2,
            companion_sleep_minutes: 10,
            away_summary_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WaterData {
    #[serde(default = "default_2000")]
    goal_ml: i32,
    #[serde(default = "default_250")]
    cup_ml: i32,
    #[serde(default = "default_60")]
    reminder_minutes: i32,
    #[serde(default)]
    reminders_enabled: bool,
    #[serde(default)]
    last_drink: Option<String>,
    #[serde(default)]
    days: Option<BTreeMap<String, i32>>,
}

impl Default for WaterData {
    fn default() -> Self {
        Self {
            goal_ml: 2000,
            cup_ml: 250,
            reminder_minutes: 60,
            reminders_enabled: false,
            last_drink: None,
            days: Some(BTreeMap::new()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct FocusData {
    #[serde(default)]
    days: Option<BTreeMap<String, i32>>,
    #[serde(default)]
    task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskData {
    #[serde(default = "default_guid")]
    id: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    due: Option<String>,
    #[serde(default)]
    due_all_day: bool,
    #[serde(default)]
    reminded: bool,
    #[serde(default)]
    google_id: Option<String>,
    #[serde(default)]
    dirty: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ClipboardEntryData {
    #[serde(default)]
    text: String,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UpdateData {
    #[serde(default)]
    last_check: Option<String>,
    #[serde(default)]
    latest: Option<String>,
    #[serde(default)]
    announced: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StoreData {
    #[serde(default = "default_1")]
    schema_version: i32,
    #[serde(default = "default_some_settings")]
    settings: Option<SettingsData>,
    #[serde(default = "default_some_water")]
    water: Option<WaterData>,
    #[serde(default = "default_some_focus")]
    focus: Option<FocusData>,
    #[serde(default = "default_some_vec")]
    tasks: Option<Vec<Option<TaskData>>>,
    #[serde(default = "default_some_vec")]
    task_deletes: Option<Vec<Option<String>>>,
    #[serde(default = "default_some_vec")]
    shelf: Option<Vec<Option<String>>>,
    #[serde(default = "default_some_vec")]
    clipboard: Option<Vec<Option<ClipboardEntryData>>>,
    #[serde(default = "default_some_updates")]
    updates: Option<UpdateData>,
}

fn default_some_settings() -> Option<SettingsData> {
    Some(SettingsData::default())
}
fn default_some_water() -> Option<WaterData> {
    Some(WaterData::default())
}
fn default_some_focus() -> Option<FocusData> {
    Some(FocusData {
        days: Some(BTreeMap::new()),
        task_id: None,
    })
}
fn default_some_vec<T>() -> Option<Vec<T>> {
    Some(Vec::new())
}
fn default_some_updates() -> Option<UpdateData> {
    Some(UpdateData::default())
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            schema_version: 1,
            settings: Some(SettingsData::default()),
            water: Some(WaterData::default()),
            focus: Some(FocusData {
                days: Some(BTreeMap::new()),
                task_id: None,
            }),
            tasks: Some(Vec::new()),
            task_deletes: Some(Vec::new()),
            shelf: Some(Vec::new()),
            clipboard: Some(Vec::new()),
            updates: Some(UpdateData::default()),
        }
    }
}

fn default_true() -> bool {
    true
}
fn default_1() -> i32 {
    1
}
fn default_2() -> i32 {
    2
}
fn default_4() -> i32 {
    4
}
fn default_5() -> i32 {
    5
}
fn default_10() -> i32 {
    10
}
fn default_15() -> i32 {
    15
}
fn default_25() -> i32 {
    25
}
fn default_60() -> i32 {
    60
}
fn default_120() -> i32 {
    120
}
fn default_140() -> i32 {
    140
}
fn default_250() -> i32 {
    250
}
fn default_2000() -> i32 {
    2000
}
fn default_guid() -> String {
    "00000000-0000-0000-0000-000000000000".to_string()
}
fn default_kemenag() -> String {
    "Kemenag".to_string()
}

pub fn normalize_store_json(json_str: String) -> Result<Value, String> {
    let mut data: StoreData = serde_json::from_str(&json_str).unwrap_or_default();
    if data.schema_version < 1 {
        data.schema_version = 1;
    }
    let before = serde_json::to_string(&data).unwrap_or_default();

    let mut s = data.settings.unwrap_or_default();
    s.language = match s.language.as_deref() {
        Some("en") => Some("en".to_string()),
        Some("id") | Some("id-ID") => Some("id".to_string()),
        _ => None,
    };
    s.hover_delay_ms = s.hover_delay_ms.clamp(0, 1000);
    s.calendar_reminder_minutes = s.calendar_reminder_minutes.clamp(0, 30);
    s.focus_minutes = s.focus_minutes.clamp(5, 120);
    s.short_break_minutes = s.short_break_minutes.clamp(1, 30);
    s.long_break_minutes = s.long_break_minutes.clamp(5, 60);
    s.sessions_per_cycle = s.sessions_per_cycle.clamp(2, 8);
    s.focus_goal_minutes = s.focus_goal_minutes.clamp(15, 600);
    s.companion_idle_minutes = s.companion_idle_minutes.clamp(1, 30);
    s.companion_sleep_minutes = s.companion_sleep_minutes.clamp(2, 120);
    s.pray_pre_alert_minutes = s.pray_pre_alert_minutes.clamp(0, 60);
    data.settings = Some(s);

    let mut w = data.water.unwrap_or_default();
    if w.days.is_none() {
        w.days = Some(BTreeMap::new());
    }
    w.goal_ml = w.goal_ml.clamp(500, 5000);
    w.cup_ml = w.cup_ml.clamp(50, 1000);
    w.reminder_minutes = w.reminder_minutes.clamp(15, 240);
    data.water = Some(w);

    let mut f = data.focus.unwrap_or_default();
    if f.days.is_none() {
        f.days = Some(BTreeMap::new());
    }
    data.focus = Some(f);

    if data.updates.is_none() {
        data.updates = Some(UpdateData::default());
    }

    let tasks: Vec<Option<TaskData>> = data
        .tasks
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(|mut t| {
            if t.text.is_none() {
                t.text = Some(String::new());
            }
            Some(t)
        })
        .collect();
    data.tasks = Some(tasks);

    let deletes: Vec<Option<String>> = data
        .task_deletes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .filter(|id| !id.is_empty())
        .map(Some)
        .collect();
    data.task_deletes = Some(deletes);

    let shelf: Vec<Option<String>> = data
        .shelf
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .filter(|p| !p.trim().is_empty())
        .map(Some)
        .collect();
    data.shelf = Some(shelf);

    let clipboard: Vec<Option<ClipboardEntryData>> = data
        .clipboard
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .filter(|c| !c.text.trim().is_empty())
        .map(Some)
        .collect();
    data.clipboard = Some(clipboard);

    let after = serde_json::to_string(&data).unwrap_or_default();
    Ok(json!({
        "Changed": after != before,
        "NormalizedJson": after,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range_settings_and_preserves_prayer_and_clipboard() {
        let raw = r#"{"Settings":{"Language":"pt-BR","HoverDelayMs":9999,"PrayMethod":"MWL"},"Clipboard":[{"Text":"hi","Pinned":true,"At":"2026-11-24T10:00:00"}]}"#;
        let res = normalize_store_json(raw.to_string()).unwrap();
        let norm: Value = serde_json::from_str(res["NormalizedJson"].as_str().unwrap()).unwrap();
        assert!(norm["Settings"]["Language"].is_null());
        assert_eq!(norm["Settings"]["HoverDelayMs"], 1000);
        assert_eq!(norm["Settings"]["PrayMethod"], "MWL");
        assert_eq!(norm["Clipboard"][0]["Text"], "hi");
    }
}
