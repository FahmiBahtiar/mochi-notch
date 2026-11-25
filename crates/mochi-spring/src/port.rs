//! The Rust port of `spring/`.

#![forbid(unsafe_code)]

use serde_json::{Value, json};

fn out_num(x: f64) -> Value {
    if x.is_nan() {
        json!({"$num": "NaN"})
    } else if x.is_infinite() {
        json!({"$num": if x > 0.0 { "Infinity" } else { "-Infinity" }})
    } else if x == 0.0 && x.is_sign_negative() {
        json!({"$num": "-0"})
    } else {
        json!(x)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    pub stiffness: f64,
    pub damping_ratio: f64,
    epsilon: f64,
}

impl Spring {
    pub fn new(value: f64, epsilon: f64) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness: 380.0,
            damping_ratio: 0.72,
            epsilon,
        }
    }

    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    pub fn step(&mut self, mut seconds: f64) -> bool {
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

/// Port of `StepSpring` (`spring/SpringUnit.cs:23`).
pub fn step_spring(
    mut value: f64,
    mut velocity: f64,
    mut target: f64,
    mut stiffness: f64,
    mut damping_ratio: f64,
    mut epsilon: f64,
    mut seconds: f64,
) -> Result<Value, String> {
    if value.is_nan() || value.is_infinite() {
        value = 0.0;
    }
    if velocity.is_nan() || velocity.is_infinite() {
        velocity = 0.0;
    }
    if target.is_nan() || target.is_infinite() {
        target = 0.0;
    }
    if stiffness.is_nan() || stiffness.is_infinite() || stiffness <= 0.0 {
        stiffness = 380.0;
    }
    if damping_ratio.is_nan() || damping_ratio.is_infinite() || damping_ratio < 0.0 {
        damping_ratio = 0.72;
    }
    if epsilon.is_nan() || epsilon.is_infinite() || epsilon <= 0.0 {
        epsilon = 0.002;
    }
    if seconds.is_nan() || seconds.is_infinite() || seconds < 0.0 {
        seconds = 0.0;
    }
    seconds = seconds.min(1.0);

    let mut s = Spring::new(value, epsilon);
    s.velocity = velocity;
    s.target = target;
    s.stiffness = stiffness;
    s.damping_ratio = damping_ratio;
    let moving = s.step(seconds);

    Ok(json!({
        "Value": out_num(s.value),
        "Velocity": out_num(s.velocity),
        "Moving": moving,
    }))
}

/// Port of `SimulateSpringTrajectory` (`spring/SpringUnit.cs:52`).
pub fn simulate_spring_trajectory(
    mut initial: f64,
    mut target: f64,
    mut stiffness: f64,
    mut damping_ratio: f64,
    mut epsilon: f64,
    mut dt: f64,
    max_steps: i64,
) -> Result<Vec<f64>, String> {
    if initial.is_nan() || initial.is_infinite() {
        initial = 0.0;
    }
    if target.is_nan() || target.is_infinite() {
        target = 0.0;
    }
    if stiffness.is_nan() || stiffness.is_infinite() || stiffness <= 0.0 {
        stiffness = 380.0;
    }
    if damping_ratio.is_nan() || damping_ratio.is_infinite() || damping_ratio < 0.0 {
        damping_ratio = 0.72;
    }
    if epsilon.is_nan() || epsilon.is_infinite() || epsilon <= 0.0 {
        epsilon = 0.002;
    }
    if dt.is_nan() || dt.is_infinite() || dt <= 0.0 {
        dt = 1.0 / 100.0;
    }
    dt = dt.clamp(0.001, 0.05);
    let steps = max_steps.clamp(1, 300) as usize;

    let mut s = Spring::new(initial, epsilon);
    s.target = target;
    s.stiffness = stiffness;
    s.damping_ratio = damping_ratio;

    let mut values = Vec::with_capacity(steps);
    for _ in 0..steps {
        let moving = s.step(dt);
        values.push(s.value);
        if !moving {
            break;
        }
    }
    Ok(values)
}

/// Port of `ShapeMotion` (`spring/SpringUnit.cs:87`).
pub fn shape_motion(expanded: bool, calm_motion: bool) -> Result<Value, String> {
    let (stiffness, damping) = match (expanded, calm_motion) {
        (true, false) => (400.0, 1.0),
        (true, true) => (420.0, 1.0),
        (false, false) => (520.0, 1.0),
        (false, true) => (600.0, 1.0),
    };
    Ok(json!({
        "Stiffness": stiffness,
        "DampingRatio": damping,
    }))
}

/// Port of `ComputeFrameOpacities` (`spring/SpringUnit.cs:99`).
pub fn compute_frame_opacities(
    expanded: bool,
    mut expand_progress: f64,
    mut hud_value: f64,
    mut ambient_value: f64,
    overlay_is_news: bool,
) -> Result<Value, String> {
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

    Ok(json!({
        "CompactOpacity": out_num(compact_opacity),
        "ExpandedOpacity": out_num(expanded_opacity),
        "CompactViewOpacity": out_num(under_overlay * (1.0 - ambient)),
        "AmbientViewOpacity": out_num(under_overlay * ambient),
        "PrivacyDotsOpacity": out_num(compact_opacity * if overlay_is_news { 1.0 - overlay } else { 1.0 }),
        "HudViewOpacity": out_num(if overlay_is_news { 0.0 } else { compact_opacity * overlay }),
        "NewsViewOpacity": out_num(if overlay_is_news { compact_opacity * overlay } else { 0.0 }),
        "ExpandedHitTest": expand_progress > 0.7,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_at_100hz() {
        let traj = simulate_spring_trajectory(120.0, 520.0, 400.0, 1.0, 0.1, 0.01, 300).unwrap();
        assert!(!traj.is_empty());
        assert!((*traj.last().unwrap() - 520.0).abs() < 0.1);
    }

    #[test]
    fn opacities_crossfade_thresholds() {
        let open_mid = compute_frame_opacities(true, 0.5, 0.0, 0.0, false).unwrap();
        assert_eq!(open_mid["CompactOpacity"], 0.0);
        assert!(open_mid["ExpandedOpacity"].as_f64().unwrap() > 0.3);
    }
}
