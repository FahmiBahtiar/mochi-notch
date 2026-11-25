use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, POINT};
use windows::Win32::Graphics::Gdi::BITMAPINFOHEADER;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP};
use windows_core::{BOOL, PCWSTR};

use crate::native::to_wide;

#[derive(Debug, Clone)]
pub enum ClipboardPayload {
    Text(String),
    Files(Vec<String>),
    ImageDib {
        dib: Vec<u8>,
        width: u32,
        height: u32,
    },
}

fn open_clipboard_retry() -> bool {
    for attempt in 0..4 {
        // SAFETY: Opening clipboard without owner window.
        if unsafe { OpenClipboard(Some(HWND::default())) }.is_ok() {
            return true;
        }
        if attempt < 3 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    false
}

fn should_exclude_clipboard() -> bool {
    let excl_name = to_wide("ExcludeClipboardContentFromMonitorProcessing");
    // SAFETY: RegisterClipboardFormatW takes a NUL-terminated UTF-16 string.
    let excl_fmt = unsafe { RegisterClipboardFormatW(PCWSTR(excl_name.as_ptr())) };
    if excl_fmt != 0 && unsafe { IsClipboardFormatAvailable(excl_fmt) }.is_ok() {
        return true;
    }
    let hist_name = to_wide("CanIncludeInClipboardHistory");
    // SAFETY: RegisterClipboardFormatW takes a NUL-terminated UTF-16 string.
    let hist_fmt = unsafe { RegisterClipboardFormatW(PCWSTR(hist_name.as_ptr())) };
    if hist_fmt != 0
        && unsafe { IsClipboardFormatAvailable(hist_fmt) }.is_ok()
        && let Ok(h) = unsafe { GetClipboardData(hist_fmt) }
    {
        let hmem = HGLOBAL(h.0);
        // SAFETY: Checking DWORD flag inside `CanIncludeInClipboardHistory`.
        let size = unsafe { GlobalSize(hmem) };
        if size >= 4 {
            // SAFETY: Locking `hmem` for read.
            let ptr = unsafe { GlobalLock(hmem) };
            if !ptr.is_null() {
                // SAFETY: `size >= 4` verified above.
                let val = unsafe { *(ptr.cast::<u32>()) };
                // SAFETY: Unlocking `hmem`.
                unsafe {
                    let _ = GlobalUnlock(hmem);
                }
                if val == 0 {
                    return true;
                }
            }
        }
    }
    false
}

pub fn dib_dimensions(dib: &[u8]) -> Option<(u32, u32)> {
    if dib.len() < std::mem::size_of::<BITMAPINFOHEADER>() {
        return None;
    }
    // SAFETY: `dib` length >= `size_of::<BITMAPINFOHEADER>()` and `read_unaligned` handles alignment.
    let hdr: BITMAPINFOHEADER = unsafe { std::ptr::read_unaligned(dib.as_ptr().cast()) };
    let w = hdr.biWidth.unsigned_abs();
    let h = hdr.biHeight.unsigned_abs();
    if w == 0 || h == 0 || w > 16384 || h > 16384 {
        return None;
    }
    Some((w, h))
}

pub fn dib_to_bmp(dib: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = dib_dimensions(dib)?;
    // SAFETY: `dib` length >= `size_of::<BITMAPINFOHEADER>()` verified by `dib_dimensions`.
    let hdr: BITMAPINFOHEADER = unsafe { std::ptr::read_unaligned(dib.as_ptr().cast()) };
    let header_size = hdr
        .biSize
        .max(std::mem::size_of::<BITMAPINFOHEADER>() as u32);
    let palette_entries = if hdr.biClrUsed > 0 {
        hdr.biClrUsed
    } else if hdr.biBitCount <= 8 {
        1u32 << hdr.biBitCount
    } else if hdr.biCompression == 3 && header_size == 40 {
        3
    } else {
        0
    };
    let off_bits = 14u32
        .saturating_add(header_size)
        .saturating_add(palette_entries.saturating_mul(4));
    let file_size = 14u32.saturating_add(dib.len() as u32);
    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&off_bits.to_le_bytes());
    bmp.extend_from_slice(dib);
    Some((bmp, w, h))
}

