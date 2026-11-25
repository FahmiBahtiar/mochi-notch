use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use chrono::{Local, NaiveDateTime};
pub use mochi_store::port::{
    ClipboardEntryData, FocusData, HudKind, SettingsData, StoreData, TaskData, UpdateData,
    WaterData,
};
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

    pub fn updates_dir() -> PathBuf {
        Self::data_dir().join("updates")
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
    pub const DOWNLOAD: &str = "\u{E896}";
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
                let raw = raw.trim_start_matches('\u{FEFF}');
                let data = match mochi_store::port::parse_and_normalize_store(raw) {
                    Ok(d) => d,
                    Err(_) => {
                        if serde_json::from_str::<serde_json::Value>(raw).is_err() {
                            let _ = fs::rename(&path, path.with_extension("json.bad"));
                            return Self {
                                path,
                                data: StoreData::default(),
                                recovered_from_damage: true,
                                is_new: false,
                                read_only: false,
                            };
                        }
                        StoreData::default()
                    }
                };
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
