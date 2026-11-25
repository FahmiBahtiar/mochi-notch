use std::path::Path;
use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW,
};
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, SetProcessWorkingSetSize,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetLastInputInfo, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, LASTINPUTINFO,
    SendInput, VIRTUAL_KEY, VK_BACK, VK_C, VK_CONTROL, VK_I, VK_L, VK_MENU, VK_RETURN, VK_V,
};
use windows::Win32::UI::Shell::{
    SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetIconInfo, GetWindow, GetWindowLongW,
    GetWindowThreadProcessId, HWND_TOPMOST, ICONINFO, IsIconic, IsWindow, IsWindowVisible,
    SW_RESTORE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowDisplayAffinity, SetWindowPos, ShowWindow,
    WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WS_EX_TOOLWINDOW,
};
use windows_core::{BOOL, PCWSTR};

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn get_cursor_pos() -> Option<(i32, i32)> {
    let mut pt = POINT::default();
    // SAFETY: `pt` is a valid stack local.
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&raw mut pt) }.is_ok() {
        Some((pt.x, pt.y))
    } else {
        None
    }
}

pub fn get_idle_ms() -> u64 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` has `cbSize` initialized.
    if unsafe { GetLastInputInfo(&raw mut info) }.as_bool() {
        // SAFETY: GetTickCount has no preconditions.
        let now = unsafe { GetTickCount() };
        u64::from(now.wrapping_sub(info.dwTime))
    } else {
        0
    }
}

pub fn keep_topmost(hwnd: HWND) {
    // SAFETY: `hwnd` is a valid window handle owned by this process.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub fn move_and_resize_window(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    // SAFETY: `hwnd` is a valid window handle owned by this process.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub fn set_capture_exclusion(hwnd: HWND, exclude: bool) -> bool {
    let affinity = if exclude {
        WDA_EXCLUDEFROMCAPTURE
    } else {
        WDA_NONE
    };
    // SAFETY: `hwnd` is a valid window handle owned by this process.
    unsafe { SetWindowDisplayAffinity(hwnd, affinity) }.is_ok()
}

pub fn focus_host_window(handle: i64, pid: i32) -> bool {
    let hwnd = HWND(handle as isize as *mut _);
    // SAFETY: Win32 window validation and foreground calls accept any HWND safely.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        let mut owner = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&raw mut owner)) == 0
            || owner != pid as u32
            || owner == GetCurrentProcessId()
        {
            return false;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

#[derive(Debug, Clone)]
pub enum VsCodeChatAction {
    OpenChat,
    NewChat,
    StopAgent,
    SendPrompt(String),
}

fn process_is_vscode(pid: u32) -> bool {
    // SAFETY: `OpenProcess` returns a handle closed before returning.
    let Ok(proc) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return false;
    };
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    // SAFETY: `proc` is valid and `buf` has capacity `len`.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            windows_core::PWSTR(buf.as_mut_ptr()),
            &raw mut len,
        )
    }
    .is_ok();
    // SAFETY: `proc` is valid.
    unsafe {
        let _ = CloseHandle(proc);
    }
    if !ok {
        return false;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| {
            n.eq_ignore_ascii_case("Code.exe") || n.eq_ignore_ascii_case("Code - Insiders.exe")
        })
}

// SAFETY: Called synchronously by Win32 `EnumWindows` with `lparam` pointing to `Option<(i64, i32)>`.
unsafe extern "system" fn enum_vscode_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` points to a live stack local during `EnumWindows`.
    let out = unsafe { &mut *(lparam.0 as *mut Option<(i64, i32)>) };
    // SAFETY: Win32 window queries accept any HWND safely.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid())
            || (GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as i32) != 0
        {
            return BOOL(1);
        }
        let mut pid = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) != 0
            && pid != 0
            && process_is_vscode(pid)
        {
            *out = Some((hwnd.0 as isize as i64, pid as i32));
            return BOOL(0);
        }
    }
    BOOL(1)
}

