use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime};

use mochi_winrt::clipboard::{ClipboardPayload, dib_to_bmp, write_clipboard_payload};
pub use mochi_winrt::mochi_clipboard::port::ClipboardKind;
use mochi_winrt::mochi_clipboard::port::{
    make_files_clipboard_meta, make_text_clipboard_meta, sort_pinned_first,
};
use mochi_winrt::paths::{AppPaths, ClipboardEntryData, SecretFile};

use crate::d2d::D2dFactories;
use crate::state::NotchApp;
use crate::theme::DecodedFrame;

#[derive(Debug, Clone)]
pub struct ClipboardItem {
    pub id: u64,
    pub kind: ClipboardKind,
    pub payload: ClipboardPayload,
    pub title: String,
    pub subtitle: String,
    pub preview: String,
    pub at: NaiveDateTime,
    pub pinned: bool,
    pub image_id: u64,
    pub decoded: Option<DecodedFrame>,
}

impl NotchApp {
    pub fn load_clipboard_from_store(&mut self) {
        let persist = !cfg!(test) && self.bench.is_none() && self.shots_dir.is_none();
        let mut saved = if persist {
            SecretFile::read::<Vec<ClipboardEntryData>>(&AppPaths::clipboard_file())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let had_legacy = !self.store.data.clipboard.is_empty();
        if had_legacy {
            for legacy in std::mem::take(&mut self.store.data.clipboard) {
                if !saved.iter().any(|e| e.text == legacy.text) {
                    saved.push(legacy);
                }
            }
        }
        self.clipboard_items.clear();
        for entry in saved {
            if entry.text.trim().is_empty() {
                continue;
            }
            let at = NaiveDateTime::parse_from_str(&entry.at, "%Y-%m-%dT%H:%M:%S")
                .unwrap_or_else(|_| Local::now().naive_local());
            let (title, subtitle, preview) = make_text_clipboard_meta(&entry.text);
            self.next_clipboard_id += 1;
            self.clipboard_items.push(ClipboardItem {
                id: self.next_clipboard_id,
                kind: ClipboardKind::Text,
                payload: ClipboardPayload::Text(entry.text),
                title,
                subtitle,
                preview,
                at,
                pinned: entry.pinned,
                image_id: 0,
                decoded: None,
            });
        }
        self.sort_and_trim_clipboard();
        if had_legacy && persist {
            self.sync_clipboard_store();
        }
    }

    fn sort_and_trim_clipboard(&mut self) {
        sort_pinned_first(&mut self.clipboard_items, |item| item.pinned, 40);
        const MAX_IMAGES: usize = 4;
        const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
        let mut img_count = 0usize;
        let mut img_bytes = 0usize;
        self.clipboard_items.retain(|item| {
            if let ClipboardPayload::ImageDib { dib, .. } = &item.payload {
                if item.pinned {
                    img_count += 1;
                    img_bytes += dib.len();
                    return true;
                }
                if img_count >= MAX_IMAGES || img_bytes + dib.len() > MAX_IMAGE_BYTES {
                    return false;
                }
                img_count += 1;
                img_bytes += dib.len();
            }
            true
        });
        if self.clipboard_items.is_empty() {
            self.clipboard_selected = 0;
        } else if self.clipboard_selected >= self.clipboard_items.len() {
            self.clipboard_selected = self.clipboard_items.len() - 1;
        }
    }

    fn sync_clipboard_store(&mut self) {
        let entries: Vec<ClipboardEntryData> = self
            .clipboard_items
            .iter()
            .filter_map(|item| match &item.payload {
                ClipboardPayload::Text(t) if t.len() <= 16 * 1024 => Some(ClipboardEntryData {
                    text: t.clone(),
                    pinned: item.pinned,
                    at: item.at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                }),
                _ => None,
            })
            .take(30)
            .collect();
        // Never persist clipboard text in plaintext inside data.json.
        self.store.data.clipboard.clear();
        if !cfg!(test) && self.bench.is_none() && self.shots_dir.is_none() {
            if entries.is_empty() {
                SecretFile::delete(&AppPaths::clipboard_file());
            } else {
                SecretFile::write(&AppPaths::clipboard_file(), &entries);
            }
        }
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
    }

    pub fn capture_clipboard_payload(
        &mut self,
        seq: u32,
        payload: ClipboardPayload,
        factories: Option<&D2dFactories>,
    ) {
        if seq != 0 && seq == self.last_clipboard_seq {
            return;
        }
        if seq != 0 {
            self.last_clipboard_seq = seq;
        }
        let now = Local::now().naive_local();
        let existing_idx =
            self.clipboard_items
                .iter()
                .position(|item| match (&item.payload, &payload) {
                    (ClipboardPayload::Text(a), ClipboardPayload::Text(b)) => a == b,
                    (ClipboardPayload::Files(a), ClipboardPayload::Files(b)) => a == b,
                    (
                        ClipboardPayload::ImageDib { dib: a, .. },
                        ClipboardPayload::ImageDib { dib: b, .. },
                    ) => a == b,
                    _ => false,
                });
        if let Some(idx) = existing_idx {
            let mut item = self.clipboard_items.remove(idx);
            item.at = now;
            let insert_at = if item.pinned {
                0
            } else {
                self.clipboard_items.iter().take_while(|x| x.pinned).count()
            };
            self.clipboard_items.insert(insert_at, item);
            self.clipboard_selected = insert_at;
            self.sync_clipboard_store();
            self.dirty = true;
            return;
        }

        let item = match payload {
            ClipboardPayload::Text(text) => {
                if text.trim().is_empty() {
                    return;
                }
                let (title, subtitle, preview) = make_text_clipboard_meta(&text);
                self.next_clipboard_id += 1;
                ClipboardItem {
                    id: self.next_clipboard_id,
                    kind: ClipboardKind::Text,
                    payload: ClipboardPayload::Text(text),
                    title,
                    subtitle,
                    preview,
                    at: now,
                    pinned: false,
                    image_id: 0,
                    decoded: None,
                }
            }
            ClipboardPayload::Files(paths) => {
                if paths.is_empty() {
                    return;
                }
                let (title, subtitle, preview) = make_files_clipboard_meta(&paths);
                self.next_clipboard_id += 1;
                ClipboardItem {
                    id: self.next_clipboard_id,
                    kind: ClipboardKind::Files,
                    payload: ClipboardPayload::Files(paths),
                    title,
                    subtitle,
                    preview,
                    at: now,
                    pinned: false,
                    image_id: 0,
                    decoded: None,
                }
            }
            ClipboardPayload::ImageDib { dib, width, height } => {
                let decoded = factories.and_then(|f| {
                    let (bmp, _, _) = dib_to_bmp(&dib)?;
                    f.decode_thumbnail_bytes(&bmp, 256)
                });
                self.next_image_id += 1;
                let image_id = self.next_image_id;
                self.next_clipboard_id += 1;
                ClipboardItem {
                    id: self.next_clipboard_id,
                    kind: ClipboardKind::Image { width, height },
                    payload: ClipboardPayload::ImageDib { dib, width, height },
                    title: format!("Image {width}×{height}"),
                    subtitle: "Bitmap".to_string(),
                    preview: format!("{width} × {height} px"),
                    at: now,
                    pinned: false,
                    image_id,
                    decoded,
                }
            }
        };

        let insert_at = self.clipboard_items.iter().take_while(|x| x.pinned).count();
        self.clipboard_items.insert(insert_at, item);
        self.sort_and_trim_clipboard();
        self.clipboard_selected = insert_at.min(self.clipboard_items.len().saturating_sub(1));
        self.sync_clipboard_store();
        self.dirty = true;
    }

    pub fn copy_clipboard_item(&mut self, idx: usize) {
        let Some(item) = self.clipboard_items.get(idx) else {
            return;
        };
        self.clipboard_selected = idx;
        let id = item.id;
        if let Some(seq) = write_clipboard_payload(&item.payload) {
            self.last_clipboard_seq = seq;
        }
        self.clipboard_copied_id = Some(id);
        self.clipboard_copied_until = Some(Instant::now() + Duration::from_millis(1400));
        self.dirty = true;
    }

    pub fn toggle_pin_clipboard_item(&mut self, idx: usize) {
        if idx >= self.clipboard_items.len() {
            return;
        }
        let id = self.clipboard_items[idx].id;
        self.clipboard_items[idx].pinned = !self.clipboard_items[idx].pinned;
        self.sort_and_trim_clipboard();
        if let Some(new_idx) = self.clipboard_items.iter().position(|x| x.id == id) {
            self.clipboard_selected = new_idx;
        }
        self.sync_clipboard_store();
        self.dirty = true;
    }

    pub fn remove_clipboard_item(&mut self, idx: usize) {
        if idx < self.clipboard_items.len() {
            self.clipboard_items.remove(idx);
            if self.clipboard_items.is_empty() {
                self.clipboard_selected = 0;
            } else if self.clipboard_selected >= self.clipboard_items.len() {
                self.clipboard_selected = self.clipboard_items.len() - 1;
            }
            self.sync_clipboard_store();
            self.dirty = true;
        }
    }

    pub fn clear_clipboard_history(&mut self) {
        let has_unpinned = self.clipboard_items.iter().any(|x| !x.pinned);
        if has_unpinned {
            self.clipboard_items.retain(|x| x.pinned);
        } else {
            self.clipboard_items.clear();
        }
        self.clipboard_scroll = 0.0;
        self.clipboard_selected = 0;
        self.sync_clipboard_store();
        self.dirty = true;
    }
}