pub fn read_clipboard_payload() -> Option<(u32, ClipboardPayload)> {
    // SAFETY: Querying clipboard sequence number.
    let seq = unsafe { GetClipboardSequenceNumber() };
    if !open_clipboard_retry() {
        return None;
    }
    let result = (|| -> Option<ClipboardPayload> {
        if should_exclude_clipboard() {
            return None;
        }
        // 1. Copied files (CF_HDROP)
        if unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP.0)) }.is_ok()
            && let Ok(h) = unsafe { GetClipboardData(u32::from(CF_HDROP.0)) }
        {
            let hdrop = HDROP(h.0);
            // SAFETY: Querying file count from clipboard HDROP.
            let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
            if count > 0 {
                let mut paths = Vec::with_capacity((count as usize).min(32));
                for i in 0..count.min(32) {
                    // SAFETY: Querying path length.
                    let len = unsafe { DragQueryFileW(hdrop, i, None) };
                    let mut buf = vec![0u16; len as usize + 1];
                    // SAFETY: Reading path into `buf`.
                    unsafe {
                        DragQueryFileW(hdrop, i, Some(&mut buf));
                    }
                    let p = String::from_utf16_lossy(&buf[..len as usize]);
                    if !p.is_empty() {
                        paths.push(p);
                    }
                }
                if !paths.is_empty() {
                    return Some(ClipboardPayload::Files(paths));
                }
            }
        }

        // 2. Copied text (CF_UNICODETEXT)
        if unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)) }.is_ok()
            && let Ok(h) = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }
        {
            let hmem = HGLOBAL(h.0);
            // SAFETY: Querying global memory size.
            let bytes = unsafe { GlobalSize(hmem) };
            if bytes >= 2 {
                // SAFETY: Locking clipboard text handle.
                let ptr = unsafe { GlobalLock(hmem) };
                if !ptr.is_null() {
                    let max_units = (bytes / 2).min(32 * 1024);
                    // SAFETY: `ptr` points to at least `max_units` UTF-16 code units.
                    let slice = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), max_units) };
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                    let text = String::from_utf16_lossy(&slice[..end]);
                    // SAFETY: Unlocking clipboard handle.
                    unsafe {
                        let _ = GlobalUnlock(hmem);
                    }
                    if !text.trim().is_empty() {
                        return Some(ClipboardPayload::Text(text));
                    }
                }
            }
        }

        // 3. Copied image (CF_DIB)
        if unsafe { IsClipboardFormatAvailable(u32::from(CF_DIB.0)) }.is_ok()
            && let Ok(h) = unsafe { GetClipboardData(u32::from(CF_DIB.0)) }
        {
            let hmem = HGLOBAL(h.0);
            // SAFETY: Querying DIB size.
            let bytes = unsafe { GlobalSize(hmem) };
            if bytes >= std::mem::size_of::<BITMAPINFOHEADER>() && bytes <= 24 * 1024 * 1024 {
                // SAFETY: Locking DIB handle.
                let ptr = unsafe { GlobalLock(hmem) };
                if !ptr.is_null() {
                    // SAFETY: `ptr` points to `bytes` bytes of DIB data.
                    let dib =
                        unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), bytes) }.to_vec();
                    // SAFETY: Unlocking DIB handle.
                    unsafe {
                        let _ = GlobalUnlock(hmem);
                    }
                    if let Some((width, height)) = dib_dimensions(&dib) {
                        return Some(ClipboardPayload::ImageDib { dib, width, height });
                    }
                }
            }
        }
        None
    })();
    // SAFETY: Closing clipboard opened above.
    unsafe {
        let _ = CloseClipboard();
    }
    result.map(|p| (seq, p))
}

pub fn write_clipboard_payload(payload: &ClipboardPayload) -> Option<u32> {
    if !open_clipboard_retry() {
        return None;
    }
    // SAFETY: Clipboard is open; allocating GMEM_MOVEABLE buffer and transferring ownership to Windows.
    let ok = unsafe {
        let _ = EmptyClipboard();
        match payload {
            ClipboardPayload::Text(text) => {
                let wide = to_wide(text);
                let bytes = wide.len() * 2;
                if let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
                    let ptr = GlobalLock(hmem);
                    if !ptr.is_null() {
                        std::ptr::copy_nonoverlapping(
                            wide.as_ptr().cast::<u8>(),
                            ptr.cast::<u8>(),
                            bytes,
                        );
                        let _ = GlobalUnlock(hmem);
                        if SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(hmem.0)))
                            .is_ok()
                        {
                            true
                        } else {
                            let _ = GlobalFree(Some(hmem));
                            false
                        }
                    } else {
                        let _ = GlobalFree(Some(hmem));
                        false
                    }
                } else {
                    false
                }
            }
            ClipboardPayload::ImageDib { dib, .. } => {
                if let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, dib.len()) {
                    let ptr = GlobalLock(hmem);
                    if !ptr.is_null() {
                        std::ptr::copy_nonoverlapping(dib.as_ptr(), ptr.cast::<u8>(), dib.len());
                        let _ = GlobalUnlock(hmem);
                        if SetClipboardData(u32::from(CF_DIB.0), Some(HANDLE(hmem.0))).is_ok() {
                            true
                        } else {
                            let _ = GlobalFree(Some(hmem));
                            false
                        }
                    } else {
                        let _ = GlobalFree(Some(hmem));
                        false
                    }
                } else {
                    false
                }
            }
            ClipboardPayload::Files(paths) => {
                let mut wide_paths: Vec<u16> = Vec::new();
                for p in paths {
                    wide_paths.extend(p.encode_utf16());
                    wide_paths.push(0);
                }
                wide_paths.push(0);
                let hdr_size = std::mem::size_of::<DROPFILES>();
                let total = hdr_size + wide_paths.len() * 2;
                if let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, total) {
                    let ptr = GlobalLock(hmem);
                    if !ptr.is_null() {
                        let df = DROPFILES {
                            pFiles: hdr_size as u32,
                            pt: POINT::default(),
                            fNC: BOOL(0),
                            fWide: BOOL(1),
                        };
                        std::ptr::write_unaligned(ptr.cast::<DROPFILES>(), df);
                        std::ptr::copy_nonoverlapping(
                            wide_paths.as_ptr().cast::<u8>(),
                            ptr.cast::<u8>().add(hdr_size),
                            wide_paths.len() * 2,
                        );
                        let _ = GlobalUnlock(hmem);
                        if SetClipboardData(u32::from(CF_HDROP.0), Some(HANDLE(hmem.0))).is_ok() {
                            true
                        } else {
                            let _ = GlobalFree(Some(hmem));
                            false
                        }
                    } else {
                        let _ = GlobalFree(Some(hmem));
                        false
                    }
                } else {
                    false
                }
            }
        }
    };
    // SAFETY: Closing clipboard opened above.
    unsafe {
        let _ = CloseClipboard();
    }
    if ok {
        // SAFETY: Querying updated clipboard sequence number.
        Some(unsafe { GetClipboardSequenceNumber() })
    } else {
        None
    }
}
