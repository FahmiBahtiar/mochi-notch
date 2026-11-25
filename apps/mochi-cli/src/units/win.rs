use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_BUSY, FILETIME, GetLastError, HANDLE, HWND,
    INVALID_HANDLE_VALUE, LocalFree, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FlushFileBuffers,
    GetFileVersionInfoSizeW, GetFileVersionInfoW, OPEN_EXISTING, ReadFile, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT, SYNCHRONIZE, VerQueryValueW, WriteFile,
};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, OpenMutexW, OpenProcess,
    OpenProcessToken, PROCESS_BASIC_INFORMATION, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongW, GetWindowThreadProcessId,
    IsWindowVisible, WS_EX_TOOLWINDOW,
};

const MAX_DEPTH: usize = 12;
const SINGLE_INSTANCE_NAME: &str = "Mochi.SingleInstance";

#[derive(Debug, Clone, Copy)]
pub struct HostWindow {
    pub handle: i64,
    pub pid: i32,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide_slice(slice: &[u16]) -> String {
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end])
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

fn process_started(pid: u32) -> Option<u64> {
    // SAFETY: OpenProcess returns null on failure; handle is closed before returning.
    let proc = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if proc.is_null() {
        return None;
    }
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    // SAFETY: `proc` is a valid process handle and all FILETIME pointers are valid stack locals.
    let ok = unsafe { GetProcessTimes(proc, &mut created, &mut exit, &mut kernel, &mut user) };
    // SAFETY: `proc` is a valid handle opened above.
    unsafe { CloseHandle(proc) };
    if ok != 0 {
        Some(filetime_to_u64(created))
    } else {
        None
    }
}

fn image_path(pid: u32) -> Option<String> {
    // SAFETY: OpenProcess returns null on failure; handle is closed before returning.
    let proc = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if proc.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut size = buf.len() as u32;
    // SAFETY: `proc` is valid and `buf` has capacity `size` UTF-16 code units.
    let ok = unsafe {
        QueryFullProcessImageNameW(proc, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size)
    };
    // SAFETY: `proc` is a valid handle opened above.
    unsafe { CloseHandle(proc) };
    if ok != 0 {
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    } else {
        None
    }
}

fn is_explorer(pid: u32) -> bool {
    image_path(pid)
        .and_then(|p| {
            Path::new(&p)
                .file_stem()
                .map(|s| s.to_string_lossy().eq_ignore_ascii_case("explorer"))
        })
        .unwrap_or(false)
}

fn parent_pid(pid: u32) -> Option<u32> {
    // SAFETY: OpenProcess returns null on failure; handle is closed before returning.
    let proc = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if proc.is_null() {
        return None;
    }
    // SAFETY: PROCESS_BASIC_INFORMATION is a plain C struct of integers/pointers where all-zero is valid.
    let mut info: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let mut ret_len = 0u32;
    // SAFETY: `proc` is valid and `info` matches ProcessBasicInformation (class 0) size.
    let status = unsafe {
        NtQueryInformationProcess(
            proc,
            0,
            (&raw mut info).cast::<c_void>(),
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            &mut ret_len,
        )
    };
    // SAFETY: `proc` is a valid handle opened above.
    unsafe { CloseHandle(proc) };
    if status != 0 {
        return None;
    }
    let parent = info.InheritedFromUniqueProcessId as u32;
    if parent == 0 {
        return None;
    }
    let born = process_started(parent)?;
    let child = process_started(pid)?;
    if born <= child { Some(parent) } else { None }
}

struct EnumState {
    target_pid: u32,
    found: HWND,
}

// SAFETY: Called synchronously by Win32 EnumWindows with `lparam` pointing to `EnumState`.
unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lparam: isize) -> i32 {
    // SAFETY: `lparam` points to a live `EnumState` on the caller's stack for the duration of `EnumWindows`.
    let state = unsafe { &mut *(lparam as *mut EnumState) };
    let mut owner = 0u32;
    // SAFETY: Win32 window query functions accept any HWND safely.
    unsafe {
        if GetWindowThreadProcessId(hwnd, &mut owner) == 0
            || owner != state.target_pid
            || IsWindowVisible(hwnd) == 0
            || !GetWindow(hwnd, GW_OWNER).is_null()
            || (GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as i32) != 0
        {
            return 1;
        }
    }
    state.found = hwnd;
    0
}

fn top_window(pid: u32) -> Option<HWND> {
    let mut state = EnumState {
        target_pid: pid,
        found: null_mut(),
    };
    // SAFETY: `enum_windows_cb` only mutates `state` synchronously during `EnumWindows`.
    unsafe {
        EnumWindows(Some(enum_windows_cb), (&raw mut state) as isize);
    }
    if state.found.is_null() {
        None
    } else {
        Some(state.found)
    }
}