pub fn find_vscode_window() -> Option<(i64, i32)> {
    let mut found: Option<(i64, i32)> = None;
    // SAFETY: `enum_vscode_cb` only mutates `found` synchronously during `EnumWindows`.
    unsafe {
        let _ = EnumWindows(Some(enum_vscode_cb), LPARAM((&raw mut found) as isize));
    }
    found
}

fn send_key_chord(mods: &[VIRTUAL_KEY], key: VIRTUAL_KEY) {
    let mut inputs = Vec::with_capacity(mods.len() * 2 + 2);
    let make = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    };
    for &m in mods {
        inputs.push(make(m, false));
    }
    inputs.push(make(key, false));
    inputs.push(make(key, true));
    for &m in mods.iter().rev() {
        inputs.push(make(m, true));
    }
    // SAFETY: `inputs` is a valid slice of `INPUT` structs.
    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn trigger_vscode_chat_action(window: Option<(i64, i32)>, action: VsCodeChatAction) {
    std::thread::spawn(move || {
        let target = window
            .filter(|&(_, pid)| pid > 0 && process_is_vscode(pid as u32))
            .or_else(find_vscode_window);
        let Some((handle, pid)) = target else {
            return;
        };
        if !focus_host_window(handle, pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(70));
        match action {
            VsCodeChatAction::OpenChat => {
                send_key_chord(&[VK_CONTROL, VK_MENU], VK_I);
            }
            VsCodeChatAction::NewChat => {
                send_key_chord(&[VK_CONTROL, VK_MENU], VK_I);
                std::thread::sleep(std::time::Duration::from_millis(90));
                send_key_chord(&[VK_CONTROL], VK_L);
            }
            VsCodeChatAction::StopAgent => {
                send_key_chord(&[VK_MENU], VK_BACK);
            }
            VsCodeChatAction::SendPrompt(prompt) => {
                let trimmed = prompt.trim();
                if trimmed.is_empty() {
                    return;
                }
                crate::agent_setup::copy_text_to_clipboard(trimmed);
                send_key_chord(&[VK_CONTROL, VK_MENU], VK_I);
                std::thread::sleep(std::time::Duration::from_millis(130));
                send_key_chord(&[VK_CONTROL], VK_V);
                std::thread::sleep(std::time::Duration::from_millis(60));
                send_key_chord(&[], VK_RETURN);
            }
        }
    });
}

pub fn send_terminal_interrupt(handle: i64, pid: i32) {
    std::thread::spawn(move || {
        if focus_host_window(handle, pid) {
            std::thread::sleep(std::time::Duration::from_millis(70));
            send_key_chord(&[VK_CONTROL], VK_C);
        }
    });
}

pub struct Autostart;

impl Autostart {
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "Mochi";
    pub const ARGUMENT: &str = "--autostart";

