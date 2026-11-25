use std::ffi::c_void;
use std::io::Write;
use std::path::Path;

use url::Url;
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_LOCATION, WINHTTP_QUERY_STATUS_CODE,
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
};
use windows_core::PCWSTR;

use crate::native::to_wide;

pub struct HttpResponse {
    pub status: u32,
    pub body: String,
}

struct WinHttpHandle(*mut c_void);
impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` is a valid WinHTTP handle returned by WinHttpOpen/Connect/OpenRequest.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

pub fn http_request(
    method: &str,
    url_str: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
    timeout_ms: i32,
) -> Result<HttpResponse, String> {
    let parsed = Url::parse(url_str).map_err(|e| e.to_string())?;
    let host = parsed.host_str().ok_or("missing host")?;
    let is_https = parsed.scheme().eq_ignore_ascii_case("https");
    let port = parsed
        .port_or_known_default()
        .unwrap_or(if is_https { 443 } else { 80 });
    let mut path = parsed.path().to_string();
    if let Some(q) = parsed.query() {
        path.push('?');
        path.push_str(q);
    }

    let ua = to_wide("Mochi/1.0");
    // SAFETY: `ua` is NUL-terminated UTF-16.
    let session = unsafe {
        WinHttpOpen(
            PCWSTR(ua.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    if session.is_null() {
        return Err("WinHttpOpen failed".to_string());
    }
    let _session_guard = WinHttpHandle(session);
    // SAFETY: `session` is valid.
    unsafe {
        let _ = WinHttpSetTimeouts(session, timeout_ms, timeout_ms, timeout_ms, timeout_ms);
    }

    let host_w = to_wide(host);
    // SAFETY: `session` and `host_w` are valid.
    let conn = unsafe { WinHttpConnect(session, PCWSTR(host_w.as_ptr()), port, 0) };
    if conn.is_null() {
        return Err("WinHttpConnect failed".to_string());
    }
    let _conn_guard = WinHttpHandle(conn);

    let method_w = to_wide(method);
    let path_w = to_wide(&path);
    let flags = if is_https {
        WINHTTP_FLAG_SECURE
    } else {
        WINHTTP_OPEN_REQUEST_FLAGS(0)
    };
    // SAFETY: `conn`, `method_w`, and `path_w` are valid.
    let req = unsafe {
        WinHttpOpenRequest(
            conn,
            PCWSTR(method_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    };
    if req.is_null() {
        return Err("WinHttpOpenRequest failed".to_string());
    }
    let _req_guard = WinHttpHandle(req);

    for (k, v) in headers {
        let line = to_wide(&format!("{k}: {v}\r\n"));
        // SAFETY: `req` and `line` are valid; `-1` tells WinHTTP to compute length from NUL.
        unsafe {
            let _ = WinHttpAddRequestHeaders(req, &line[..line.len() - 1], 0x2000_0000);
        }
    }

    let (ptr, len) = match body {
        Some(b) if !b.is_empty() => (Some(b.as_ptr().cast::<c_void>()), b.len() as u32),
        _ => (None, 0),
    };
    // SAFETY: `req` and optional `ptr` (`len` bytes) are valid for the synchronous send.
    if unsafe { WinHttpSendRequest(req, None, ptr, len, len, 0) }.is_err() {
        return Err("WinHttpSendRequest failed".to_string());
    }
    // SAFETY: `req` is valid.
    if unsafe { WinHttpReceiveResponse(req, std::ptr::null_mut()) }.is_err() {
        return Err("WinHttpReceiveResponse failed".to_string());
    }

    let mut status = 0u32;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    // SAFETY: `req` is valid and `status` is a 4-byte u32.
    let _ = unsafe {
        WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &raw mut status_len,
            std::ptr::null_mut(),
        )
    };

    let mut out = Vec::new();
    loop {
        let mut avail = 0u32;
        // SAFETY: `req` is valid.
        if unsafe { WinHttpQueryDataAvailable(req, &raw mut avail) }.is_err() || avail == 0 {
            break;
        }
        let mut chunk = vec![0u8; avail as usize];
        let mut read = 0u32;
        // SAFETY: `req` is valid and `chunk` has `avail` bytes.
        if unsafe { WinHttpReadData(req, chunk.as_mut_ptr().cast(), avail, &raw mut read) }.is_err()
            || read == 0
        {
            break;
        }
        out.extend_from_slice(&chunk[..read as usize]);
        // Cap response body at 32 MB to prevent OOM on malicious/broken servers.
        if out.len() > 32 * 1024 * 1024 {
            break;
        }
    }

    Ok(HttpResponse {
        status,
        body: String::from_utf8_lossy(&out).into_owned(),
    })
}

fn query_location_header(req: *mut c_void) -> Option<String> {
    let mut bytes = 0u32;
    // SAFETY: Querying required buffer size for `Location` header.
    let _ = unsafe {
        WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_LOCATION,
            PCWSTR::null(),
            None,
            &raw mut bytes,
            std::ptr::null_mut(),
        )
    };
    if bytes == 0 || bytes > 16 * 1024 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    let mut actual = (buf.len() * 2) as u32;
    // SAFETY: `req` is valid and `buf` has `actual` bytes capacity.
    if unsafe {
        WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_LOCATION,
            PCWSTR::null(),
            Some(buf.as_mut_ptr().cast()),
            &raw mut actual,
            std::ptr::null_mut(),
        )
    }
    .is_err()
    {
        return None;
    }
    let units = (actual as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..units]);
    let trimmed = s.trim_end_matches('\0').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn download_stream(
    url_str: &str,
    headers: &[(&str, &str)],
    timeout_ms: i32,
    max_bytes: usize,
    allow_url: fn(&str) -> bool,
    sink: &mut dyn Write,
) -> Result<u64, String> {
    let mut current_url = url_str.to_string();
    for _hop in 0..=5 {
        if !allow_url(&current_url) {
            return Err("Untrusted download URL".to_string());
        }
        let parsed = Url::parse(&current_url).map_err(|e| e.to_string())?;
        let host = parsed.host_str().ok_or("missing host")?;
        let is_https = parsed.scheme().eq_ignore_ascii_case("https");
        let port = parsed
            .port_or_known_default()
            .unwrap_or(if is_https { 443 } else { 80 });
        let mut path = parsed.path().to_string();
        if let Some(q) = parsed.query() {
            path.push('?');
            path.push_str(q);
        }

        let ua = to_wide("Mochi/1.0");
        // SAFETY: `ua` is NUL-terminated UTF-16.
        let session = unsafe {
            WinHttpOpen(
                PCWSTR(ua.as_ptr()),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        };
        if session.is_null() {
            return Err("WinHttpOpen failed".to_string());
        }
        let _session_guard = WinHttpHandle(session);
        // SAFETY: `session` is valid.
        unsafe {
            let _ = WinHttpSetTimeouts(session, timeout_ms, timeout_ms, timeout_ms, timeout_ms);
        }

        let host_w = to_wide(host);
        // SAFETY: `session` and `host_w` are valid.
        let conn = unsafe { WinHttpConnect(session, PCWSTR(host_w.as_ptr()), port, 0) };
        if conn.is_null() {
            return Err("WinHttpConnect failed".to_string());
        }
        let _conn_guard = WinHttpHandle(conn);

        let method_w = to_wide("GET");
        let path_w = to_wide(&path);
        let flags = if is_https {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        };
        // SAFETY: `conn`, `method_w`, and `path_w` are valid.
        let req = unsafe {
            WinHttpOpenRequest(
                conn,
                PCWSTR(method_w.as_ptr()),
                PCWSTR(path_w.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                flags,
            )
        };
        if req.is_null() {
            return Err("WinHttpOpenRequest failed".to_string());
        }
        let _req_guard = WinHttpHandle(req);

        let policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_ne_bytes();
        // SAFETY: `req` is valid and `policy` is a 4-byte DWORD option value.
        unsafe {
            let _ = WinHttpSetOption(Some(req), WINHTTP_OPTION_REDIRECT_POLICY, Some(&policy));
        }

        for (k, v) in headers {
            let line = to_wide(&format!("{k}: {v}\r\n"));
            // SAFETY: `req` and `line` are valid.
            unsafe {
                let _ = WinHttpAddRequestHeaders(req, &line[..line.len() - 1], 0x2000_0000);
            }
        }

        // SAFETY: `req` is valid for synchronous GET.
        if unsafe { WinHttpSendRequest(req, None, None, 0, 0, 0) }.is_err() {
            return Err("WinHttpSendRequest failed".to_string());
        }
        // SAFETY: `req` is valid.
        if unsafe { WinHttpReceiveResponse(req, std::ptr::null_mut()) }.is_err() {
            return Err("WinHttpReceiveResponse failed".to_string());
        }

        let mut status = 0u32;
        let mut status_len = std::mem::size_of::<u32>() as u32;
        // SAFETY: `req` is valid and `status` is a 4-byte u32.
        let _ = unsafe {
            WinHttpQueryHeaders(
                req,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some((&raw mut status).cast()),
                &raw mut status_len,
                std::ptr::null_mut(),
            )
        };

        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let loc = query_location_header(req).ok_or("Redirect missing Location header")?;
            let next = parsed.join(&loc).map_err(|e| e.to_string())?;
            current_url = next.to_string();
            continue;
        }

        if !(200..300).contains(&status) {
            return Err(format!("HTTP {status}"));
        }

        let mut total = 0usize;
        loop {
            let mut avail = 0u32;
            // SAFETY: `req` is valid.
            if unsafe { WinHttpQueryDataAvailable(req, &raw mut avail) }.is_err() || avail == 0 {
                break;
            }
            let mut chunk = vec![0u8; avail as usize];
            let mut read = 0u32;
            // SAFETY: `req` is valid and `chunk` has `avail` bytes.
            if unsafe { WinHttpReadData(req, chunk.as_mut_ptr().cast(), avail, &raw mut read) }
                .is_err()
                || read == 0
            {
                break;
            }
            let n = read as usize;
            total = total
                .checked_add(n)
                .ok_or_else(|| "Download size overflow".to_string())?;
            if total > max_bytes {
                return Err("Download exceeded size limit".to_string());
            }
            sink.write_all(&chunk[..n]).map_err(|e| e.to_string())?;
        }
        sink.flush().map_err(|e| e.to_string())?;
        return Ok(total as u64);
    }
    Err("Too many redirects".to_string())
}

pub fn http_download_bytes(
    url_str: &str,
    headers: &[(&str, &str)],
    timeout_ms: i32,
    max_bytes: usize,
    allow_url: fn(&str) -> bool,
) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    download_stream(url_str, headers, timeout_ms, max_bytes, allow_url, &mut buf)?;
    Ok(buf)
}

pub fn http_download_to_file(
    url_str: &str,
    dest: &Path,
    headers: &[(&str, &str)],
    timeout_ms: i32,
    max_bytes: usize,
    allow_url: fn(&str) -> bool,
) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    let res = download_stream(
        url_str,
        headers,
        timeout_ms,
        max_bytes,
        allow_url,
        &mut writer,
    );
    if res.is_err() {
        drop(writer);
        let _ = std::fs::remove_file(dest);
    }
    res
}

pub fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub fn is_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("ms-settings:") {
        return trimmed.len() > "ms-settings:".len() && !trimmed.chars().any(char::is_control);
    }
    Url::parse(trimmed).is_ok_and(|u| matches!(u.scheme(), "https" | "http"))
}

pub fn is_safe_local_path(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("\\\\")
        || trimmed.starts_with("//")
        || trimmed.chars().any(char::is_control)
    {
        return false;
    }
    let bytes = trimmed.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

pub fn open_url(url: &str) {
    if !is_safe_external_url(url) {
        return;
    }
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = to_wide("open");
    let target = to_wide(url.trim());
    // SAFETY: `verb` and `target` are NUL-terminated UTF-16 strings.
    unsafe {
        let _ = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Opens a local drive-rooted file or directory path via `ShellExecuteW`.
/// Rejects UNC paths (`\\`) and URI schemes to prevent SMB hash leaks and scheme abuse.
pub fn open_local_path(path: &str) {
    if !is_safe_local_path(path) {
        return;
    }
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = to_wide("open");
    let target = to_wide(path.trim());
    // SAFETY: `verb` and `target` are NUL-terminated UTF-16 strings.
    unsafe {
        let _ = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{is_safe_external_url, is_safe_local_path};

    #[test]
    fn external_url_allows_only_http_https_and_ms_settings() {
        assert!(is_safe_external_url("https://meet.google.com/abc-defg-hij"));
        assert!(is_safe_external_url("http://127.0.0.1:8080/cb"));
        assert!(is_safe_external_url("ms-settings:notifications"));
        assert!(!is_safe_external_url("file:///C:/Windows/System32/cmd.exe"));
        assert!(!is_safe_external_url("ms-msdt:/id PCWDiagnostic"));
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url(r"\\attacker\share\payload.exe"));
    }

    #[test]
    fn local_path_allows_drive_paths_and_rejects_unc_and_schemes() {
        assert!(is_safe_local_path(r"C:\Users\demo\notes.md"));
        assert!(is_safe_local_path("D:/Projects/Mochi"));
        assert!(!is_safe_local_path(r"\\attacker\share\file.txt"));
        assert!(!is_safe_local_path("//attacker/share/file.txt"));
        assert!(!is_safe_local_path("https://example.com"));
        assert!(!is_safe_local_path("ms-msdt:foo"));
    }
}