// SAFETY: Called synchronously by Win32 `EnumWindows` with `lparam` pointing to `Option<HostWindow>`.
unsafe extern "system" fn enum_vscode_cb(hwnd: HWND, lparam: isize) -> i32 {
    // SAFETY: `lparam` points to a live stack local during `EnumWindows`.
    let out = unsafe { &mut *(lparam as *mut Option<HostWindow>) };
    let mut owner = 0u32;
    // SAFETY: Win32 window query functions accept any HWND safely.
    unsafe {
        if GetWindowThreadProcessId(hwnd, &mut owner) == 0
            || owner == 0
            || IsWindowVisible(hwnd) == 0
            || !GetWindow(hwnd, GW_OWNER).is_null()
            || (GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as i32) != 0
        {
            return 1;
        }
    }
    if let Some(path) = image_path(owner)
        && let Some(name) = Path::new(&path).file_name().and_then(|n| n.to_str())
        && (name.eq_ignore_ascii_case("Code.exe")
            || name.eq_ignore_ascii_case("Code - Insiders.exe"))
    {
        *out = Some(HostWindow {
            handle: hwnd as isize as i64,
            pid: owner as i32,
        });
        return 0;
    }
    1
}

impl HostWindow {
    pub fn find() -> Option<Self> {
        // SAFETY: GetCurrentProcessId has no preconditions.
        let mut pid = unsafe { GetCurrentProcessId() };
        for _ in 0..MAX_DEPTH {
            let Some(parent) = parent_pid(pid) else {
                break;
            };
            if parent == pid || is_explorer(parent) {
                break;
            }
            pid = parent;
            if let Some(hwnd) = top_window(pid) {
                return Some(Self {
                    handle: hwnd as isize as i64,
                    pid: pid as i32,
                });
            }
        }
        // SAFETY: GetConsoleWindow and window queries are safe Win32 calls.
        unsafe {
            let console = GetConsoleWindow();
            if !console.is_null() && IsWindowVisible(console) != 0 {
                let mut host = 0u32;
                if GetWindowThreadProcessId(console, &mut host) != 0 {
                    return Some(Self {
                        handle: console as isize as i64,
                        pid: host as i32,
                    });
                }
            }
        }
        None
    }

    pub fn find_vscode() -> Option<Self> {
        let mut found: Option<Self> = None;
        // SAFETY: `enum_vscode_cb` only mutates `found` synchronously during `EnumWindows`.
        unsafe {
            EnumWindows(Some(enum_vscode_cb), (&raw mut found) as isize);
        }
        found
    }

