use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, RECT, WAIT_OBJECT_0};
use windows::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SNATIVELANGUAGENAME};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoSetProxyBlanket, EOLE_AUTHENTICATION_CAPABILITIES, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
    RegCloseKey, RegEnumKeyExW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW,
};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, WaitForMultipleObjects};
use windows::Win32::System::Variant::{VARIANT, VariantClear};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
    WBEM_GENERIC_FLAG_TYPE, WbemLocator,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClassNameW, GetDesktopWindow, GetForegroundWindow, GetShellWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsZoomed, WS_CAPTION,
};
use windows_core::{BSTR, PCWSTR, PWSTR};

use crate::audio::file_description;
use crate::monitors::Monitors;
use crate::native::to_wide;
use crate::paths::{Glyphs, HudKind, HudMessage};
use crate::strings::Language;

pub fn create_single_instance_mutex() -> Option<HANDLE> {
    let name = to_wide("Mochi.SingleInstance");
    // SAFETY: `name` is NUL-terminated UTF-16.
    let h = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.ok()?;
    // SAFETY: GetLastError reads thread-local Win32 error code.
    if unsafe { GetLastError() }.0 == 183 {
        // ERROR_ALREADY_EXISTS
        // SAFETY: `h` is a valid handle.
        unsafe {
            let _ = CloseHandle(h);
        }
        None
    } else {
        Some(h)
    }
}

pub fn windows_wants_quiet() -> bool {
    // SAFETY: SHQueryUserNotificationState takes no pointers other than stack out param.
    if let Ok(state) = unsafe { SHQueryUserNotificationState() } {
        state == QUNS_BUSY || state == QUNS_QUIET_TIME
    } else {
        false
    }
}

pub fn detect_fullscreen(
    own_hwnd: windows::Win32::Foundation::HWND,
    monitor_device: Option<&str>,
    lang: Language,
) -> bool {
    // SAFETY: SHQueryUserNotificationState is a safe Win32 call.
    if let Ok(state) = unsafe { SHQueryUserNotificationState() }
        && (state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE)
    {
        return true;
    }
    // SAFETY: Win32 window queries accept any HWND safely.
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || fg == own_hwnd || fg == GetShellWindow() || fg == GetDesktopWindow() {
            return false;
        }
        let style = GetWindowLongPtrW(fg, GWL_STYLE) as u32;
        if IsZoomed(fg).as_bool() && (style & WS_CAPTION.0) == WS_CAPTION.0 {
            return false;
        }
        let mut cls = [0u16; 256];
        let len = GetClassNameW(fg, &mut cls) as usize;
        let class_name = String::from_utf16_lossy(&cls[..len]);
        if matches!(class_name.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let fg_mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let own_mon = MonitorFromWindow(own_hwnd, MONITOR_DEFAULTTONEAREST);
        if fg_mon != own_mon {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(fg, &raw mut rect).is_err() {
            return false;
        }
        let disp = Monitors::resolve(monitor_device, lang);
        rect.left <= disp.x
            && rect.top <= disp.y
            && rect.right >= disp.x + disp.width
            && rect.bottom >= disp.y + disp.height
    }
}

pub struct KeyboardWatcher {
    caps_lock: bool,
    last_window: isize,
    last_layout: isize,
}

impl KeyboardWatcher {
    pub fn new() -> Self {
        Self {
            caps_lock: Self::is_caps_on(),
            last_window: 0,
            last_layout: 0,
        }
    }

    fn is_caps_on() -> bool {
        // SAFETY: GetKeyState(VK_CAPITAL = 0x14) has no preconditions.
        (unsafe { GetKeyState(0x14) } & 1) != 0
    }

    pub fn poll(&mut self, lang: Language) -> Vec<HudMessage> {
        let mut out = Vec::new();
        let s = lang.strings();
        let caps = Self::is_caps_on();
        if caps != self.caps_lock {
            self.caps_lock = caps;
            let mut msg = if caps {
                HudMessage::new(Glyphs::LOCK, s.hud_caps_lock_on)
            } else {
                HudMessage::new(Glyphs::KEYBOARD, s.hud_caps_lock_off)
            };
            msg.kind = HudKind::CapsLock;
            out.push(msg);
        }

        // SAFETY: GetForegroundWindow, GetWindowThreadProcessId, GetKeyboardLayout are safe Win32 calls.
        let (win, layout) = unsafe {
            let fg = GetForegroundWindow();
            let tid = GetWindowThreadProcessId(fg, None);
            let hkl = GetKeyboardLayout(tid);
            (fg.0 as isize, hkl.0 as isize)
        };
        if win == self.last_window
            && layout != self.last_layout
            && layout != 0
            && self.last_layout != 0
        {
            let (name, code) = describe_layout(layout, lang);
            let mut msg = HudMessage::new(Glyphs::KEYBOARD, name);
            msg.value = Some(code);
            msg.kind = HudKind::KeyboardLayout;
            out.push(msg);
        }
        self.last_window = win;
        self.last_layout = layout;
        out
    }
}

impl Default for KeyboardWatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn describe_layout(layout: isize, lang: Language) -> (String, String) {
    let lang_id = (layout & 0xFFFF) as u16;
    let tag = match lang_id & 0x03FF {
        0x09 => "en-US",
        0x21 => "id-ID",
        _ => "",
    };
    if !tag.is_empty() {
        let wtag = to_wide(tag);
        let mut buf = [0u16; 128];
        // SAFETY: `wtag` is NUL-terminated UTF-16 and `buf` has 128 u16 slots.
        let len = unsafe {
            GetLocaleInfoEx(
                PCWSTR(wtag.as_ptr()),
                LOCALE_SNATIVELANGUAGENAME,
                Some(&mut buf),
            )
        };
        if len > 1 {
            let raw = String::from_utf16_lossy(&buf[..(len as usize - 1)]);
            let mut chars = raw.chars();
            let cap = match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => raw,
            };
            return (cap, tag.to_ascii_uppercase());
        }
    }
    (
        lang.strings().hud_keyboard.to_string(),
        format!("{lang_id:04X}"),
    )
}

