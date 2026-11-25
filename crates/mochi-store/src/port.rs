//! Pure store schema, deserialization resilience, and clamping normalization rules.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn vec_skip_nulls<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::<Vec<Option<T>>>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default().into_iter().flatten().collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HudKind {
    Reminder,
    Volume,
    Brightness,
    Power,
    AudioDevice,
    CapsLock,
    KeyboardLayout,
    Network,
    Privacy,
    Weather,
    Activity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SettingsData {
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "default_true")]
    pub hide_in_fullscreen: bool,
    #[serde(default = "default_true")]
    pub hide_from_capture: bool,
    #[serde(default = "default_140")]
    pub hover_delay_ms: i32,
    #[serde(default)]
    pub calm_motion: bool,
    #[serde(default = "default_true")]
    pub use_artwork_accent: bool,
    #[serde(default)]
    pub monitor: Option<String>,
    #[serde(default = "default_true")]
    pub hotkey_enabled: bool,
    #[serde(default)]
    pub jump_hotkey_enabled: bool,
    #[serde(default)]
    pub mic_hotkey_enabled: bool,
    #[serde(default = "default_true")]
    pub lyrics_enabled: bool,
    #[serde(default)]
    pub update_check_enabled: bool,
    #[serde(default)]
    pub auto_update_enabled: bool,
    #[serde(default = "default_5")]
    pub calendar_reminder_minutes: i32,
    #[serde(default = "default_true")]
    pub calendar_in_compact: bool,
    #[serde(default)]
    pub calendar_ids: Option<Vec<String>>,
    #[serde(default)]
    pub tasks_sync_enabled: bool,
    #[serde(default)]
    pub tasks_list_id: Option<String>,
    #[serde(default)]
    pub tasks_list_name: Option<String>,
    #[serde(default)]
    pub weather_enabled: bool,
    #[serde(default)]
    pub weather_place: Option<String>,
    #[serde(default)]
    pub weather_latitude: Option<f64>,
    #[serde(default)]
    pub weather_longitude: Option<f64>,
    #[serde(default = "default_true")]
    pub weather_rain_alerts: bool,
    #[serde(default = "default_true")]
    pub pray_alerts_enabled: bool,
    #[serde(default = "default_10")]
    pub pray_pre_alert_minutes: i32,
    #[serde(default = "default_kemenag")]
    pub pray_method: String,
    #[serde(default)]
    pub pray_muted: Vec<String>,
    #[serde(default)]
    pub pray_place: Option<String>,
    #[serde(default)]
    pub pray_latitude: Option<f64>,
    #[serde(default)]
    pub pray_longitude: Option<f64>,
    #[serde(default = "default_true")]
    pub sounds_enabled: bool,
    #[serde(default = "default_true")]
    pub quiet_during_calls: bool,
    #[serde(default = "default_true")]
    pub hud_volume: bool,
    #[serde(default = "default_true")]
    pub hud_brightness: bool,
    #[serde(default = "default_true")]
    pub hud_power: bool,
    #[serde(default = "default_true")]
    pub hud_audio_device: bool,
    #[serde(default = "default_true")]
    pub hud_caps_lock: bool,
    #[serde(default = "default_true")]
    pub hud_keyboard_layout: bool,
    #[serde(default = "default_true")]
    pub hud_network: bool,
    #[serde(default = "default_true")]
    pub hud_privacy: bool,
    #[serde(default = "default_true")]
    pub activities_enabled: bool,
    #[serde(default)]
    pub copilot_hooks_initialized: bool,
    #[serde(default = "default_25")]
    pub focus_minutes: i32,
    #[serde(default = "default_5")]
    pub short_break_minutes: i32,
    #[serde(default = "default_15")]
    pub long_break_minutes: i32,
    #[serde(default = "default_4")]
    pub sessions_per_cycle: i32,
    #[serde(default = "default_true")]
    pub pause_focus_when_away: bool,
    #[serde(default = "default_120")]
    pub focus_goal_minutes: i32,
    #[serde(default = "default_true")]
    pub companion_enabled: bool,
    #[serde(default = "default_2")]
    pub companion_idle_minutes: i32,
    #[serde(default = "default_10")]
    pub companion_sleep_minutes: i32,
    #[serde(default = "default_true")]
    pub away_summary_enabled: bool,
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
            auto_update_enabled: false,
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

