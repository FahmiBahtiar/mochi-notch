//! Pure daily water intake tracker and reminder logic.

#![forbid(unsafe_code)]

use std::collections::HashMap;

use serde_json::{Value, json};

const TICKS_PER_SECOND: i64 = 10_000_000;
const TICKS_PER_MINUTE: i64 = 60 * TICKS_PER_SECOND;

#[derive(Debug, Clone)]
struct HudMsg {
    text: String,
    value: Option<String>,
    celebrate: bool,
}

struct WaterTracker {
    goal_ml: i64,
    cup_ml: i64,
    reminder_minutes: i64,
    reminders_enabled: bool,
    last_drink_ticks: Option<i64>,
    last_reminder_ticks: i64,
    now_ticks: i64,
    days: HashMap<i64, i64>,
    last_msg: Option<HudMsg>,
}

impl WaterTracker {
    fn new() -> Self {
        Self {
            goal_ml: 2000,
            cup_ml: 250,
            reminder_minutes: 60,
            reminders_enabled: true,
            last_drink_ticks: None,
            last_reminder_ticks: 10 * 3600 * TICKS_PER_SECOND,
            now_ticks: 10 * 3600 * TICKS_PER_SECOND,
            days: HashMap::new(),
            last_msg: None,
        }
    }

    fn day_index(&self) -> i64 {
        self.now_ticks.div_euclid(86400 * TICKS_PER_SECOND)
    }

    fn today_ml(&self) -> i64 {
        self.days.get(&self.day_index()).copied().unwrap_or(0)
    }

    fn add(&mut self, ml: i64) {
        let before = self.today_ml();
        let today = (before + ml).max(0);
        let day = self.day_index();
        self.days.insert(day, today);
        if ml > 0 {
            self.last_drink_ticks = Some(self.now_ticks);
        }
        if before < self.goal_ml && today >= self.goal_ml {
            self.last_msg = Some(HudMsg {
                text: "Goal reached".to_string(),
                value: Some(format!("{today}/{} ml", self.goal_ml)),
                celebrate: true,
            });
        }
    }

    fn poll(&mut self) {
        let sec_of_day = self.now_ticks.rem_euclid(86400 * TICKS_PER_SECOND) / TICKS_PER_SECOND;
        let hour = sec_of_day / 3600;
        if !self.reminders_enabled || !(8..22).contains(&hour) || self.today_ml() >= self.goal_ml {
            return;
        }
        let day_start = self.day_index() * 86400 * TICKS_PER_SECOND + 8 * 3600 * TICKS_PER_SECOND;
        let since = day_start
            .max(self.last_reminder_ticks)
            .max(self.last_drink_ticks.unwrap_or(i64::MIN));
        if self.now_ticks - since < self.reminder_minutes * TICKS_PER_MINUTE {
            return;
        }
        self.last_reminder_ticks = self.now_ticks;
        self.last_msg = Some(HudMsg {
            text: "Time to drink water".to_string(),
            value: Some(format!("{}/{} ml", self.today_ml(), self.goal_ml)),
            celebrate: false,
        });
    }
}

fn seconds_to_ticks_clamped(secs: f64) -> i64 {
    (secs * (TICKS_PER_SECOND as f64)).round() as i64
}

pub fn simulate_water_tracker(steps: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
    let Some(steps) = steps else {
        return Ok(Vec::new());
    };
    let mut water = WaterTracker::new();
    let mut results = Vec::with_capacity(steps.len());

    for step in steps {
        water.last_msg = None;
        if let Some(adv) = step.get("AdvanceSeconds").and_then(Value::as_f64)
            && adv > 0.0
            && !adv.is_nan()
            && !adv.is_infinite()
        {
            water.now_ticks += seconds_to_ticks_clamped(adv.min(86400.0 * 30.0));
        }

        let op = step.get("Op").and_then(Value::as_str).unwrap_or("");
        match op {
            "drink" => water.add(water.cup_ml),
            "undo" => water.add(-water.cup_ml),
            "poll" => water.poll(),
            "settings" => {
                if let Some(g) = step.get("GoalMl").and_then(Value::as_i64) {
                    water.goal_ml = g.clamp(500, 5000);
                }
                if let Some(c) = step.get("CupMl").and_then(Value::as_i64) {
                    water.cup_ml = c.clamp(50, 1000);
                }
                if let Some(rm) = step.get("ReminderMinutes").and_then(Value::as_i64) {
                    water.reminder_minutes = rm.clamp(15, 240);
                }
                if let Some(re) = step.get("RemindersEnabled").and_then(Value::as_bool) {
                    water.reminders_enabled = re;
                }
            }
            _ => {}
        }

        results.push(json!({
            "TodayMl": water.today_ml(),
            "GoalMl": water.goal_ml,
            "CupMl": water.cup_ml,
            "MessageText": water.last_msg.as_ref().map(|m| m.text.as_str()),
            "MessageValue": water.last_msg.as_ref().and_then(|m| m.value.as_deref()),
            "MessageCelebrate": water.last_msg.as_ref().is_some_and(|m| m.celebrate),
        }));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_tracker_adds_undoes_and_celebrates_goal() {
        let res = simulate_water_tracker(Some(vec![
            json!({"Op": "settings", "GoalMl": 500, "CupMl": 250}),
            json!({"Op": "drink"}),
            json!({"Op": "drink"}),
            json!({"Op": "undo"}),
        ]))
        .unwrap();
        assert_eq!(res[1]["TodayMl"], 250);
        assert_eq!(res[2]["TodayMl"], 500);
        assert_eq!(res[2]["MessageCelebrate"], true);
        assert_eq!(res[3]["TodayMl"], 250);
    }
}
