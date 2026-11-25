//! Pure idle presence tracking and away summary models.

#![forbid(unsafe_code)]

use std::time::Duration;

use chrono::NaiveDateTime;

#[derive(Debug, Clone)]
pub struct AwayItem {
    pub glyph: String,
    pub text: String,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct AwayReport {
    pub duration: Duration,
    pub items: Vec<AwayItem>,
}

pub fn presence_next_check_seconds(idle_seconds: f64) -> Result<f64, String> {
    if idle_seconds.is_nan() || idle_seconds.is_infinite() {
        return Ok(1.0);
    }
    let clamped = idle_seconds.clamp(-86400.0, 86400.0);
    let idle_ticks = (clamped * 10_000_000.0).round() as i64;
    let unseen_ticks = 60 * 10_000_000;
    let away_ticks = 10_000_000;
    let next_ticks = if idle_ticks >= unseen_ticks {
        away_ticks
    } else {
        unseen_ticks - idle_ticks + 500_000
    };
    Ok((next_ticks as f64) / 10_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_next_presence_check_interval() {
        assert!((presence_next_check_seconds(10.0).unwrap() - 50.05).abs() < 0.001);
        assert!((presence_next_check_seconds(65.0).unwrap() - 1.0).abs() < 0.001);
    }
}
