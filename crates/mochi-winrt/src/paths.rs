use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

const SECRET_ENTROPY: &[u8] = b"Mochi.SecretFile.v1";
const LEGACY_SECRET_ENTROPY: &[u8] = b"Yata.SecretFile.v1";

pub struct AppPaths;

impl AppPaths {
    pub fn data_dir() -> PathBuf {
        if let Ok(overridden) =
            std::env::var("MOCHI_DATA_DIR").or_else(|_| std::env::var("YATA_DATA_DIR"))
            && !overridden.trim().is_empty()
        {
            return PathBuf::from(overridden);
        }
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let root = local.join("Mochi");
        if !root.exists() {
            for legacy_name in ["Yata", "Notch"] {
                let legacy = local.join(legacy_name);
                if legacy.exists() {
                    if fs::rename(&legacy, &root).is_err() {
                        return legacy;
                    }
                    break;
                }
            }
        }
        root
    }

    pub fn data_file() -> PathBuf {
        Self::data_dir().join("data.json")
    }

    pub fn google_file() -> PathBuf {
        Self::data_dir().join("google.dat")
    }

    pub fn clipboard_file() -> PathBuf {
        Self::data_dir().join("clipboard.dat")
    }

    pub fn lyrics_dir() -> PathBuf {
        Self::data_dir().join("lyrics")
    }

    pub fn logs_dir() -> PathBuf {
        Self::data_dir().join("logs")
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!(
        "{}.{}.{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("dat"),
        std::process::id(),
        seq
    ));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub struct Log;

impl Log {
    fn write(level: &str, area: &str, message: &str) {
        let lock = LOG_LOCK.get_or_init(|| Mutex::new(()));
        let Ok(_guard) = lock.lock() else {
            return;
        };
        let dir = AppPaths::logs_dir();
        let _ = fs::create_dir_all(&dir);
        let now = Local::now();
        let file = dir.join(format!("mochi-{}.log", now.format("%Y-%m-%d")));
        let line = format!(
            "{} {:<5} [{}] {}\n",
            now.format("%Y-%m-%d %H:%M:%S%.3f"),
            level,
            area,
            message
        );
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn info(area: &str, message: &str) {
        Self::write("INFO", area, message);
    }

    pub fn warn(area: &str, message: &str) {
        Self::write("WARN", area, message);
    }

    pub fn error(area: &str, message: &str) {
        Self::write("ERROR", area, message);
    }
}

pub struct SecretFile;

impl SecretFile {
    pub fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
        let cipher = fs::read(path).ok()?;
        let plain = dpapi_unprotect(&cipher)?;
        serde_json::from_slice(&plain).ok()
    }

    pub fn write<T: Serialize>(path: &Path, value: &T) -> bool {
        let Ok(plain) = serde_json::to_vec(value) else {
            return false;
        };
        let Some(cipher) = dpapi_protect(&plain) else {
            return false;
        };
        atomic_write(path, &cipher).is_ok()
    }

