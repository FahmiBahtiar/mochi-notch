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