    pub fn app_name(self) -> Option<String> {
        let path = image_path(self.pid as u32)?;
        let exe = Path::new(&path).file_stem()?.to_string_lossy();
        if exe.eq_ignore_ascii_case("conhost") || exe.eq_ignore_ascii_case("OpenConsole") {
            return None;
        }
        let wide_path = to_wide(&path);
        let mut dummy = 0u32;
        // SAFETY: `wide_path` is NUL-terminated UTF-16.
        let size = unsafe { GetFileVersionInfoSizeW(wide_path.as_ptr(), &mut dummy) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: `data` has length `size` matching `GetFileVersionInfoSizeW`.
        if unsafe { GetFileVersionInfoW(wide_path.as_ptr(), 0, size, data.as_mut_ptr().cast()) }
            == 0
        {
            return None;
        }
        let mut trans_ptr: *mut c_void = null_mut();
        let mut trans_len = 0u32;
        let trans_query = to_wide("\\VarFileInfo\\Translation");
        // SAFETY: `data` holds valid version info resource bytes.
        let has_trans = unsafe {
            VerQueryValueW(
                data.as_ptr().cast(),
                trans_query.as_ptr(),
                &mut trans_ptr,
                &mut trans_len,
            )
        };
        let (lang, cp) = if has_trans != 0 && trans_len >= 4 && !trans_ptr.is_null() {
            // SAFETY: `trans_ptr` points to at least 4 bytes (`[u16; 2]`) inside `data`.
            let pair = unsafe { std::slice::from_raw_parts(trans_ptr.cast::<u16>(), 2) };
            (pair[0], pair[1])
        } else {
            (0x0409, 0x04B0)
        };

        for key in ["ProductName", "FileDescription"] {
            let sub = to_wide(&format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\{key}"));
            let mut val_ptr: *mut c_void = null_mut();
            let mut val_len = 0u32;
            // SAFETY: `data` holds valid version info and `sub` is NUL-terminated UTF-16.
            let ok = unsafe {
                VerQueryValueW(
                    data.as_ptr().cast(),
                    sub.as_ptr(),
                    &mut val_ptr,
                    &mut val_len,
                )
            };
            if ok != 0 && val_len > 0 && !val_ptr.is_null() {
                let ptr = val_ptr.cast::<u16>();
                let len = val_len as usize;
                // SAFETY: `ptr` points to `len` UTF-16 code units inside `data`.
                let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
                let text = from_wide_slice(slice);
                let trimmed = text.trim();
                if !trimmed.is_empty() && !trimmed.contains("Windows®") {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }
}

fn current_user_sid_bytes() -> Option<(Vec<u8>, String)> {
    let mut token: HANDLE = null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle; `token` is closed before returning.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let mut needed = 0u32;
    // SAFETY: Querying required buffer size for TokenUser.
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed);
    }
    if needed == 0 {
        // SAFETY: `token` is valid.
        unsafe { CloseHandle(token) };
        return None;
    }
    let mut buf = vec![0u8; needed as usize];
    // SAFETY: `buf` has `needed` bytes for `TOKEN_USER`.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    // SAFETY: `token` is valid.
    unsafe { CloseHandle(token) };
    if ok == 0 {
        return None;
    }
    // SAFETY: `buf` contains a valid `TOKEN_USER` populated by `GetTokenInformation`.
    let sid: PSID = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut sid_str_ptr: *mut u16 = null_mut();
    // SAFETY: `sid` points inside `buf`; `sid_str_ptr` is freed with `LocalFree`.
    if unsafe { ConvertSidToStringSidW(sid, &mut sid_str_ptr) } == 0 || sid_str_ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    // SAFETY: `ConvertSidToStringSidW` returns a NUL-terminated UTF-16 string.
    unsafe {
        while *sid_str_ptr.add(len) != 0 {
            len += 1;
        }
    }
    // SAFETY: `sid_str_ptr` is valid for `len` UTF-16 code units.
    let sid_slice = unsafe { std::slice::from_raw_parts(sid_str_ptr, len) };
    let sid_string = String::from_utf16_lossy(sid_slice);
    // SAFETY: `sid_str_ptr` was allocated by `ConvertSidToStringSidW`.
    unsafe { LocalFree(sid_str_ptr.cast()) };
    Some((buf, sid_string))
}

fn default_pipe_name() -> Option<String> {
    let (_, sid) = current_user_sid_bytes()?;
    let mut session_id = 0u32;
    // SAFETY: GetCurrentProcessId and ProcessIdToSessionId are safe Win32 calls.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) } == 0 {
        return None;
    }
    Some(format!("Mochi.{sid}.{session_id}"))
}

fn is_mochi_running() -> bool {
    let name = to_wide(SINGLE_INSTANCE_NAME);
    // SAFETY: `name` is NUL-terminated UTF-16; returned handle is closed immediately.
    let mutex = unsafe { OpenMutexW(SYNCHRONIZE, 0, name.as_ptr()) };
    if mutex.is_null() {
        false
    } else {
        // SAFETY: `mutex` is a valid handle opened above.
        unsafe { CloseHandle(mutex) };
        true
    }
}

fn pipe_owned_by_current_user(pipe: HANDLE) -> bool {
    let Some((user_buf, _)) = current_user_sid_bytes() else {
        return false;
    };
    // SAFETY: `user_buf` holds a valid `TOKEN_USER`.
    let current_sid: PSID = unsafe { (*user_buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut owner_sid: PSID = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `pipe` is a valid kernel handle; `sd` is freed with `LocalFree`.
    let err = unsafe {
        GetSecurityInfo(
            pipe,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner_sid,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        )
    };
    if err != 0 || owner_sid.is_null() {
        if !sd.is_null() {
            // SAFETY: `sd` was allocated by `GetSecurityInfo`.
            unsafe { LocalFree(sd) };
        }
        return false;
    }
    // SAFETY: Both SIDs are valid pointers.
    let same = unsafe { EqualSid(owner_sid, current_sid) } != 0;
    if !sd.is_null() {
        // SAFETY: `sd` was allocated by `GetSecurityInfo`.
        unsafe { LocalFree(sd) };
    }
    same
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: `self.0` is an owned Win32 handle.
            unsafe { CloseHandle(self.0) };
        }
    }
}

pub fn send_activity(json: &str, pipe_name: Option<&str>, timeout_ms: u32) -> Option<String> {
    if pipe_name.is_none() && !is_mochi_running() {
        return None;
    }
    let resolved = match pipe_name {
        Some(n) => n.to_string(),
        None => default_pipe_name()?,
    };
    let full_name = to_wide(&format!("\\\\.\\pipe\\{resolved}"));
    let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));

