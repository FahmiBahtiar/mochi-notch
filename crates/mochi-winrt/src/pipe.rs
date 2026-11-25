use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, HLOCAL,
    INVALID_HANDLE_VALUE, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, OpenProcessToken};
use windows_core::{PCWSTR, PWSTR};

use crate::native::to_wide;

pub struct ActivityPipe {
    stop: Arc<AtomicBool>,
}

impl ActivityPipe {
    pub fn start(handler: Arc<dyn Fn(String) -> String + Send + Sync>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("mochi-pipe".to_string())
            .stack_size(256 * 1024)
            .spawn(move || {
                let Some(pipe_name) = default_pipe_name() else {
                    return;
                };
                let Some(sddl_str) = current_user_sddl() else {
                    return;
                };
                let full = to_wide(&format!("\\\\.\\pipe\\{pipe_name}"));
                let wsddl = to_wide(&sddl_str);
                let active_clients = Arc::new(AtomicUsize::new(0));
                const MAX_PIPE_CLIENTS: usize = 16;
                let mut first_instance = true;

                while !stop_bg.load(Ordering::Relaxed) {
                    let mut sd = PSECURITY_DESCRIPTOR::default();
                    // SAFETY: Converting SDDL string to security descriptor; freed with LocalFree.
                    if unsafe {
                        ConvertStringSecurityDescriptorToSecurityDescriptorW(
                            PCWSTR(wsddl.as_ptr()),
                            SDDL_REVISION_1,
                            &raw mut sd,
                            None,
                        )
                    }
                    .is_err()
                        || sd.0.is_null()
                    {
                        return;
                    }
                    let sa = SECURITY_ATTRIBUTES {
                        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                        lpSecurityDescriptor: sd.0,
                        bInheritHandle: false.into(),
                    };
                    let open_mode = if first_instance {
                        PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE
                    } else {
                        PIPE_ACCESS_DUPLEX
                    };
                    // SAFETY: Creating named pipe instance with strict user-only DACL and remote-client rejection.
                    let pipe = unsafe {
                        CreateNamedPipeW(
                            PCWSTR(full.as_ptr()),
                            open_mode,
                            PIPE_TYPE_BYTE
                                | PIPE_READMODE_BYTE
                                | PIPE_WAIT
                                | PIPE_REJECT_REMOTE_CLIENTS,
                            4,
                            1024,
                            1024,
                            0,
                            Some(&raw const sa),
                        )
                    };
                    // SAFETY: `sd.0` was allocated by `ConvertStringSecurityDescriptorToSecurityDescriptorW`.
                    unsafe {
                        let _ = LocalFree(Some(HLOCAL(sd.0)));
                    }
                    if pipe == INVALID_HANDLE_VALUE {
                        std::thread::sleep(Duration::from_millis(250));
                        continue;
                    }
                    first_instance = false;
                    // SAFETY: Waiting for client connection on `pipe`.
                    let connected = unsafe { ConnectNamedPipe(pipe, None) }.is_ok()
                        || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                    if !connected || stop_bg.load(Ordering::Relaxed) {
                        // SAFETY: Closing unused pipe handle.
                        unsafe {
                            let _ = CloseHandle(pipe);
                        }
                        continue;
                    }
                    if active_clients.load(Ordering::Acquire) >= MAX_PIPE_CLIENTS {
                        // SAFETY: Rejecting excess concurrent client to prevent thread exhaustion.
                        unsafe {
                            let _ = DisconnectNamedPipe(pipe);
                            let _ = CloseHandle(pipe);
                        }
                        std::thread::sleep(Duration::from_millis(25));
                        continue;
                    }
                    active_clients.fetch_add(1, Ordering::AcqRel);
                    let clients_ref = Arc::clone(&active_clients);
                    let h = Arc::clone(&handler);
                    let raw_pipe = pipe.0 as usize;
                    if std::thread::Builder::new()
                        .name("mochi-pipe-client".to_string())
                        .spawn(move || {
                            struct ClientGuard(Arc<AtomicUsize>);
                            impl Drop for ClientGuard {
                                fn drop(&mut self) {
                                    self.0.fetch_sub(1, Ordering::AcqRel);
                                }
                            }
                            let _guard = ClientGuard(clients_ref);
                            let p = HANDLE(raw_pipe as *mut c_void);
                            serve_pipe_client(p, &*h);
                            // SAFETY: Disconnecting and closing client pipe handle.
                            unsafe {
                                let _ = FlushFileBuffers(p);
                                let _ = DisconnectNamedPipe(p);
                                let _ = CloseHandle(p);
                            }
                        })
                        .is_err()
                    {
                        active_clients.fetch_sub(1, Ordering::AcqRel);
                        // SAFETY: Closing pipe handle if thread spawn failed.
                        unsafe {
                            let _ = DisconnectNamedPipe(pipe);
                            let _ = CloseHandle(pipe);
                        }
                    }
                }
            })
            .ok();
        Self { stop }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for ActivityPipe {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve_pipe_client(pipe: HANDLE, handler: &dyn Fn(String) -> String) {
    let mut buf = vec![0u8; 4097];
    let mut len = 0usize;
    while len < buf.len() {
        let mut read = 0u32;
        // SAFETY: `pipe` is valid and `buf[len..]` is writable.
        let ok = unsafe { ReadFile(pipe, Some(&mut buf[len..]), Some(&raw mut read), None) };
        if ok.is_err() {
            // SAFETY: GetLastError reads thread-local Win32 error code.
            if unsafe { GetLastError() } == ERROR_BROKEN_PIPE {
                break;
            }
            break;
        }
        if read == 0 {
            break;
        }
        if let Some(pos) = buf[len..len + read as usize]
            .iter()
            .position(|&b| b == b'\n')
        {
            len += pos;
            break;
        }
        len += read as usize;
    }
    let reply = if len == 0 || len > 4096 {
        "error: request too long or empty".to_string()
    } else {
        let line = String::from_utf8_lossy(&buf[..len])
            .trim_end_matches('\r')
            .to_string();
        handler(line)
    };
    let mut out = reply.into_bytes();
    out.push(b'\n');
    let mut written = 0u32;
    // SAFETY: `pipe` is valid and `out` is readable.
    unsafe {
        let _ = WriteFile(pipe, Some(&out), Some(&raw mut written), None);
    }
}

fn current_user_sid_string() -> Option<String> {
    let mut token = HANDLE::default();
    // SAFETY: Opening current process token.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) }.is_err() {
        return None;
    }
    let mut needed = 0u32;
    // SAFETY: Querying required buffer size for TokenUser.
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, None, 0, &raw mut needed);
    }
    if needed == 0 {
        // SAFETY: Closing token handle.
        unsafe {
            let _ = CloseHandle(token);
        }
        return None;
    }
    let mut buf = vec![0u8; needed as usize];
    // SAFETY: Reading TokenUser into `buf`.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            needed,
            &raw mut needed,
        )
    };
    // SAFETY: Closing token handle.
    unsafe {
        let _ = CloseHandle(token);
    }
    if ok.is_err() {
        return None;
    }
    // SAFETY: `buf` holds a valid `TOKEN_USER`.
    let sid: PSID = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut pw = PWSTR::null();
    // SAFETY: Converting SID to string; freed with LocalFree.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut pw) }.is_err() || pw.is_null() {
        return None;
    }
    // SAFETY: `pw` is a valid NUL-terminated UTF-16 string.
    let s = unsafe { pw.to_string() }.ok();
    // SAFETY: `pw` was allocated by `ConvertSidToStringSidW`.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(pw.0.cast())));
    }
    s
}

fn default_pipe_name() -> Option<String> {
    let sid = current_user_sid_string()?;
    let mut session_id = 0u32;
    // SAFETY: Querying session ID of current process.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &raw mut session_id) }.is_err() {
        return None;
    }
    Some(format!("Mochi.{sid}.{session_id}"))
}

fn current_user_sddl() -> Option<String> {
    let sid = current_user_sid_string()?;
    Some(format!("D:(D;;GA;;;NU)(A;;GA;;;{sid})"))
}
