use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW,
    MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_core::BOOL;

use crate::strings::{Language, fmt3};

#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub device_name: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub is_primary: bool,
}

struct RawMonitor {
    device_name: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    is_primary: bool,
}

// SAFETY: Called synchronously by Win32 EnumDisplayMonitors with `lparam` pointing to `Vec<RawMonitor>`.
unsafe extern "system" fn enum_monitor_cb(
    hmon: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: `lparam` points to a live `Vec<RawMonitor>` on the caller's stack.
    let list = unsafe { &mut *(lparam.0 as *mut Vec<RawMonitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `hmon` is a valid monitor handle and `info` has `cbSize` initialized.
    if unsafe { GetMonitorInfoW(hmon, (&raw mut info).cast()) }.as_bool() {
        let end = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        let device_name = String::from_utf16_lossy(&info.szDevice[..end]);
        let r = info.monitorInfo.rcMonitor;
        list.push(RawMonitor {
            device_name,
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
            is_primary: (info.monitorInfo.dwFlags & 1) != 0,
        });
    }
    BOOL(1)
}

pub struct Monitors;

impl Monitors {
    pub fn all(lang: Language) -> Vec<DisplayInfo> {
        let mut raw: Vec<RawMonitor> = Vec::new();
        // SAFETY: `enum_monitor_cb` only mutates `raw` synchronously during `EnumDisplayMonitors`.
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(enum_monitor_cb),
                LPARAM((&raw mut raw) as isize),
            );
        }
        raw.sort_by_key(|m| (m.x, m.y));
        let s = lang.strings();
        raw.into_iter()
            .enumerate()
            .map(|(i, m)| {
                let tmpl = if m.is_primary {
                    s.monitor_label_primary
                } else {
                    s.monitor_label
                };
                let label = fmt3(
                    tmpl,
                    &(i + 1).to_string(),
                    &m.width.to_string(),
                    &m.height.to_string(),
                );
                DisplayInfo {
                    device_name: m.device_name,
                    label,
                    x: m.x,
                    y: m.y,
                    width: m.width,
                    height: m.height,
                    is_primary: m.is_primary,
                }
            })
            .collect()
    }

    pub fn resolve(device_name: Option<&str>, lang: Language) -> DisplayInfo {
        let all = Self::all(lang);
        if let Some(name) = device_name
            && let Some(found) = all.iter().find(|m| m.device_name == name)
        {
            return found.clone();
        }
        if let Some(primary) = all.iter().find(|m| m.is_primary) {
            return primary.clone();
        }
        all.into_iter().next().unwrap_or(DisplayInfo {
            device_name: String::new(),
            label: "Display 1".to_string(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            is_primary: true,
        })
    }

    pub fn scale_at(x: i32, y: i32) -> f64 {
        // SAFETY: MonitorFromPoint is a safe Win32 call.
        let hmon = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        // SAFETY: `hmon` is valid and `dpi_x`/`dpi_y` are stack locals.
        if unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) }
            .is_ok()
            && dpi_x > 0
        {
            f64::from(dpi_x) / 96.0
        } else {
            1.0
        }
    }
}
