use std::time::{Duration, Instant};

use mochi_winrt::mochi_shelf::port::{
    MAX_SHELF_ITEMS, MAX_THUMB_BYTES, is_image_extension, shelf_display_name,
};
use mochi_winrt::native::{IconBitmap, load_shell_icon};
use mochi_winrt::paths::TaskData;

use crate::d2d::D2dFactories;
use crate::state::{NotchApp, NotchPage};
use crate::theme::DecodedFrame;

#[derive(Debug, Clone)]
pub struct ShelfEntry {
    pub path: String,
    pub name: String,
    pub available: bool,
    pub icon: Option<IconBitmap>,
    pub image_id: u64,
    pub decoded: Option<DecodedFrame>,
}

#[derive(Debug, Clone)]
pub struct UndoState {
    pub message: String,
    pub expires_at: Instant,
    pub kind: UndoKind,
}

#[derive(Debug, Clone)]
pub enum UndoKind {
    Task { item: TaskData, index: usize },
    Shelf { items: Vec<ShelfEntry> },
}

impl NotchApp {
    pub fn load_shelf_from_store(&mut self) {
        let paths = self.store.data.shelf.clone();
        self.shelf_items.clear();
        for p in paths {
            let exists = std::path::Path::new(&p).exists();
            let name = shelf_display_name(&p);
            let icon = load_shell_icon(&p, !exists);
            self.next_image_id += 1;
            self.shelf_items.push(ShelfEntry {
                path: p,
                name,
                available: exists,
                icon,
                image_id: self.next_image_id,
                decoded: None,
            });
        }
    }

    pub fn apply_undo(&mut self) {
        let Some(u) = self.undo.take() else {
            return;
        };
        match u.kind {
            UndoKind::Task { mut item, index } => {
                if let Some(ref gid) = item.google_id {
                    if let Some(pos) = self.store.data.task_deletes.iter().position(|x| x == gid) {
                        self.store.data.task_deletes.remove(pos);
                    } else {
                        item.google_id = None;
                        item.dirty = false;
                    }
                }
                let at = index.min(self.store.data.tasks.len());
                self.store.data.tasks.insert(at, item);
            }
            UndoKind::Shelf { items } => {
                for entry in items {
                    if self.shelf_items.len() < MAX_SHELF_ITEMS
                        && !self
                            .shelf_items
                            .iter()
                            .any(|x| x.path.eq_ignore_ascii_case(&entry.path))
                    {
                        self.shelf_items.push(entry);
                    }
                }
                self.store.data.shelf = self.shelf_items.iter().map(|x| x.path.clone()).collect();
            }
        }
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }

    pub fn on_drag_approach(
        &mut self,
        cursor_dip: (f64, f64),
        proximity: f64,
        factories: Option<&D2dFactories>,
    ) {
        let prox = proximity.clamp(0.15, 1.0);
        let changed = !self.drag_approaching || (self.drag_approach_target - prox).abs() > 0.03;
        self.drag_approaching = true;
        self.drag_approach_target = prox;
        self.drag_cursor_dip = Some(cursor_dip);
        if !self.expanded && changed {
            self.update_targets(factories);
        }
        self.dirty = true;
    }

    pub fn clear_drag_approach(&mut self, factories: Option<&D2dFactories>) {
        if !self.drag_approaching {
            return;
        }
        self.drag_approaching = false;
        self.drag_approach_target = 0.0;
        if !self.drop_hinting {
            self.drag_cursor_dip = None;
        }
        self.update_targets(factories);
        self.dirty = true;
    }

    pub fn on_drag_enter(
        &mut self,
        file_count: usize,
        cursor_dip: (f64, f64),
        factories: Option<&D2dFactories>,
    ) {
        self.drag_approaching = false;
        self.drag_approach_target = 0.0;
        self.drag_file_count = file_count.max(1);
        self.drag_cursor_dip = Some(cursor_dip);
        self.hover_deadline = None;
        self.pending_expand = false;
        self.pinned_until = None;
        if !self.drop_hinting {
            self.drag_opened_notch = !self.expanded;
            self.drag_prev_page = if self.expanded && self.page != NotchPage::Shelf {
                Some(self.page)
            } else {
                None
            };
        }
        self.drop_hinting = true;
        self.drop_companion.perch();
        self.select_page(NotchPage::Shelf, factories);
        self.set_expanded(true, factories);
        self.dirty = true;
    }

    pub fn on_drag_over(&mut self, cursor_dip: (f64, f64)) {
        self.drag_cursor_dip = Some(cursor_dip);
        self.hover_deadline = None;
        self.dirty = true;
    }

