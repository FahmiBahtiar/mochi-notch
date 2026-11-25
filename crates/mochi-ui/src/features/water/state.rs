use std::time::{Duration, Instant};

use chrono::Local;

use mochi_winrt::mochi_water;
use mochi_winrt::paths::{Glyphs, HudMessage};

use crate::d2d::D2dFactories;
use crate::state::NotchApp;

impl NotchApp {
    pub fn water_today_ml(&self) -> i32 {
        let key = Local::now().format("%Y-%m-%d").to_string();
        mochi_water::port::today_ml(&self.store.data.water, &key)
    }

    pub fn water_drink(&mut self, factories: Option<&D2dFactories>) {
        let cup = self.store.data.water.cup_ml;
        self.water_add(cup, factories);
    }

    pub fn water_undo(&mut self, factories: Option<&D2dFactories>) {
        let cup = self.store.data.water.cup_ml;
        self.water_add(-cup, factories);
    }

    fn water_add(&mut self, ml: i32, factories: Option<&D2dFactories>) {
        let now = Local::now();
        let key = now.format("%Y-%m-%d").to_string();
        let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();
        let outcome = mochi_water::port::add_water(&mut self.store.data.water, &key, ml, &now_iso);
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        let goal = self.store.data.water.goal_ml;
        if outcome.goal_reached_now {
            let mut msg = HudMessage::new(Glyphs::WATER, self.lang.strings().water_goal_reached);
            msg.value = Some(format!("{}/{goal} ml", outcome.today_ml));
            msg.level = Some(1.0);
            msg.duration_ms = 4000;
            msg.important = true;
            msg.celebrate = true;
            self.show_hud(msg, factories);
        }
        self.dirty = true;
    }
}
