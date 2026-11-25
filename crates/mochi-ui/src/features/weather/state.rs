use mochi_winrt::weather::{fetch_weather, search_weather_places};

use crate::state::NotchApp;

impl NotchApp {
    pub fn refresh_weather_sync(&mut self) {
        if let (Some(lat), Some(lon)) = (
            self.store.data.settings.weather_latitude,
            self.store.data.settings.weather_longitude,
        ) {
            self.weather_report = fetch_weather(lat, lon, self.lang);
            self.update_targets(None);
            self.dirty = true;
        }
    }

    pub fn search_weather(&mut self, query: &str) {
        self.weather_searching = true;
        self.weather_places = search_weather_places(query, self.lang);
        self.weather_searching = false;
    }
}