impl SettingsData {
    pub fn shows(&self, kind: HudKind) -> bool {
        match kind {
            HudKind::Volume => self.hud_volume,
            HudKind::Brightness => self.hud_brightness,
            HudKind::Power => self.hud_power,
            HudKind::AudioDevice => self.hud_audio_device,
            HudKind::CapsLock => self.hud_caps_lock,
            HudKind::KeyboardLayout => self.hud_keyboard_layout,
            HudKind::Network => self.hud_network,
            HudKind::Privacy => self.hud_privacy,
            HudKind::Activity => self.activities_enabled,
            HudKind::Reminder | HudKind::Weather => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct WaterData {
    #[serde(default = "default_2000")]
    pub goal_ml: i32,
    #[serde(default = "default_250")]
    pub cup_ml: i32,
    #[serde(default = "default_60")]
    pub reminder_minutes: i32,
    #[serde(default)]
    pub reminders_enabled: bool,
    #[serde(default)]
    pub last_drink: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub days: BTreeMap<String, i32>,
}

impl Default for WaterData {
    fn default() -> Self {
        Self {
            goal_ml: 2000,
            cup_ml: 250,
            reminder_minutes: 60,
            reminders_enabled: false,
            last_drink: None,
            days: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FocusData {
    #[serde(default, deserialize_with = "null_as_default")]
    pub days: BTreeMap<String, i32>,
    #[serde(default)]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskData {
    #[serde(default = "default_guid")]
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub text: String,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub due_all_day: bool,
    #[serde(default)]
    pub reminded: bool,
    #[serde(default)]
    pub google_id: Option<String>,
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ClipboardEntryData {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateData {
    #[serde(default)]
    pub last_check: Option<String>,
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub announced: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StoreData {
    #[serde(default = "default_1")]
    pub schema_version: i32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub settings: SettingsData,
    #[serde(default, deserialize_with = "null_as_default")]
    pub water: WaterData,
    #[serde(default, deserialize_with = "null_as_default")]
    pub focus: FocusData,
    #[serde(default, deserialize_with = "vec_skip_nulls")]
    pub tasks: Vec<TaskData>,
    #[serde(default, deserialize_with = "vec_skip_nulls")]
    pub task_deletes: Vec<String>,
    #[serde(default, deserialize_with = "vec_skip_nulls")]
    pub shelf: Vec<String>,
    #[serde(default, deserialize_with = "vec_skip_nulls")]
    pub clipboard: Vec<ClipboardEntryData>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub updates: UpdateData,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            schema_version: 1,
            settings: SettingsData::default(),
            water: WaterData::default(),
            focus: FocusData::default(),
            tasks: Vec::new(),
            task_deletes: Vec::new(),
            shelf: Vec::new(),
            clipboard: Vec::new(),
            updates: UpdateData::default(),
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

pub fn normalize_store_data(data: &mut StoreData) {
    if data.schema_version < 1 {
        data.schema_version = 1;
    }

    let s = &mut data.settings;
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

    let w = &mut data.water;
    w.goal_ml = w.goal_ml.clamp(500, 5000);
    w.cup_ml = w.cup_ml.clamp(50, 1000);
    w.reminder_minutes = w.reminder_minutes.clamp(15, 240);

    data.task_deletes.retain(|id| !id.is_empty());
    data.shelf.retain(|p| !p.trim().is_empty());
    data.clipboard.retain(|c| !c.text.trim().is_empty());
}

pub fn parse_and_normalize_store(json_str: &str) -> Result<StoreData, serde_json::Error> {
    let mut data: StoreData = serde_json::from_str(json_str)?;
    normalize_store_data(&mut data);
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_out_of_range_settings_and_preserves_prayer_and_clipboard() {
        let raw = r#"{"Settings":{"Language":"pt-BR","HoverDelayMs":9999,"PrayMethod":"MWL"},"Tasks":[null,{"Text":null}],"Clipboard":[{"Text":"hi","Pinned":true,"At":"2026-11-24T10:00:00"}]}"#;
        let data = parse_and_normalize_store(raw).unwrap();
        assert!(data.settings.language.is_none());
        assert_eq!(data.settings.hover_delay_ms, 1000);
        assert_eq!(data.settings.pray_method, "MWL");
        assert_eq!(data.tasks.len(), 1);
        assert_eq!(data.tasks[0].text, "");
        assert_eq!(data.clipboard.len(), 1);
        assert_eq!(data.clipboard[0].text, "hi");
    }
}