pub struct PowerWatcher {
    has_battery: bool,
    ac_online: u8,
    last_warning: i32,
}

impl PowerWatcher {
    pub fn new() -> Self {
        let mut st = SYSTEM_POWER_STATUS::default();
        // SAFETY: `st` is a valid stack local.
        let ok = unsafe { GetSystemPowerStatus(&raw mut st) }.is_ok();
        let has_battery = ok && (st.BatteryFlag & 128) == 0 && st.BatteryFlag != 255;
        Self {
            has_battery,
            ac_online: st.ACLineStatus,
            last_warning: i32::MAX,
        }
    }

    pub fn poll(&mut self, lang: Language) -> Option<HudMessage> {
        if !self.has_battery {
            return None;
        }
        let mut st = SYSTEM_POWER_STATUS::default();
        // SAFETY: `st` is a valid stack local.
        if unsafe { GetSystemPowerStatus(&raw mut st) }.is_err() {
            return None;
        }
        if st.BatteryLifePercent > 100 {
            return None;
        }
        let percent = i32::from(st.BatteryLifePercent);
        let fraction = f64::from(percent) / 100.0;
        let s = lang.strings();

        if st.ACLineStatus != self.ac_online && st.ACLineStatus != 255 {
            self.ac_online = st.ACLineStatus;
            if self.ac_online == 1 {
                self.last_warning = i32::MAX;
                let mut m = HudMessage::new(Glyphs::CHARGING, s.hud_charging);
                m.value = Some(format!("{percent}%"));
                m.level = Some(fraction);
                m.kind = HudKind::Power;
                return Some(m);
            } else {
                let mut m = HudMessage::new(Glyphs::BATTERY, s.hud_on_battery);
                m.value = Some(format!("{percent}%"));
                m.level = Some(fraction);
                m.kind = HudKind::Power;
                return Some(m);
            }
        }

        if self.ac_online != 0 {
            return None;
        }
        for threshold in [20, 10] {
            if percent <= threshold && self.last_warning > threshold {
                self.last_warning = threshold;
                let mut m = HudMessage::new(
                    Glyphs::BATTERY_LOW,
                    if threshold <= 10 {
                        s.hud_battery_very_low
                    } else {
                        s.hud_battery_low
                    },
                );
                m.value = Some(format!("{percent}%"));
                m.level = Some(fraction);
                m.warning = true;
                m.kind = HudKind::Power;
                return Some(m);
            }
        }
        None
    }
}

