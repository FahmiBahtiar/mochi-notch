//! Pure clipboard history metadata formatting and pin-preserving ordering.

#![forbid(unsafe_code)]

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Files,
    Image { width: u32, height: u32 },
}

pub fn make_text_clipboard_meta(text: &str) -> (String, String, String) {
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_else(|| text.trim());
    let title: String = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = text.chars().count();
    let lines = text.lines().count().max(1);
    let subtitle = if lines > 1 {
        format!("{lines} lines · {chars} chars")
    } else {
        format!("{chars} chars")
    };
    let preview: String = text.chars().take(4000).collect();
    (title, subtitle, preview)
}

pub fn make_files_clipboard_meta(paths: &[String]) -> (String, String, String) {
    if paths.is_empty() {
        return (String::new(), String::new(), String::new());
    }
    let first_name = Path::new(&paths[0])
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| paths[0].clone());
    let title = if paths.len() == 1 {
        first_name
    } else {
        format!("{first_name} (+{})", paths.len() - 1)
    };
    let subtitle = if paths.len() == 1 {
        Path::new(&paths[0])
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        format!("{} files", paths.len())
    };
    let preview = paths.join("\n");
    (title, subtitle, preview)
}

pub fn sort_pinned_first<T>(items: &mut Vec<T>, is_pinned: impl Fn(&T) -> bool, max_len: usize) {
    let mut pinned = Vec::new();
    let mut unpinned = Vec::new();
    for item in items.drain(..) {
        if is_pinned(&item) {
            pinned.push(item);
        } else {
            unpinned.push(item);
        }
    }
    items.extend(pinned);
    items.extend(unpinned);
    items.truncate(max_len);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_meta_summarizes_multiline_snippets() {
        let (title, sub, preview) = make_text_clipboard_meta("  hello   world \nsecond line");
        assert_eq!(title, "hello world");
        assert_eq!(sub, "2 lines · 28 chars");
        assert!(preview.contains("second line"));
    }

    #[test]
    fn files_meta_formats_single_and_multiple_paths() {
        let (t1, s1, _) = make_files_clipboard_meta(&[r"C:\docs\notes.md".to_string()]);
        assert_eq!(t1, "notes.md");
        assert_eq!(s1, r"C:\docs");

        let (t2, s2, _) = make_files_clipboard_meta(&[
            r"C:\docs\a.txt".to_string(),
            r"C:\docs\b.txt".to_string(),
        ]);
        assert_eq!(t2, "a.txt (+1)");
        assert_eq!(s2, "2 files");
    }

    #[test]
    fn sort_pinned_first_preserves_pinned_items_at_top() {
        let mut items = vec![("a", false), ("b", true), ("c", false), ("d", true)];
        sort_pinned_first(&mut items, |x| x.1, 10);
        assert_eq!(
            items,
            vec![("b", true), ("d", true), ("a", false), ("c", false)]
        );
    }
}
