//! Pure damped spring physics and frame opacity cross-fade math.

#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    pub stiffness: f64,
    pub damping_ratio: f64,
    pub epsilon: f64,
}

impl Spring {
    pub fn new(value: f64, epsilon: f64) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness: 400.0,
            damping_ratio: 1.0,
            epsilon,
        }
    }

    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    pub fn step(&mut self, mut seconds: f64) -> bool {
        if !self.value.is_finite() {
            self.value = 0.0;
        }
        if !self.velocity.is_finite() {
            self.velocity = 0.0;
        }
        if !self.target.is_finite() {
            self.target = 0.0;
        }
        if !self.stiffness.is_finite() || self.stiffness <= 0.0 {
            self.stiffness = 400.0;
        }
        if !self.damping_ratio.is_finite() || self.damping_ratio < 0.0 {
            self.damping_ratio = 1.0;
        }
        if !self.epsilon.is_finite() || self.epsilon <= 0.0 {
            self.epsilon = 0.002;
        }
        if !seconds.is_finite() || seconds < 0.0 {
            seconds = 0.0;
        }
        seconds = seconds.min(1.0);

        let damping = 2.0 * self.stiffness.sqrt() * self.damping_ratio;
        while seconds > 0.0 {
            let dt = seconds.min(1.0 / 240.0);
            let acceleration =
                self.stiffness * (self.target - self.value) - damping * self.velocity;
            self.velocity += acceleration * dt;
            self.value += self.velocity * dt;
            seconds -= dt;
        }

        if (self.target - self.value).abs() < self.epsilon
            && self.velocity.abs() < self.epsilon * 10.0
        {
            self.value = self.target;
            self.velocity = 0.0;
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionParams {
    pub stiffness: f64,
    pub damping_ratio: f64,
}

pub fn shape_motion(expanded: bool, calm_motion: bool) -> MotionParams {
    let (stiffness, damping_ratio) = match (expanded, calm_motion) {
        (true, false) => (400.0, 1.0),
        (true, true) => (420.0, 1.0),
        (false, false) => (520.0, 1.0),
        (false, true) => (600.0, 1.0),
    };
    MotionParams {
        stiffness,
        damping_ratio,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameOpacities {
    pub compact_opacity: f64,
    pub expanded_opacity: f64,
    pub compact_view_opacity: f64,
    pub ambient_view_opacity: f64,
    pub privacy_dots_opacity: f64,
    pub hud_view_opacity: f64,
    pub news_view_opacity: f64,
    pub expanded_hit_test: bool,
}

pub fn compute_frame_opacities(
    expanded: bool,
    mut expand_progress: f64,
    mut hud_value: f64,
    mut ambient_value: f64,
    overlay_is_news: bool,
) -> FrameOpacities {
    if expand_progress.is_nan() {
        expand_progress = 0.0;
    }
    if hud_value.is_nan() {
        hud_value = 0.0;
    }
    if ambient_value.is_nan() {
        ambient_value = 0.0;
    }

    let (compact_opacity, expanded_opacity) = if expanded {
        (
            (1.0 - expand_progress / 0.28).clamp(0.0, 1.0),
            ((expand_progress - 0.32) / 0.58).clamp(0.0, 1.0),
        )
    } else {
        (
            (1.0 - expand_progress / 0.35).clamp(0.0, 1.0),
            ((expand_progress - 0.45) / 0.45).clamp(0.0, 1.0),
        )
    };

    let overlay = hud_value.clamp(0.0, 1.0);
    let ambient = ambient_value.clamp(0.0, 1.0);
    let under_overlay = compact_opacity * (1.0 - overlay);

    FrameOpacities {
        compact_opacity,
        expanded_opacity,
        compact_view_opacity: under_overlay * (1.0 - ambient),
        ambient_view_opacity: under_overlay * ambient,
        privacy_dots_opacity: compact_opacity * if overlay_is_news { 1.0 - overlay } else { 1.0 },
        hud_view_opacity: if overlay_is_news {
            0.0
        } else {
            compact_opacity * overlay
        },
        news_view_opacity: if overlay_is_news {
            compact_opacity * overlay
        } else {
            0.0
        },
        expanded_hit_test: expand_progress > 0.7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_at_100hz() {
        let mut s = Spring::new(120.0, 0.1);
        s.target = 520.0;
        s.stiffness = 400.0;
        s.damping_ratio = 1.0;
        for _ in 0..300 {
            if !s.step(0.01) {
                break;
            }
        }
        assert!((s.value - 520.0).abs() < 0.1);
    }

    #[test]
    fn opacities_crossfade_thresholds() {
        let open_mid = compute_frame_opacities(true, 0.5, 0.0, 0.0, false);
        assert_eq!(open_mid.compact_opacity, 0.0);
        assert!(open_mid.expanded_opacity > 0.3);
    }
}