impl Default for PowerWatcher {
    fn default() -> Self {
        Self::new()
    }
}

pub fn start_brightness_watcher(
    stop: Arc<AtomicBool>,
    lang: Language,
    on_msg: Arc<dyn Fn(HudMessage) + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("mochi-brightness".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            // SAFETY: Initializes COM MTA on this WMI watcher thread.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            // SAFETY: CoCreateInstance creates WbemLocator.
            let Ok(locator): windows_core::Result<IWbemLocator> =
                (unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) })
            else {
                return;
            };
            let ns = BSTR::from("root\\wmi");
            // SAFETY: Connecting to local WMI namespace `root\wmi`.
            let Ok(svc) = (unsafe {
                locator.ConnectServer(
                    &ns,
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )
            }) else {
                return;
            };
            // SAFETY: Setting proxy blanket for WMI calls.
            unsafe {
                let _ = CoSetProxyBlanket(
                    &svc,
                    10,
                    0,
                    None,
                    RPC_C_AUTHN_LEVEL_CALL,
                    RPC_C_IMP_LEVEL_IMPERSONATE,
                    None,
                    EOLE_AUTHENTICATION_CAPABILITIES(0),
                );
            }
            let wql = BSTR::from("WQL");
            let query = BSTR::from("SELECT * FROM WmiMonitorBrightnessEvent");
            // SAFETY: Executing WMI event notification query.
            let Ok(enumerator) = (unsafe {
                svc.ExecNotificationQuery(
                    &wql,
                    &query,
                    WBEM_GENERIC_FLAG_TYPE(
                        WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0,
                    ),
                    None,
                )
            }) else {
                return;
            };
            let prop = to_wide("Brightness");
            while !stop.load(Ordering::Relaxed) {
                let mut objs: [Option<IWbemClassObject>; 1] = [None];
                let mut returned = 0u32;
                // SAFETY: Waiting up to 1000ms for next brightness event.
                let hr = unsafe { enumerator.Next(1000, &mut objs, &raw mut returned) };
                if hr.is_err() || returned == 0 {
                    continue;
                }
                if let Some(obj) = objs[0].take() {
                    let mut val = VARIANT::default();
                    // SAFETY: Reading `Brightness` property from WMI event object.
                    let got =
                        unsafe { obj.Get(PCWSTR(prop.as_ptr()), 0, &raw mut val, None, None) };
                    let parsed = if got.is_ok() {
                        val.to_string().trim().parse::<u8>().ok()
                    } else {
                        None
                    };
                    // SAFETY: Clearing VARIANT allocated by WMI `Get`.
                    unsafe {
                        let _ = VariantClear(&raw mut val);
                    }
                    if let Some(level) = parsed {
                        let mut m =
                            HudMessage::new(Glyphs::BRIGHTNESS, lang.strings().hud_brightness);
                        m.value = Some(level.to_string());
                        m.level = Some(f64::from(level) / 100.0);
                        m.kind = HudKind::Brightness;
                        on_msg(m);
                    }
                }
            }
        })
        .ok();
}

