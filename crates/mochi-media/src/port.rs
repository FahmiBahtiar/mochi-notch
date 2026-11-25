//! Pure media session selection, player choice, and artwork accent extraction.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionCandidate {
    pub is_playing: bool,
    pub is_shown: bool,
    pub is_system_current: bool,
    pub is_untitled: bool,
}

pub fn pick_media_session(sessions: &[SessionCandidate]) -> Option<usize> {
    if sessions.is_empty() {
        return None;
    }
    let any_titled = sessions.iter().any(|s| !s.is_untitled);
    let find = |pred: fn(&SessionCandidate) -> bool| -> Option<usize> {
        sessions
            .iter()
            .enumerate()
            .find(|(_, s)| (!any_titled || !s.is_untitled) && pred(s))
            .map(|(i, _)| i)
    };

    let rules: [fn(&SessionCandidate) -> bool; 6] = [
        |s| s.is_shown && s.is_playing,
        |s| s.is_system_current && s.is_playing,
        |s| s.is_playing,
        |s| s.is_shown,
        |s| s.is_system_current,
        |_| true,
    ];

    for rule in rules {
        if let Some(idx) = find(rule) {
            return Some(idx);
        }
    }
    None
}

#[derive(Default)]
pub struct PlayerChoice {
    playing: HashMap<String, bool>,
    removed: HashSet<String>,
    pub chosen: Option<String>,
}

impl PlayerChoice {
    pub fn is_removed(&self, id: &str) -> bool {
        self.removed.contains(id)
    }

    pub fn seen(&mut self, id: &str, playing: bool) -> bool {
        let started = playing && self.playing.get(id).copied() == Some(false);
        self.playing.insert(id.to_string(), playing);
        if !started {
            return false;
        }
        let mut changed = self.removed.remove(id);
        if self.chosen.as_deref().is_some_and(|c| c != id) {
            self.chosen = None;
            changed = true;
        }
        changed
    }

    pub fn choose(&mut self, id: &str) {
        self.chosen = Some(id.to_string());
        self.removed.remove(id);
    }

    pub fn remove(&mut self, id: &str) {
        self.removed.insert(id.to_string());
        if self.chosen.as_deref() == Some(id) {
            self.chosen = None;
        }
    }

    pub fn keep(&mut self, open: &HashSet<String>) {
        self.playing.retain(|k, _| open.contains(k));
        self.removed.retain(|k| open.contains(k));
        if self.chosen.as_deref().is_some_and(|c| !open.contains(c)) {
            self.chosen = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const DEFAULT_ACCENT: Self = Self {
        r: 0x8A,
        g: 0xB4,
        b: 0xFF,
    };
}

fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn channel(p: f64, q: f64, mut t: f64) -> u8 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    let v = if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    };
    (v.clamp(0.0, 1.0) * 255.0).round_ties_even() as u8
}

fn from_hsl(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        channel(p, q, h + 1.0 / 3.0),
        channel(p, q, h),
        channel(p, q, h - 1.0 / 3.0),
    )
}

pub fn extract_accent_color(bgra_pixels: &[u8]) -> RgbColor {
    if bgra_pixels.len() < 4 {
        return RgbColor::DEFAULT_ACCENT;
    }
    let count = bgra_pixels.len() - (bgra_pixels.len() % 4);
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let mut total = 0.0;
    for i in (0..count).step_by(4) {
        let pb = f64::from(bgra_pixels[i]) / 255.0;
        let pg = f64::from(bgra_pixels[i + 1]) / 255.0;
        let pr = f64::from(bgra_pixels[i + 2]) / 255.0;
        let (_, s, l) = to_hsl(pr, pg, pb);
        let weight = 0.02 + s * (1.0 - (l - 0.5).abs() * 2.0);
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        total += weight;
    }
    if total <= 0.0 {
        return RgbColor::DEFAULT_ACCENT;
    }
    let (hue, mut saturation, mut lightness) = to_hsl(r / total, g / total, b / total);
    if saturation < 0.08 {
        return RgbColor::DEFAULT_ACCENT;
    }
    saturation = saturation.max(0.45);
    lightness = lightness.clamp(0.62, 0.74);
    let (rr, gg, bb) = from_hsl(hue, saturation, lightness);
    RgbColor {
        r: rr,
        g: gg,
        b: bb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_prefers_shown_playing() {
        let res = pick_media_session(&[
            SessionCandidate {
                is_playing: true,
                is_shown: true,
                is_system_current: false,
                is_untitled: false,
            },
            SessionCandidate {
                is_playing: true,
                is_shown: false,
                is_system_current: true,
                is_untitled: false,
            },
        ]);
        assert_eq!(res, Some(0));
    }
}
