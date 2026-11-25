//! Pure Pomodoro focus timer state machine and duration formatting.

#![forbid(unsafe_code)]

use serde_json::{Value, json};

const TICKS_PER_SECOND: i64 = 10_000_000;
const TICKS_PER_MINUTE: i64 = 60 * TICKS_PER_SECOND;
const SLEEP_GAP_TICKS: i64 = 30 * TICKS_PER_SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPhase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl FocusPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Focus => "Focus",
            Self::ShortBreak => "ShortBreak",
            Self::LongBreak => "LongBreak",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusState {
    Ready,
    Running,
    Paused,
}

impl FocusState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Paused => "Paused",
        }
    }
}

#[derive(Debug, Clone)]
struct HudMsg {
    text: String,
    value: Option<String>,
    celebrate: bool,
    chime: bool,
}

struct FocusTimer {
    focus_minutes: i64,
    short_break_minutes: i64,
    long_break_minutes: i64,
    sessions_per_cycle: i64,
    phase: FocusPhase,
    state: FocusState,
    completed_sessions: i64,
    paused_remaining_ticks: i64,
    run_from_ticks: i64,
    ends_at_ticks: i64,
    last_tick_ticks: i64,
    now_ticks: i64,
    focused_reported_ticks: i64,
    last_msg: Option<HudMsg>,
}

impl FocusTimer {
    fn new() -> Self {
        let mut t = Self {
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            sessions_per_cycle: 4,
            phase: FocusPhase::Focus,
            state: FocusState::Ready,
            completed_sessions: 0,
            paused_remaining_ticks: 0,
            run_from_ticks: 0,
            ends_at_ticks: 0,
            last_tick_ticks: 0,
            now_ticks: 0,
            focused_reported_ticks: 0,
            last_msg: None,
        };
        t.paused_remaining_ticks = t.phase_length_ticks();
        t
    }

    fn sessions_per_cycle(&self) -> i64 {
        self.sessions_per_cycle.max(1)
    }

    fn phase_length_ticks(&self) -> i64 {
        let mins = match self.phase {
            FocusPhase::ShortBreak => self.short_break_minutes,
            FocusPhase::LongBreak => self.long_break_minutes,
            FocusPhase::Focus => self.focus_minutes,
        };
        mins.max(1) * TICKS_PER_MINUTE
    }

    fn remaining_ticks(&self) -> i64 {
        if self.state != FocusState::Running {
            return self.paused_remaining_ticks;
        }
        (self.ends_at_ticks - self.now_ticks).max(0)
    }

    fn progress(&self) -> f64 {
        let rem = (self.remaining_ticks() as f64) / (TICKS_PER_SECOND as f64);
        let len = (self.phase_length_ticks() as f64) / (TICKS_PER_SECOND as f64);
        1.0 - rem / len
    }

    fn ran_ticks(&self, remaining: i64) -> i64 {
        if remaining < self.run_from_ticks {
            self.run_from_ticks - remaining
        } else {
            0
        }
    }

    fn current_run_ticks(&self) -> i64 {
        if self.state == FocusState::Running && self.phase == FocusPhase::Focus {
            self.ran_ticks(self.remaining_ticks())
        } else {
            0
        }
    }

    fn apply_settings(&mut self) {
        let len = self.phase_length_ticks();
        if self.state == FocusState::Ready || self.paused_remaining_ticks > len {
            self.paused_remaining_ticks = len;
        }
        let spc = self.sessions_per_cycle();
        if self.completed_sessions > spc {
            self.completed_sessions = spc;
        }
    }

    fn start(&mut self) {
        if self.state == FocusState::Ready {
            self.run();
        }
    }

    fn pause(&mut self) {
        if self.state == FocusState::Running {
            self.freeze(self.remaining_ticks());
        }
    }

    fn resume(&mut self) {
        if self.state == FocusState::Paused {
            self.run();
        }
    }

    fn stop(&mut self) {
        if self.state == FocusState::Running {
            self.end_run(self.remaining_ticks());
        }
        self.phase = FocusPhase::Focus;
        self.state = FocusState::Ready;
        self.paused_remaining_ticks = self.phase_length_ticks();
    }

    fn skip_break(&mut self) {
        if self.phase != FocusPhase::Focus {
            self.advance(false);
        }
    }