pub fn start_network_watcher(
    stop: Arc<AtomicBool>,
    lang: Language,
    on_msg: Arc<dyn Fn(HudMessage, bool) + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("mochi-network".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            let wake = Arc::new((Mutex::new(false), Condvar::new()));
            let wake_cb = Arc::clone(&wake);
            let _token = NetworkInformation::NetworkStatusChanged(
                &windows::Networking::Connectivity::NetworkStatusChangedEventHandler::new(
                    move |_| {
                        let (lock, cv) = &*wake_cb;
                        if let Ok(mut dirty) = lock.lock() {
                            *dirty = true;
                            cv.notify_one();
                        }
                        Ok(())
                    },
                ),
            );
            let (mut prev_conn, mut prev_name, _) = read_network();
            while !stop.load(Ordering::Relaxed) {
                let (lock, cv) = &*wake;
                if let Ok(guard) = lock.lock() {
                    let (mut g, _) = cv
                        .wait_timeout_while(guard, Duration::from_secs(5), |d| {
                            !*d && !stop.load(Ordering::Relaxed)
                        })
                        .unwrap_or_else(|e| e.into_inner());
                    if !*g {
                        continue;
                    }
                    *g = false;
                }
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1500));
                if let Ok(mut g) = lock.lock() {
                    *g = false;
                }
                let (conn, name, wifi) = read_network();
                if conn == prev_conn && name == prev_name {
                    continue;
                }
                let came_back = conn && !prev_conn;
                prev_conn = conn;
                prev_name = name.clone();
                let s = lang.strings();
                let mut msg = if conn {
                    let label = name
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| s.hud_connected.to_string());
                    HudMessage::new(if wifi { Glyphs::WIFI } else { Glyphs::ETHERNET }, label)
                } else {
                    let mut m = HudMessage::new(Glyphs::OFFLINE, s.hud_no_internet);
                    m.warning = true;
                    m
                };
                msg.kind = HudKind::Network;
                on_msg(msg, came_back);
            }
        })
        .ok();
}

fn read_network() -> (bool, Option<String>, bool) {
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else {
        return (false, None, false);
    };
    let conn = profile
        .GetNetworkConnectivityLevel()
        .is_ok_and(|lvl| lvl == NetworkConnectivityLevel::InternetAccess);
    let name = if conn {
        profile.ProfileName().ok().map(|s| s.to_string())
    } else {
        None
    };
    let wifi = profile.IsWlanConnectionProfile().unwrap_or(false);
    (conn, name, wifi)
}

pub fn start_camera_watcher(
    stop: Arc<AtomicBool>,
    on_change: Arc<dyn Fn(Vec<String>) + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("mochi-camera".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            let initial = read_camera_apps();
            on_change(initial);

            let path = to_wide(
                "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore",
            );
            let mut hkey = Default::default();
            // SAFETY: Opening HKCU ConsentStore key for change notifications.
            if unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    Some(0),
                    KEY_NOTIFY | KEY_READ,
                    &raw mut hkey,
                )
            }
            .is_err()
            {
                return;
            }
            // SAFETY: Creating auto-reset event for RegNotifyChangeKeyValue.
            let Ok(ev) = (unsafe { CreateEventW(None, false, false, None) }) else {
                // SAFETY: Closing registry key.
                unsafe {
                    let _ = RegCloseKey(hkey);
                }
                return;
            };
            while !stop.load(Ordering::Relaxed) {
                // SAFETY: Arming async registry change notification.
                let err = unsafe {
                    RegNotifyChangeKeyValue(
                        hkey,
                        true,
                        REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET,
                        Some(ev),
                        true,
                    )
                };
                if err.is_err() {
                    break;
                }
                let handles = [ev];
                // SAFETY: Waiting up to 500ms so `stop` is checked promptly.
                let wait = unsafe { WaitForMultipleObjects(&handles, false, 500) };
                if wait == WAIT_OBJECT_0 {
                    std::thread::sleep(Duration::from_millis(200));
                    on_change(read_camera_apps());
                }
            }
            // SAFETY: Closing event and registry key.
            unsafe {
                let _ = CloseHandle(ev);
                let _ = RegCloseKey(hkey);
            }
        })
        .ok();
}

