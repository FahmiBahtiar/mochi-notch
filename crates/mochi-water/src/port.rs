//! Pure daily water intake tracker logic.

#![forbid(unsafe_code)]

use mochi_store::port::WaterData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterAddOutcome {
    pub today_ml: i32,
    pub goal_reached_now: bool,
}

pub fn today_ml(water: &WaterData, day_key: &str) -> i32 {
    water.days.get(day_key).copied().unwrap_or(0)
}

pub fn add_water(water: &mut WaterData, day_key: &str, ml: i32, now_iso: &str) -> WaterAddOutcome {
    let before = today_ml(water, day_key);
    let today = (before + ml).max(0);
    water.days.insert(day_key.to_string(), today);
    if ml > 0 {
        water.last_drink = Some(now_iso.to_string());
    }
    WaterAddOutcome {
        today_ml: today,
        goal_reached_now: before < water.goal_ml && today >= water.goal_ml,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_tracker_adds_undoes_and_celebrates_goal() {
        let mut water = WaterData {
            goal_ml: 500,
            cup_ml: 250,
            ..WaterData::default()
        };
        let r1 = add_water(&mut water, "2026-11-25", 250, "2026-11-25T10:00:00");
        assert_eq!(r1.today_ml, 250);
        assert!(!r1.goal_reached_now);

        let r2 = add_water(&mut water, "2026-11-25", 250, "2026-11-25T11:00:00");
        assert_eq!(r2.today_ml, 500);
        assert!(r2.goal_reached_now);

        let r3 = add_water(&mut water, "2026-11-25", -250, "2026-11-25T11:05:00");
        assert_eq!(r3.today_ml, 250);
        assert!(!r3.goal_reached_now);
    }
}
