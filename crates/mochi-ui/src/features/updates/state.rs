use chrono::Local;

use mochi_winrt::mochi_updates;
use mochi_winrt::updates::fetch_latest_release_tag;

use crate::state::NotchApp;

impl NotchApp {
    pub fn check_updates_sync(&mut self) -> Option<String> {
        self.update_checking = true;
        let tag = fetch_latest_release_tag();
        self.update_checking = false;
        if let Some(ref t) = tag {
            self.store.data.updates.latest = Some(t.clone());
            self.store.data.updates.last_check =
                Some(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
            if mochi_updates::port::is_newer_version(
                Some(t.clone()),
                env!("CARGO_PKG_VERSION").to_string(),
            )
            .unwrap_or(false)
            {
                self.update_available = Some(t.clone());
            } else {
                self.update_available = None;
            }
            self.store.save_now();
        }
        tag
    }
}
