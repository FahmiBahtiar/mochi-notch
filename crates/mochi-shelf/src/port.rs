//! Pure file shelf path helpers and deduplication rules.

#![forbid(unsafe_code)]

use std::path::Path;

pub const MAX_SHELF_ITEMS: usize = 30;
pub const MAX_THUMB_BYTES: u64 = 25 * 1024 * 1024;

pub fn is_image_extension(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tif" | "tiff"
    )
}

pub fn shelf_display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| path.to_string())
}

pub fn dedup_shelf_paths(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for p in incoming {
        if existing.iter().any(|x| x.eq_ignore_ascii_case(p))
            || out.iter().any(|x: &String| x.eq_ignore_ascii_case(p))
        {
            continue;
        }
        out.push(p.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_image_extensions_and_dedups_case_insensitively() {
        assert!(is_image_extension(r"C:\pics\photo.PNG"));
        assert!(!is_image_extension(r"C:\docs\notes.md"));
        assert_eq!(shelf_display_name(r"C:\docs\notes.md"), "notes.md");

        let existing = vec![r"C:\Docs\A.txt".to_string()];
        let incoming = vec![
            r"c:\docs\a.txt".to_string(),
            r"C:\Docs\B.txt".to_string(),
            r"C:\Docs\b.txt".to_string(),
        ];
        assert_eq!(
            dedup_shelf_paths(&existing, &incoming),
            vec![r"C:\Docs\B.txt".to_string()]
        );
    }
}