    fn pause_after_idle(&mut self, unattended_ticks: i64) {
        if self.state != FocusState::Running {
            return;
        }
        let remaining = self.remaining_ticks() + unattended_ticks;
        let len = self.phase_length_ticks();
        self.freeze(remaining.min(len));
    }

    fn tick(&mut self) {
        let now = self.now_ticks;
        let last = self.last_tick_ticks;
        self.last_tick_ticks = now;
        if self.state == FocusState::Running && now - last > SLEEP_GAP_TICKS {
            self.slept(last, now);
        } else {
            self.poll();
        }
    }

    fn slept(&mut self, from: i64, to: i64) {
        let left = self.ends_at_ticks - from;
        if self.phase == FocusPhase::Focus && left > 0 {
            let len = self.phase_length_ticks();
            self.freeze(left.min(len));
            let mins = ((left as f64) / (TICKS_PER_MINUTE as f64)).ceil() as i64;
            self.last_msg = Some(HudMsg {
                text: "Paused while asleep".to_string(),
                value: Some(format!("{mins} min left")),
                celebrate: false,
                chime: false,
            });
        } else if self.phase != FocusPhase::Focus && to >= self.ends_at_ticks {
            self.advance(false);
        } else {
            self.poll();
        }
    }

    fn poll(&mut self) {
        if self.state != FocusState::Running {
            return;
        }
        if self.remaining_ticks() <= 0 {
            self.advance(true);
        }
    }

    fn run(&mut self) {
        self.run_from_ticks = self.paused_remaining_ticks;
        self.last_tick_ticks = self.now_ticks;
        self.ends_at_ticks = self.last_tick_ticks + self.paused_remaining_ticks;
        self.state = FocusState::Running;
    }

    fn freeze(&mut self, remaining: i64) {
        if self.state == FocusState::Running {
            self.end_run(remaining);
        }
        self.paused_remaining_ticks = remaining;
        self.state = FocusState::Paused;
    }

    fn advance(&mut self, notify: bool) {
        if self.phase == FocusPhase::Focus {
            if self.state == FocusState::Running {
                self.end_run(0);
            }
            self.completed_sessions += 1;
            self.phase = if self.completed_sessions >= self.sessions_per_cycle() {
                FocusPhase::LongBreak
            } else {
                FocusPhase::ShortBreak
            };
            self.paused_remaining_ticks = self.phase_length_ticks();
            if notify {
                let mins = self.phase_length_ticks() / TICKS_PER_MINUTE;
                self.last_msg = Some(HudMsg {
                    text: "Break time".to_string(),
                    value: Some(format!("{mins} min")),
                    celebrate: true,
                    chime: true,
                });
            }
            self.run();
        } else {
            if self.phase == FocusPhase::LongBreak {
                self.completed_sessions = 0;
            }
            self.phase = FocusPhase::Focus;
            self.state = FocusState::Ready;
            self.paused_remaining_ticks = self.phase_length_ticks();
            if notify {
                let mins = self.phase_length_ticks() / TICKS_PER_MINUTE;
                self.last_msg = Some(HudMsg {
                    text: "Back to focus".to_string(),
                    value: Some(format!("{mins} min")),
                    celebrate: false,
                    chime: true,
                });
            }
        }
    }

    fn end_run(&mut self, remaining: i64) {
        if self.phase == FocusPhase::Focus {
            let ran = self.ran_ticks(remaining);
            if ran > 0 {
                self.focused_reported_ticks += ran;
            }
        }
        self.run_from_ticks = remaining;
    }
}

fn seconds_to_ticks_clamped(secs: f64) -> i64 {
    (secs * (TICKS_PER_SECOND as f64)).round() as i64
}