    pub fn on_drag_leave(&mut self, factories: Option<&D2dFactories>) {
        self.drop_hinting = false;
        self.drag_approaching = false;
        self.drag_approach_target = 0.0;
        self.drag_cursor_dip = None;
        self.drop_companion.stop();
        if self.drag_opened_notch {
            self.drag_opened_notch = false;
            self.drag_prev_page = None;
            self.set_expanded(false, factories);
        } else if let Some(prev) = self.drag_prev_page.take() {
            self.select_page(prev, factories);
        }
        self.dirty = true;
    }

    pub fn on_drag_drop(
        &mut self,
        paths: &[String],
        cursor_dip: Option<(f64, f64)>,
        factories: Option<&D2dFactories>,
    ) {
        let entries = Self::prepare_shelf_entries(paths, factories);
        self.on_drag_drop_entries(entries, cursor_dip, factories);
    }

    pub fn on_drag_drop_entries(
        &mut self,
        entries: Vec<ShelfEntry>,
        cursor_dip: Option<(f64, f64)>,
        factories: Option<&D2dFactories>,
    ) {
        let total_dropped = entries.len();
        let added = self.insert_shelf_entries(entries);
        self.drop_hinting = false;
        self.drag_approaching = false;
        self.drag_approach_target = 0.0;
        self.drag_opened_notch = false;
        self.drag_prev_page = None;
        self.drop_burst_t = if self.reduce_motion { 1.0 } else { 0.0 };
        self.drop_burst_pos = cursor_dip.or(self.drag_cursor_dip);
        self.drag_cursor_dip = None;
        self.shelf_flash_t = if self.reduce_motion || added == 0 {
            0.0
        } else {
            1.0
        };
        self.shelf_flash_count = added;
        self.drop_celebrate_count = added.max(total_dropped.min(1));
        self.drop_companion.nod();
        self.select_page(NotchPage::Shelf, factories);
        self.pinned_until = Some(Instant::now() + Duration::from_secs(5));
        self.set_expanded(true, factories);
        self.dirty = true;
    }

    pub fn prepare_shelf_entries(
        paths: &[String],
        factories: Option<&D2dFactories>,
    ) -> Vec<ShelfEntry> {
        let mut out = Vec::new();
        for p in paths {
            if out
                .iter()
                .any(|x: &ShelfEntry| x.path.eq_ignore_ascii_case(p))
            {
                continue;
            }
            let path = std::path::Path::new(p);
            if !path.exists() {
                continue;
            }
            let name = shelf_display_name(p);
            let decoded = if is_image_extension(p)
                && let Some(f) = factories
                && std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_THUMB_BYTES)
                && let Ok(bytes) = std::fs::read(path)
            {
                f.decode_thumbnail_bytes(&bytes, 256)
            } else {
                None
            };
            let icon = if decoded.is_none() {
                load_shell_icon(p, false)
            } else {
                None
            };
            out.push(ShelfEntry {
                path: p.clone(),
                name,
                available: true,
                icon,
                image_id: 0,
                decoded,
            });
        }
        out
    }

    pub fn insert_shelf_entries(&mut self, entries: Vec<ShelfEntry>) -> usize {
        let mut added = 0usize;
        for mut entry in entries {
            if self
                .shelf_items
                .iter()
                .any(|x| x.path.eq_ignore_ascii_case(&entry.path))
            {
                continue;
            }
            self.next_image_id += 1;
            entry.image_id = self.next_image_id;
            self.shelf_items.insert(0, entry);
            added += 1;
        }
        self.shelf_items.truncate(MAX_SHELF_ITEMS);
        self.store.data.shelf = self.shelf_items.iter().map(|x| x.path.clone()).collect();
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
        added
    }

    pub fn add_shelf_paths(&mut self, paths: &[String], factories: Option<&D2dFactories>) -> usize {
        let entries = Self::prepare_shelf_entries(paths, factories);
        self.insert_shelf_entries(entries)
    }

    pub fn remove_shelf_item(&mut self, idx: usize) {
        if idx < self.shelf_items.len() {
            self.shelf_items.remove(idx);
            self.store.data.shelf = self.shelf_items.iter().map(|x| x.path.clone()).collect();
            self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
            self.dirty = true;
        }
    }

    pub fn clear_shelf(&mut self) {
        let cleared = std::mem::take(&mut self.shelf_items);
        self.store.data.shelf.clear();
        self.undo = Some(UndoState {
            message: self.lang.strings().undo_shelf_cleared.to_string(),
            expires_at: Instant::now() + Duration::from_secs(6),
            kind: UndoKind::Shelf { items: cleared },
        });
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        self.dirty = true;
    }
}