pub fn read_camera_apps() -> Vec<String> {
    let base = "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam";
    let wbase = to_wide(base);
    let mut root = Default::default();
    // SAFETY: Opening webcam ConsentStore key.
    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wbase.as_ptr()),
            Some(0),
            KEY_READ,
            &raw mut root,
        )
    }
    .is_err()
    {
        return Vec::new();
    }
    let mut apps = BTreeSet::new();
    for sub in enum_subkeys(root) {
        if sub == "NonPackaged" {
            let wnp = to_wide(&format!("{base}\\NonPackaged"));
            let mut np_key = Default::default();
            // SAFETY: Opening NonPackaged subkey.
            if unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(wnp.as_ptr()),
                    Some(0),
                    KEY_READ,
                    &raw mut np_key,
                )
            }
            .is_ok()
            {
                for exe in enum_subkeys(np_key) {
                    let wexe = to_wide(&format!("{base}\\NonPackaged\\{exe}"));
                    let mut app_key = Default::default();
                    // SAFETY: Opening app subkey.
                    if unsafe {
                        RegOpenKeyExW(
                            HKEY_CURRENT_USER,
                            PCWSTR(wexe.as_ptr()),
                            Some(0),
                            KEY_READ,
                            &raw mut app_key,
                        )
                    }
                    .is_ok()
                    {
                        if reg_in_use(app_key) {
                            let path = exe.replace('#', "\\");
                            let name = file_description(&path).unwrap_or_else(|| {
                                Path::new(&path)
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or(path)
                            });
                            apps.insert(name);
                        }
                        // SAFETY: Closing app key.
                        unsafe {
                            let _ = RegCloseKey(app_key);
                        }
                    }
                }
                // SAFETY: Closing NonPackaged key.
                unsafe {
                    let _ = RegCloseKey(np_key);
                }
            }
        } else {
            let wapp = to_wide(&format!("{base}\\{sub}"));
            let mut app_key = Default::default();
            // SAFETY: Opening packaged app subkey.
            if unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(wapp.as_ptr()),
                    Some(0),
                    KEY_READ,
                    &raw mut app_key,
                )
            }
            .is_ok()
            {
                if reg_in_use(app_key) {
                    let name = sub.split('_').next().unwrap_or(&sub).to_string();
                    apps.insert(name);
                }
                // SAFETY: Closing app key.
                unsafe {
                    let _ = RegCloseKey(app_key);
                }
            }
        }
    }
    // SAFETY: Closing root webcam key.
    unsafe {
        let _ = RegCloseKey(root);
    }
    apps.into_iter().collect()
}

fn enum_subkeys(hkey: windows::Win32::System::Registry::HKEY) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0u32;
    loop {
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        // SAFETY: Enumerating subkey `idx` of `hkey`.
        let res = unsafe {
            RegEnumKeyExW(
                hkey,
                idx,
                Some(PWSTR(buf.as_mut_ptr())),
                &raw mut len,
                None,
                None,
                None,
                None,
            )
        };
        if res.is_err() {
            break;
        }
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
        idx += 1;
    }
    out
}

fn reg_in_use(hkey: windows::Win32::System::Registry::HKEY) -> bool {
    let start = reg_qword(hkey, "LastUsedTimeStart").unwrap_or(0);
    let stop = reg_qword(hkey, "LastUsedTimeStop").unwrap_or(1);
    start > 0 && stop == 0
}

fn reg_qword(hkey: windows::Win32::System::Registry::HKEY, name: &str) -> Option<u64> {
    let wname = to_wide(name);
    let mut val = 0u64;
    let mut size = std::mem::size_of::<u64>() as u32;
    // SAFETY: Querying 8-byte QWORD registry value.
    let ok = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(wname.as_ptr()),
            None,
            None,
            Some((&raw mut val).cast()),
            Some(&raw mut size),
        )
    };
    if ok.is_ok() && size == 8 {
        Some(val)
    } else {
        None
    }
}

pub fn is_motion_reduced() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut anim = 1i32;
    // SAFETY: Querying SPI_GETCLIENTAREAANIMATION into stack local `anim`.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut anim).cast()),
            Default::default(),
        )
    };
    let anim_off = ok.is_ok() && anim == 0;
    let saver = windows::System::Power::PowerManager::EnergySaverStatus()
        .is_ok_and(|s| s == windows::System::Power::EnergySaverStatus::On);
    anim_off || saver
}

pub fn _unused_mutex_marker(_: &Mutex<()>) {}
