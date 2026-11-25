use std::time::{Duration, Instant};

use chrono::Local;

use mochi_winrt::paths::{Glyphs, HudMessage};

use crate::d2d::D2dFactories;
use crate::state::NotchApp;

impl NotchApp {
    pub fn water_today_ml(&self) -> i32 {
        let key = Local::now().format("%Y-%m-%d").to_string();
        self.store.data.water.days.get(&key).copied().unwrap_or(0)
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
        let key = Local::now().format("%Y-%m-%d").to_string();
        let before = self.water_today_ml();
        let today = (before + ml).max(0);
        self.store.data.water.days.insert(key, today);
        if ml > 0 {
            self.store.data.water.last_drink =
                Some(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
        }
        self.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
        let goal = self.store.data.water.goal_ml;
        if before < goal && today >= goal {
            let mut msg = HudMessage::new(Glyphs::WATER, self.lang.strings().water_goal_reached);
            msg.value = Some(format!("{today}/{goal} ml"));
            msg.level = Some(1.0);
            msg.duration_ms = 4000;
            msg.important = true;
            msg.celebrate = true;
            self.show_hud(msg, factories);
        }
        self.dirty = true;
    }
}
