use chrono::Local;

use mochi_winrt::mochi_updates;

use crate::state::NotchApp;

impl NotchApp {
    pub fn apply_update_tag(&mut self, tag: Option<String>) {
        self.update_checking = false;
        if let Some(ref t) = tag {
            self.store.data.updates.latest = Some(t.clone());
            self.store.data.updates.last_check =
                Some(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
            if mochi_updates::port::is_newer_version(Some(t.as_str()), env!("CARGO_PKG_VERSION")) {
                self.update_available = Some(t.clone());
            } else {
                self.update_available = None;
            }
            self.store.save_now();
        }
        self.dirty = true;
    }
}
