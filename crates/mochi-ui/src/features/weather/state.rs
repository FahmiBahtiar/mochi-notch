use mochi_winrt::weather::{WeatherPlace, WeatherReport};

use crate::d2d::D2dFactories;
use crate::state::NotchApp;

impl NotchApp {
    pub fn apply_weather_report(
        &mut self,
        report: Option<WeatherReport>,
        factories: Option<&D2dFactories>,
    ) {
        self.weather_report = report;
        self.update_targets(factories);
        self.dirty = true;
    }

    pub fn apply_weather_places(&mut self, places: Vec<WeatherPlace>) {
        self.weather_searching = false;
        self.weather_places = places;
        self.dirty = true;
    }
}