    let pipe = loop {
        // SAFETY: `full_name` is NUL-terminated UTF-16.
        let h = unsafe {
            CreateFileW(
                full_name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        };
        if h != INVALID_HANDLE_VALUE {
            break HandleGuard(h);
        }
        // SAFETY: GetLastError reads thread-local Win32 error code.
        let err = unsafe { GetLastError() };
        if err != ERROR_PIPE_BUSY {
            return None;
        }
        let rem = deadline.saturating_duration_since(Instant::now());
        if rem.is_zero() {
            return None;
        }
        let wait_ms = rem.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        // SAFETY: `full_name` is NUL-terminated UTF-16.
        if unsafe { WaitNamedPipeW(full_name.as_ptr(), wait_ms) } == 0 {
            return None;
        }
    };

    if !pipe_owned_by_current_user(pipe.0) {
        return None;
    }

    let mut payload = Vec::with_capacity(json.len() + 1);
    payload.extend_from_slice(json.as_bytes());
    payload.push(b'\n');

    if !write_overlapped(pipe.0, &payload, deadline) {
        return Some("error: no answer".to_string());
    }
    // SAFETY: `pipe.0` is a valid pipe handle.
    unsafe { FlushFileBuffers(pipe.0) };

    read_line_overlapped(pipe.0, 1024, deadline).or_else(|| Some("error: no answer".to_string()))
}

fn write_overlapped(pipe: HANDLE, data: &[u8], deadline: Instant) -> bool {
    // SAFETY: CreateEventW creates an unnamed manual-reset event.
    let ev = unsafe { CreateEventW(null(), 1, 0, null()) };
    if ev.is_null() {
        return false;
    }
    let _ev_guard = HandleGuard(ev);
    // SAFETY: OVERLAPPED is a plain Win32 C struct where all-zero is valid before setting hEvent.
    let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
    ov.hEvent = ev;
    let mut written = 0u32;
    // SAFETY: `pipe`, `data`, and `ov` remain valid until `GetOverlappedResult` completes.
    let ok = unsafe {
        WriteFile(
            pipe,
            data.as_ptr(),
            data.len() as u32,
            &mut written,
            &mut ov,
        )
    };
    if ok == 0 {
        // SAFETY: GetLastError reads thread-local Win32 error code.
        if unsafe { GetLastError() } != ERROR_IO_PENDING {
            return false;
        }
        let rem = deadline.saturating_duration_since(Instant::now());
        let wait_ms = rem.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        // SAFETY: `ev` is a valid event handle.
        if unsafe { WaitForSingleObject(ev, wait_ms) } != WAIT_OBJECT_0 {
            return false;
        }
        // SAFETY: `pipe` and `ov` are valid and I/O completed.
        if unsafe { GetOverlappedResult(pipe, &ov, &mut written, 0) } == 0 {
            return false;
        }
    }
    written as usize == data.len()
}

fn read_line_overlapped(pipe: HANDLE, max: usize, deadline: Instant) -> Option<String> {
    // SAFETY: CreateEventW creates an unnamed manual-reset event.
    let ev = unsafe { CreateEventW(null(), 1, 0, null()) };
    if ev.is_null() {
        return None;
    }
    let _ev_guard = HandleGuard(ev);
    let mut buf = vec![0u8; max + 1];
    let mut length = 0usize;
    while length < buf.len() {
        // SAFETY: OVERLAPPED is a plain Win32 C struct where all-zero is valid before setting hEvent.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = ev;
        let mut read = 0u32;
        // SAFETY: `pipe`, `buf[length..]`, and `ov` remain valid until `GetOverlappedResult`.
        let ok = unsafe {
            ReadFile(
                pipe,
                buf[length..].as_mut_ptr(),
                (buf.len() - length) as u32,
                &mut read,
                &mut ov,
            )
        };
        if ok == 0 {
            // SAFETY: GetLastError reads thread-local Win32 error code.
            if unsafe { GetLastError() } != ERROR_IO_PENDING {
                break;
            }
            let rem = deadline.saturating_duration_since(Instant::now());
            let wait_ms = rem.as_millis().min(u128::from(u32::MAX - 1)) as u32;
            // SAFETY: `ev` is a valid event handle.
            if unsafe { WaitForSingleObject(ev, wait_ms) } != WAIT_OBJECT_0 {
                return None;
            }
            // SAFETY: `pipe` and `ov` are valid and I/O completed.
            if unsafe { GetOverlappedResult(pipe, &ov, &mut read, 0) } == 0 {
                break;
            }
        }
        if read == 0 {
            break;
        }
        let chunk = &buf[length..length + read as usize];
        if let Some(pos) = chunk.iter().position(|&b| b == b'\n') {
            length += pos;
            break;
        }
        length += read as usize;
    }
    if length == 0 || length > max {
        return None;
    }
    let s = String::from_utf8_lossy(&buf[..length]);
    Some(s.trim_end_matches('\r').to_string())
}