    fn command() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(format!("\"{}\" {}", exe.display(), Self::ARGUMENT))
    }

    fn legacy_path() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(format!("\"{}\"", exe.display()))
    }

    pub fn is_enabled() -> bool {
        let sub = to_wide(Self::RUN_KEY);
        let val = to_wide(Self::VALUE_NAME);
        let mut hkey = Default::default();
        // SAFETY: `sub` is NUL-terminated UTF-16; `hkey` is closed before returning.
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                Some(0),
                KEY_READ,
                &raw mut hkey,
            )
        }
        .is_err()
        {
            return false;
        }
        let mut buf = vec![0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `hkey` is valid and `buf` has `bytes` capacity.
        let ok = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(val.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            )
        };
        // SAFETY: `hkey` is valid.
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if ok.is_err() {
            return false;
        }
        let len = (bytes as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..len])
            .trim_end_matches('\0')
            .to_string();
        Self::command().is_some_and(|c| s.eq_ignore_ascii_case(&c))
            || Self::legacy_path().is_some_and(|p| s.eq_ignore_ascii_case(&p))
    }

    pub fn upgrade() {
        let sub = to_wide(Self::RUN_KEY);
        let val = to_wide(Self::VALUE_NAME);
        let mut hkey = Default::default();
        // SAFETY: `sub` is NUL-terminated UTF-16; `hkey` is closed before returning.
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                Some(0),
                KEY_READ,
                &raw mut hkey,
            )
        }
        .is_err()
        {
            return;
        }
        let mut buf = vec![0u16; 1024];
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `hkey` is valid and `buf` has `bytes` capacity.
        let ok = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(val.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&raw mut bytes),
            )
        };
        // SAFETY: `hkey` is valid.
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if ok.is_err() {
            return;
        }
        let len = (bytes as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..len])
            .trim_end_matches('\0')
            .to_string();
        if Self::legacy_path().is_some_and(|p| s.eq_ignore_ascii_case(&p)) {
            Self::set(true);
        }
    }

    pub fn set(enabled: bool) {
        let sub = to_wide(Self::RUN_KEY);
        let val = to_wide(Self::VALUE_NAME);
        let mut hkey = Default::default();
        // SAFETY: `sub` is NUL-terminated UTF-16; `hkey` is closed before returning.
        if unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                Some(0),
                None,
                Default::default(),
                KEY_WRITE,
                None,
                &raw mut hkey,
                None,
            )
        }
        .is_err()
        {
            return;
        }
        if enabled {
            if let Some(cmd) = Self::command() {
                let wide = to_wide(&cmd);
                // SAFETY: `wide` is NUL-terminated UTF-16 and `wide.len() * 2` is its byte length.
                let bytes = unsafe {
                    std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2)
                };
                // SAFETY: `hkey` is valid.
                unsafe {
                    let _ =
                        RegSetValueExW(hkey, PCWSTR(val.as_ptr()), Some(0), REG_SZ, Some(bytes));
                }
            }
        } else {
            // SAFETY: `hkey` is valid.
            unsafe {
                let _ = RegDeleteValueW(hkey, PCWSTR(val.as_ptr()));
            }
        }
        // SAFETY: `hkey` is valid.
        unsafe {
            let _ = RegCloseKey(hkey);
        }
    }
}

static CHIME_WAV: OnceLock<Vec<u8>> = OnceLock::new();

pub struct Chime;