pub fn simulate_focus_timer(steps: Option<Vec<Value>>) -> Result<Vec<Value>, String> {
    let Some(steps) = steps else {
        return Ok(Vec::new());
    };
    let mut timer = FocusTimer::new();
    let mut results = Vec::with_capacity(steps.len());

    for step in steps {
        timer.last_msg = None;
        timer.focused_reported_ticks = 0;
        if let Some(adv) = step.get("AdvanceSeconds").and_then(Value::as_f64)
            && adv > 0.0
            && !adv.is_nan()
            && !adv.is_infinite()
        {
            timer.now_ticks += seconds_to_ticks_clamped(adv.min(86400.0 * 30.0));
        }

        let op = step.get("Op").and_then(Value::as_str).unwrap_or("");
        match op {
            "start" => timer.start(),
            "pause" => timer.pause(),
            "resume" => timer.resume(),
            "stop" => timer.stop(),
            "skip_break" => timer.skip_break(),
            "poll" => timer.poll(),
            "tick" => timer.tick(),
            "idle" => {
                let unatt = step
                    .get("UnattendedSeconds")
                    .and_then(Value::as_f64)
                    .filter(|v| !v.is_nan() && !v.is_infinite())
                    .map(|v| v.clamp(0.0, 86400.0))
                    .unwrap_or(0.0);
                timer.pause_after_idle(seconds_to_ticks_clamped(unatt));
            }
            "settings" => {
                if let Some(fm) = step.get("FocusMinutes").and_then(Value::as_i64) {
                    timer.focus_minutes = fm.clamp(5, 120);
                }
                if let Some(sb) = step.get("ShortBreakMinutes").and_then(Value::as_i64) {
                    timer.short_break_minutes = sb.clamp(1, 30);
                }
                if let Some(lb) = step.get("LongBreakMinutes").and_then(Value::as_i64) {
                    timer.long_break_minutes = lb.clamp(5, 60);
                }
                if let Some(spc) = step.get("SessionsPerCycle").and_then(Value::as_i64) {
                    timer.sessions_per_cycle = spc.clamp(2, 8);
                }
                timer.apply_settings();
            }
            _ => {}
        }

        results.push(json!({
            "Phase": timer.phase.as_str(),
            "State": timer.state.as_str(),
            "CompletedSessions": timer.completed_sessions,
            "RemainingSeconds": (timer.remaining_ticks() as f64) / (TICKS_PER_SECOND as f64),
            "Progress": timer.progress(),
            "CurrentRunSeconds": (timer.current_run_ticks() as f64) / (TICKS_PER_SECOND as f64),
            "FocusedReportedSeconds": (timer.focused_reported_ticks as f64) / (TICKS_PER_SECOND as f64),
            "MessageText": timer.last_msg.as_ref().map(|m| m.text.as_str()),
            "MessageValue": timer.last_msg.as_ref().and_then(|m| m.value.as_deref()),
            "MessageCelebrate": timer.last_msg.as_ref().is_some_and(|m| m.celebrate),
            "MessageChime": timer.last_msg.as_ref().is_some_and(|m| m.chime),
        }));
    }
    Ok(results)
}

pub fn focus_until_next_second(remaining_seconds: f64) -> Result<f64, String> {
    if remaining_seconds.is_nan() || remaining_seconds.is_infinite() {
        return Ok(0.005);
    }
    let clamped = remaining_seconds.clamp(-86400.0, 86400.0);
    let ticks = seconds_to_ticks_clamped(clamped);
    let rem_ticks = (ticks % TICKS_PER_SECOND) + 50_000;
    Ok((rem_ticks as f64) / (TICKS_PER_SECOND as f64))
}

pub fn format_focus_duration(minutes: i64) -> Result<String, String> {
    let m = minutes.clamp(0, 100_000);
    if m < 60 {
        Ok(format!("{m} min"))
    } else if m % 60 == 0 {
        Ok(format!("{}h", m / 60))
    } else {
        Ok(format!("{}h {:02}", m / 60, m % 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_matches_examples() {
        assert_eq!(format_focus_duration(25).unwrap(), "25 min");
        assert_eq!(format_focus_duration(100).unwrap(), "1h 40");
        assert_eq!(format_focus_duration(120).unwrap(), "2h");
    }

    #[test]
    fn focus_timer_transitions_to_break_after_completion() {
        let res = simulate_focus_timer(Some(vec![
            json!({"Op": "settings", "FocusMinutes": 5}),
            json!({"Op": "start"}),
            json!({"Op": "poll", "AdvanceSeconds": 300.0}),
        ]))
        .unwrap();
        assert_eq!(res[2]["Phase"], "ShortBreak");
        assert_eq!(res[2]["State"], "Running");
        assert_eq!(res[2]["CompletedSessions"], 1);
    }
}