    pub fn delete(path: &Path) {
        let _ = fs::remove_file(path);
    }
}

fn dpapi_protect(plain: &[u8]) -> Option<Vec<u8>> {
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr().cast_mut(),
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: SECRET_ENTROPY.len() as u32,
        pbData: SECRET_ENTROPY.as_ptr().cast_mut(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `in_blob` and `entropy_blob` point to valid slices for the duration of the call;
    // `out_blob.pbData` is freed with `LocalFree`.
    let ok = unsafe {
        CryptProtectData(
            &raw const in_blob,
            None,
            Some(&raw const entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut out_blob,
        )
    };
    if ok.is_err() || out_blob.pbData.is_null() {
        return None;
    }
    // SAFETY: `out_blob.pbData` is valid for `out_blob.cbData` bytes.
    let bytes =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    // SAFETY: `out_blob.pbData` was allocated by `CryptProtectData`.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData.cast())));
    }
    Some(bytes)
}

fn dpapi_unprotect(cipher: &[u8]) -> Option<Vec<u8>> {
    dpapi_unprotect_with_entropy(cipher, SECRET_ENTROPY)
        .or_else(|| dpapi_unprotect_with_entropy(cipher, LEGACY_SECRET_ENTROPY))
}

fn dpapi_unprotect_with_entropy(cipher: &[u8], entropy: &[u8]) -> Option<Vec<u8>> {
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: cipher.len() as u32,
        pbData: cipher.as_ptr().cast_mut(),
    };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr().cast_mut(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `in_blob` and `entropy_blob` point to valid slices for the duration of the call;
    // `out_blob.pbData` is freed with `LocalFree`.
    let ok = unsafe {
        CryptUnprotectData(
            &raw const in_blob,
            None,
            Some(&raw const entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut out_blob,
        )
    };
    if ok.is_err() || out_blob.pbData.is_null() {
        return None;
    }
    // SAFETY: `out_blob.pbData` is valid for `out_blob.cbData` bytes.
    let bytes =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    // SAFETY: `out_blob.pbData` was allocated by `CryptUnprotectData`.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData.cast())));
    }
    Some(bytes)
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

#[derive(Debug, Clone)]
pub struct HudMessage {
    pub glyph: String,
    pub text: String,
    pub value: Option<String>,
    pub level: Option<f64>,
    pub warning: bool,
    pub duration_ms: u32,
    pub important: bool,
    pub kind: HudKind,
    pub celebrate: bool,
    pub tag: Option<String>,
    pub chime: bool,
    pub expires: Option<NaiveDateTime>,
    pub badge: Option<String>,
}

impl HudMessage {
    pub fn new(glyph: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            glyph: glyph.into(),
            text: text.into(),
            value: None,
            level: None,
            warning: false,
            duration_ms: 1800,
            important: false,
            kind: HudKind::Reminder,
            celebrate: false,
            tag: None,
            chime: false,
            expires: None,
            badge: None,
        }
    }
}

pub struct Glyphs;

impl Glyphs {
    pub const VOLUME_0: &str = "\u{E992}";
    pub const VOLUME_1: &str = "\u{E993}";
    pub const VOLUME_2: &str = "\u{E994}";
    pub const VOLUME_3: &str = "\u{E995}";
    pub const MUTE: &str = "\u{E74F}";
    pub const BRIGHTNESS: &str = "\u{E706}";
    pub const BATTERY: &str = "\u{E83F}";
    pub const BATTERY_LOW: &str = "\u{E855}";
    pub const CHARGING: &str = "\u{E83E}";
    pub const HEADPHONES: &str = "\u{E7F6}";
    pub const SPEAKERS: &str = "\u{E7F5}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const WIFI: &str = "\u{E701}";
    pub const ETHERNET: &str = "\u{E839}";
    pub const OFFLINE: &str = "\u{F384}";
    pub const LOCK: &str = "\u{E72E}";
    pub const TIMER: &str = "\u{E916}";
    pub const WATER: &str = "\u{EB42}";
    pub const CAMERA: &str = "\u{E722}";
    pub const MICROPHONE: &str = "\u{E720}";
    pub const MICROPHONE_OFF: &str = "\u{EC54}";
    pub const BELL: &str = "\u{EA8F}";
    pub const CALENDAR: &str = "\u{E787}";
    pub const CLOUD: &str = "\u{E753}";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SettingsData {
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default = "d_true")]
    pub hide_in_fullscreen: bool,
    #[serde(default = "d_true")]
    pub hide_from_capture: bool,
    #[serde(default = "d_140")]
    pub hover_delay_ms: i32,
    #[serde(default)]
    pub calm_motion: bool,
    #[serde(default = "d_true")]
    pub use_artwork_accent: bool,
    #[serde(default)]
    pub monitor: Option<String>,
    #[serde(default = "d_true")]
    pub hotkey_enabled: bool,
    #[serde(default)]
    pub jump_hotkey_enabled: bool,
    #[serde(default)]
    pub mic_hotkey_enabled: bool,
    #[serde(default = "d_true")]
    pub lyrics_enabled: bool,
    #[serde(default)]
    pub update_check_enabled: bool,
    #[serde(default = "d_5")]
    pub calendar_reminder_minutes: i32,
    #[serde(default = "d_true")]
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
    #[serde(default = "d_true")]
    pub weather_rain_alerts: bool,
    #[serde(default = "d_true")]
    pub pray_alerts_enabled: bool,
    #[serde(default = "d_10")]
    pub pray_pre_alert_minutes: i32,
    #[serde(default = "d_kemenag")]
    pub pray_method: String,
    #[serde(default)]
    pub pray_muted: Vec<String>,
    #[serde(default)]
    pub pray_place: Option<String>,
    #[serde(default)]
    pub pray_latitude: Option<f64>,
    #[serde(default)]
    pub pray_longitude: Option<f64>,
    #[serde(default = "d_true")]
    pub sounds_enabled: bool,
    #[serde(default = "d_true")]
    pub quiet_during_calls: bool,
    #[serde(default = "d_true")]
    pub hud_volume: bool,
    #[serde(default = "d_true")]
    pub hud_brightness: bool,
    #[serde(default = "d_true")]
    pub hud_power: bool,
    #[serde(default = "d_true")]
    pub hud_audio_device: bool,
    #[serde(default = "d_true")]
    pub hud_caps_lock: bool,
    #[serde(default = "d_true")]
    pub hud_keyboard_layout: bool,
    #[serde(default = "d_true")]
    pub hud_network: bool,
    #[serde(default = "d_true")]
    pub hud_privacy: bool,
    #[serde(default = "d_true")]
    pub activities_enabled: bool,
    #[serde(default)]
    pub copilot_hooks_initialized: bool,
    #[serde(default = "d_25")]
    pub focus_minutes: i32,
    #[serde(default = "d_5")]
    pub short_break_minutes: i32,
    #[serde(default = "d_15")]
    pub long_break_minutes: i32,
    #[serde(default = "d_4")]
    pub sessions_per_cycle: i32,
    #[serde(default = "d_true")]
    pub pause_focus_when_away: bool,
    #[serde(default = "d_120")]
    pub focus_goal_minutes: i32,
    #[serde(default = "d_true")]
    pub companion_enabled: bool,
    #[serde(default = "d_2")]
    pub companion_idle_minutes: i32,
    #[serde(default = "d_10")]
    pub companion_sleep_minutes: i32,
    #[serde(default = "d_true")]
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
    #[serde(default = "d_2000")]
    pub goal_ml: i32,
    #[serde(default = "d_250")]
    pub cup_ml: i32,
    #[serde(default = "d_60")]
    pub reminder_minutes: i32,
    #[serde(default)]
    pub reminders_enabled: bool,
    #[serde(default)]
    pub last_drink: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
    pub days: BTreeMap<String, i32>,
    #[serde(default)]
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskData {
    #[serde(default = "d_guid")]
    pub id: String,
    #[serde(default)]
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
pub struct UpdateData {
    #[serde(default)]
    pub last_check: Option<String>,
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub announced: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StoreData {
    #[serde(default = "d_1")]
    pub schema_version: i32,
    #[serde(default)]
    pub settings: SettingsData,
    #[serde(default)]
    pub water: WaterData,
    #[serde(default)]
    pub focus: FocusData,
    #[serde(default)]
    pub tasks: Vec<TaskData>,
    #[serde(default)]
    pub task_deletes: Vec<String>,
    #[serde(default)]
    pub shelf: Vec<String>,
    #[serde(default)]
    pub clipboard: Vec<ClipboardEntryData>,
    #[serde(default)]
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

fn d_true() -> bool {
    true
}
fn d_1() -> i32 {
    1
}
fn d_2() -> i32 {
    2
}
fn d_4() -> i32 {
    4
}
fn d_5() -> i32 {
    5
}
fn d_10() -> i32 {
    10
}
fn d_15() -> i32 {
    15
}
fn d_25() -> i32 {
    25
}
fn d_60() -> i32 {
    60
}
fn d_120() -> i32 {
    120
}
fn d_140() -> i32 {
    140
}
fn d_250() -> i32 {
    250
}
fn d_2000() -> i32 {
    2000
}
fn d_guid() -> String {
    "00000000-0000-0000-0000-000000000000".to_string()
}
fn d_kemenag() -> String {
    "Kemenag".to_string()
}

pub struct AppStore {
    path: PathBuf,
    pub data: StoreData,
    pub recovered_from_damage: bool,
    pub is_new: bool,
    read_only: bool,
}

impl AppStore {
    pub fn load() -> Self {
        Self::load_from(AppPaths::data_file())
    }

    pub fn in_memory() -> Self {
        Self {
            path: PathBuf::new(),
            data: StoreData::default(),
            recovered_from_damage: false,
            is_new: true,
            read_only: true,
        }
    }

    pub fn load_from(path: PathBuf) -> Self {
        if !path.exists() {
            return Self {
                path,
                data: StoreData::default(),
                recovered_from_damage: false,
                is_new: true,
                read_only: false,
            };
        }
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let raw = raw.trim_start_matches('\u{FEFF}').to_string();
                if serde_json::from_str::<serde_json::Value>(&raw).is_err() {
                    let _ = fs::rename(&path, path.with_extension("json.bad"));
                    return Self {
                        path,
                        data: StoreData::default(),
                        recovered_from_damage: true,
                        is_new: false,
                        read_only: false,
                    };
                }
                let norm = mochi_store::port::normalize_store_json(raw)
                    .ok()
                    .and_then(|v| {
                        v.get("NormalizedJson")
                            .and_then(|s| s.as_str())
                            .map(str::to_string)
                    });
                let data = norm
                    .and_then(|s| serde_json::from_str::<StoreData>(&s).ok())
                    .unwrap_or_default();
                Self {
                    path,
                    data,
                    recovered_from_damage: false,
                    is_new: false,
                    read_only: false,
                }
            }
            Err(_) => Self {
                path,
                data: StoreData::default(),
                recovered_from_damage: false,
                is_new: false,
                read_only: true,
            },
        }
    }

    pub fn save_now(&self) {
        if self.read_only {
            return;
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            let _ = atomic_write(&self.path, json.as_bytes());
        }
    }
}
