pub mod agent_setup;
pub mod audio;
pub mod calendar;
pub mod clipboard;
pub mod google;
pub mod http;
pub mod location;
pub mod lyrics;
pub mod media;
pub mod monitors;
pub mod native;
pub mod notifications;
pub mod paths;
pub mod pipe;
pub mod strings;
pub mod tasks;
pub mod updates;
pub mod watchers;
pub mod weather;

pub use mochi_activities;
pub use mochi_calendar;
pub use mochi_clipboard;
pub use mochi_focus;
pub use mochi_gaze;
pub use mochi_lyrics;
pub use mochi_media;
pub use mochi_notifications;
pub use mochi_prayer;
pub use mochi_presence;
pub use mochi_shelf;
pub use mochi_spring;
pub use mochi_store;
pub use mochi_tasks;
pub use mochi_updates;
pub use mochi_water;
pub use mochi_weather;

#[cfg(test)]
mod tests {
    use crate::monitors::Monitors;
    use crate::paths::{AppStore, SecretFile};
    use crate::strings::Language;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct DummySecret {
        token: String,
    }

    #[test]
    fn strings_en_and_id_loaded() {
        assert_eq!(Language::En.strings().welcome_title, "Welcome to Mochi");
        assert_eq!(
            Language::Id.strings().welcome_title,
            "Selamat datang di Mochi"
        );
        assert_eq!(Language::resolve(Some("en")), Language::En);
        assert_eq!(Language::resolve(Some("id")), Language::Id);
        assert_eq!(Language::resolve(Some("id-ID")), Language::Id);
    }

    #[test]
    fn dpapi_secret_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mochi-winrt-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("secret.dat");
        let orig = DummySecret {
            token: "refresh-123".to_string(),
        };
        assert!(SecretFile::write(&file, &orig));
        let read: Option<DummySecret> = SecretFile::read(&file);
        assert_eq!(read, Some(orig));
        SecretFile::delete(&file);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_store_recovers_from_corrupt_json() {
        let dir = std::env::temp_dir().join(format!("mochi-store-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("data.json");
        std::fs::write(&file, "{broken").unwrap();
        let store = AppStore::load_from(file.clone());
        assert!(store.recovered_from_damage);
        assert!(file.with_extension("json.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn monitors_returns_at_least_one_display() {
        let all = Monitors::all(Language::En);
        assert!(!all.is_empty());
    }
}