impl Chime {
    fn build_wav() -> Vec<u8> {
        const RATE: usize = 44100;
        let samples = (RATE as f64 * 1.1) as usize;
        let mut pcm = vec![0i16; samples];
        for (freq, start) in [(1046.5_f64, 0.0_f64), (1568.0_f64, 0.16_f64)] {
            let start_idx = (start * RATE as f64) as usize;
            for (i, slot) in pcm.iter_mut().enumerate().skip(start_idx) {
                let t = (i as f64) / (RATE as f64) - start;
                let env = (t / 0.01).min(1.0) * (-t * 5.0).exp();
                let tone = (2.0 * std::f64::consts::PI * freq * t).sin()
                    + 0.25 * (4.0 * std::f64::consts::PI * freq * t).sin();
                let mixed = f64::from(*slot) + tone * env * 5500.0;
                *slot = mixed.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            }
        }
        let data_bytes = (samples * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_bytes as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&(RATE as u32).to_le_bytes());
        wav.extend_from_slice(&((RATE * 2) as u32).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for s in pcm {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        wav
    }

    pub fn play() {
        let wav = CHIME_WAV.get_or_init(Self::build_wav);
        // SAFETY: `CHIME_WAV` is `'static` (`OnceLock`), so the memory buffer outlives async playback.
        unsafe {
            let _ = PlaySoundW(
                PCWSTR(wav.as_ptr().cast()),
                None,
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct IconBitmap {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    pub is_picture: bool,
}

pub fn load_shell_icon(path: &str, type_only: bool) -> Option<IconBitmap> {
    let wide = to_wide(path);
    let mut info = SHFILEINFOW::default();
    let (attrs, flags) = if type_only {
        let attr = if Path::new(path).extension().is_some() {
            FILE_ATTRIBUTE_NORMAL.0
        } else {
            FILE_ATTRIBUTE_DIRECTORY.0
        };
        (attr, SHGFI_ICON | SHGFI_LARGEICON | SHGFI_USEFILEATTRIBUTES)
    } else {
        (0, SHGFI_ICON | SHGFI_LARGEICON)
    };
    // SAFETY: `wide` is NUL-terminated UTF-16 and `info` is zero-initialized.
    let res = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(attrs),
            Some(&raw mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };
    if res == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let bmp = hicon_to_bgra(info.hIcon);
    // SAFETY: `info.hIcon` was returned by `SHGetFileInfoW`.
    unsafe {
        let _ = DestroyIcon(info.hIcon);
    }
    bmp
}

fn hicon_to_bgra(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<IconBitmap> {
    let mut icon_info = ICONINFO::default();
    // SAFETY: `hicon` is valid and `icon_info` is a stack local.
    if unsafe { GetIconInfo(hicon, &raw mut icon_info) }.is_err() {
        return None;
    }
    let color_bmp = icon_info.hbmColor;
    let mask_bmp = icon_info.hbmMask;
    let mut bm = BITMAP::default();
    let target_bmp = if !color_bmp.is_invalid() {
        color_bmp
    } else {
        mask_bmp
    };
    // SAFETY: `target_bmp` is a valid HBITMAP from `GetIconInfo`.
    let got = unsafe {
        GetObjectW(
            target_bmp.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&raw mut bm).cast()),
        )
    };
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 || bm.bmWidth > 1024 || bm.bmHeight > 1024 {
        // SAFETY: Bitmaps from `GetIconInfo` must be deleted.
        unsafe {
            if !color_bmp.is_invalid() {
                let _ = DeleteObject(color_bmp.into());
            }
            if !mask_bmp.is_invalid() {
                let _ = DeleteObject(mask_bmp.into());
            }
        }
        return None;
    }
    let w = bm.bmWidth as u32;
    let h = bm.bmHeight as u32;
    let buf_len = w
        .checked_mul(h)
        .and_then(|px| px.checked_mul(4))
        .and_then(|b| usize::try_from(b).ok())?;
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; buf_len];
    // SAFETY: `CreateCompatibleDC(None)` creates a screen-compatible memory DC.
    let hdc = unsafe { CreateCompatibleDC(None) };
    // SAFETY: `hdc`, `target_bmp`, `pixels`, and `bmi` are valid.
    let lines = unsafe {
        GetDIBits(
            hdc,
            target_bmp,
            0,
            h,
            Some(pixels.as_mut_ptr().cast()),
            &raw mut bmi,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: Clean up GDI handles.
    unsafe {
        let _ = DeleteDC(hdc);
        if !color_bmp.is_invalid() {
            let _ = DeleteObject(color_bmp.into());
        }
        if !mask_bmp.is_invalid() {
            let _ = DeleteObject(mask_bmp.into());
        }
    }
    if lines == 0 {
        return None;
    }
    // Premultiply alpha for Direct2D
    for px in pixels.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        px[0] = ((u16::from(px[0]) * a + 127) / 255) as u8;
        px[1] = ((u16::from(px[1]) * a + 127) / 255) as u8;
        px[2] = ((u16::from(px[2]) * a + 127) / 255) as u8;
    }
    Some(IconBitmap {
        width: w,
        height: h,
        bgra: pixels,
        is_picture: false,
    })
}

/// Hints Windows to trim cold/unused pages from the process working set when idle.
pub fn trim_working_set() {
    // SAFETY: `GetCurrentProcess()` returns a valid pseudo-handle; passing `usize::MAX`
    // for both min and max instructs Windows to empty cold pages from the working set.
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}
