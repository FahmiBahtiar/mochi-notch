//! The Rust port of `gaze/`.

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

#[derive(Debug, Clone)]
pub struct EyeGazeController {
    pub norm_x: f64,
    pub norm_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    target_x: f64,
    target_y: f64,
    last_mouse_x: f64,
    last_mouse_y: f64,
    has_mouse_sample: bool,
    mouse_still_seconds: f64,
    glance_cooldown: f64,
    blink_cooldown: f64,
    blink_elapsed: f64,
    double_blink: bool,
    blink_switched_gaze: bool,
    sleep_breath_phase: f64,
    attentive_cooldown: f64,
    pub is_tracking_cursor: bool,
    rng_state: u64,
}

impl EyeGazeController {
    pub fn new(seed: u64) -> Self {
        Self {
            norm_x: 0.0,
            norm_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            target_x: 0.0,
            target_y: 0.0,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            has_mouse_sample: false,
            mouse_still_seconds: 0.0,
            glance_cooldown: 1.0,
            blink_cooldown: 2.4,
            blink_elapsed: -1.0,
            double_blink: false,
            blink_switched_gaze: false,
            sleep_breath_phase: 0.0,
            attentive_cooldown: 0.0,
            is_tracking_cursor: false,
            rng_state: seed.wrapping_add(1),
        }
    }

    fn next_f64(&mut self) -> f64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng_state >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn next_usize(&mut self, max: usize) -> usize {
        (self.next_f64() * (max as f64)) as usize % max
    }

    pub fn is_blinking(&self) -> bool {
        self.blink_elapsed >= 0.0
    }

    pub fn trigger_blink(&mut self, force_double: bool) {
        self.blink_elapsed = 0.0;
        self.double_blink = force_double || self.next_f64() < 0.28;
        self.blink_switched_gaze = false;
        self.blink_cooldown = 2.5 + self.next_f64() * 3.0;
    }

    pub fn step(
        &mut self,
        dt_seconds: f64,
        sleeping: bool,
        reduced_motion: bool,
        cursor_screen_px: Option<(f64, f64)>,
        cursor_local_offset_dip: Option<(f64, f64)>,
        sprite_y_in_window_dip: f64,
    ) {
        let dt = dt_seconds.clamp(0.001, 0.1);

        if reduced_motion {
            self.norm_x = 0.0;
            self.norm_y = if sleeping { 0.15 } else { 0.0 };
            self.scale_x = 1.0;
            self.scale_y = if sleeping { 0.12 } else { 1.0 };
            self.blink_elapsed = -1.0;
            self.is_tracking_cursor = false;
            return;
        }

        if sleeping {
            self.sleep_breath_phase += dt * 1.6;
            self.target_x = 0.0;
            self.target_y = 0.20;
            let lerp_sleep = 1.0 - (-5.0 * dt).exp();
            self.norm_x += (self.target_x - self.norm_x) * lerp_sleep;
            self.norm_y += (self.target_y - self.norm_y) * lerp_sleep;
            let breath = 0.12 + 0.025 * self.sleep_breath_phase.sin();
            self.scale_y += (breath - self.scale_y) * (1.0 - (-6.0 * dt).exp());
            self.scale_x += (1.0 - self.scale_x) * lerp_sleep;
            self.blink_elapsed = -1.0;
            self.is_tracking_cursor = false;
            return;
        }

        let mut cursor_moved = false;
        if let Some((mx, my)) = cursor_screen_px {
            if !self.has_mouse_sample {
                self.last_mouse_x = mx;
                self.last_mouse_y = my;
                self.has_mouse_sample = true;
                self.mouse_still_seconds = 0.0;
            } else if (mx - self.last_mouse_x).abs() > 2.0 || (my - self.last_mouse_y).abs() > 2.0 {
                self.last_mouse_x = mx;
                self.last_mouse_y = my;
                self.mouse_still_seconds = 0.0;
                cursor_moved = true;
            } else {
                self.mouse_still_seconds += dt;
            }
        } else {
            self.mouse_still_seconds += dt;
        }

        if self.mouse_still_seconds < 2.5
            && let Some((dx, dy)) = cursor_local_offset_dip
        {
            self.is_tracking_cursor = true;
            let dist = (dx * dx + dy * dy).sqrt();
            if cursor_moved && dist < 170.0 {
                self.attentive_cooldown = 0.9;
            }
            let proximity = (-dist / 220.0).exp();
            let ref_y = (260.0 - sprite_y_in_window_dip).max(0.0) * (1.0 - proximity);
            let mut gx = (dx / 300.0).tanh();
            let mut gy = ((dy - ref_y) / 340.0).tanh();
            let mag = (gx * gx + gy * gy).sqrt();
            if mag > 1.0 {
                gx /= mag;
                gy /= mag;
            }
            self.target_x = gx;
            self.target_y = gy;
        } else {
            self.is_tracking_cursor = false;
            self.glance_cooldown -= dt;
            if self.glance_cooldown <= 0.0 {
                self.pick_autonomous_glance();
            }
        }

        let gaze_rate = if self.is_tracking_cursor { 15.0 } else { 12.0 };
        let alpha = 1.0 - (-gaze_rate * dt).exp();
        self.norm_x += (self.target_x - self.norm_x) * alpha;
        self.norm_y += (self.target_y - self.norm_y) * alpha;

        if self.attentive_cooldown > 0.0 {
            self.attentive_cooldown = (self.attentive_cooldown - dt).max(0.0);
        }

        let open_scale_y = if self.attentive_cooldown > 0.0 {
            1.07
        } else {
            1.0
        };
        let target_scale_x = if self.attentive_cooldown > 0.0 {
            1.04
        } else {
            1.0
        };
        self.scale_x += (target_scale_x - self.scale_x) * (1.0 - (-10.0 * dt).exp());

        if self.blink_elapsed < 0.0 {
            self.blink_cooldown -= dt;
            if self.blink_cooldown <= 0.0 {
                self.trigger_blink(false);
            }
        }

        if self.blink_elapsed >= 0.0 {
            self.blink_elapsed += dt;
            let total_blink = if self.double_blink { 0.34 } else { 0.18 };
            if self.blink_elapsed >= total_blink {
                self.blink_elapsed = -1.0;
                self.scale_y = open_scale_y;
            } else {
                self.scale_y =
                    evaluate_blink_curve(self.blink_elapsed, self.double_blink, open_scale_y);
                if !self.is_tracking_cursor && !self.blink_switched_gaze && self.scale_y < 0.12 {
                    self.blink_switched_gaze = true;
                    if self.next_f64() < 0.5 {
                        self.pick_autonomous_glance();
                    }
                }
            }
        } else {
            self.scale_y += (open_scale_y - self.scale_y) * (1.0 - (-14.0 * dt).exp());
        }
    }

    fn pick_autonomous_glance(&mut self) {
        self.glance_cooldown = 1.5 + self.next_f64() * 2.5;
        match self.next_usize(12) {
            0 | 1 => {
                self.target_x = -0.65 - self.next_f64() * 0.25;
                self.target_y = (self.next_f64() - 0.5) * 0.35;
            }
            2 | 3 => {
                self.target_x = 0.65 + self.next_f64() * 0.25;
                self.target_y = (self.next_f64() - 0.5) * 0.35;
            }
            4 => {
                self.target_x = -0.45 - self.next_f64() * 0.25;
                self.target_y = -0.55 - self.next_f64() * 0.2;
            }
            5 => {
                self.target_x = 0.45 + self.next_f64() * 0.25;
                self.target_y = -0.55 - self.next_f64() * 0.2;
            }
            6 => {
                self.target_x = (self.next_f64() - 0.5) * 0.4;
                self.target_y = 0.50 + self.next_f64() * 0.25;
            }
            7 => {
                self.target_x = (self.next_f64() - 0.5) * 0.2;
                self.target_y = -0.65;
            }
            _ => {
                self.target_x = (self.next_f64() - 0.5) * 0.16;
                self.target_y = (self.next_f64() - 0.5) * 0.14;
            }
        }
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn evaluate_blink_curve(t: f64, double_blink: bool, open_scale: f64) -> f64 {
    const CLOSED: f64 = 0.06;
    if !double_blink {
        if t < 0.06 {
            return lerp(open_scale, CLOSED, t / 0.06);
        }
        if t < 0.09 {
            return CLOSED;
        }
        return lerp(CLOSED, open_scale, ((t - 0.09) / 0.09).clamp(0.0, 1.0));
    }

    if t < 0.06 {
        return lerp(open_scale, CLOSED, t / 0.06);
    }
    if t < 0.14 {
        return lerp(CLOSED, 0.72 * open_scale, (t - 0.06) / 0.08);
    }
    if t < 0.21 {
        return lerp(0.72 * open_scale, CLOSED, (t - 0.14) / 0.07);
    }
    if t < 0.24 {
        return CLOSED;
    }
    lerp(CLOSED, open_scale, ((t - 0.24) / 0.10).clamp(0.0, 1.0))
}

fn clean_num(x: f64, default: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        default
    } else {
        x
    }
}

/// Port of `SimulateCursorGaze` (`gaze/GazeUnit.cs:33`).
#[expect(clippy::too_many_arguments, reason = "matches C# facade signature")]
pub fn simulate_cursor_gaze(
    mut dt_seconds: f64,
    sleeping: bool,
    reduced_motion: bool,
    mut cursor_x: f64,
    mut cursor_y: f64,
    mut offset_x: f64,
    mut offset_y: f64,
    mut sprite_y: f64,
    steps: i64,
) -> Result<Vec<Value>, String> {
    dt_seconds = clean_num(dt_seconds, 0.033);
    cursor_x = clean_num(cursor_x, 0.0);
    cursor_y = clean_num(cursor_y, 0.0);
    offset_x = clean_num(offset_x, 0.0);
    offset_y = clean_num(offset_y, 0.0);
    sprite_y = clean_num(sprite_y, 15.0);
    let steps = steps.clamp(1, 60) as usize;

    let mut gaze = EyeGazeController::new(42);
    let mut result = Vec::with_capacity(steps);
    for i in 0..steps {
        let cx = cursor_x + (i as f64) * 3.0;
        let ox = offset_x + (i as f64) * 3.0;
        gaze.step(
            dt_seconds,
            sleeping,
            reduced_motion,
            Some((cx, cursor_y)),
            Some((ox, offset_y)),
            sprite_y,
        );
        result.push(json!({
            "NormX": out_num(gaze.norm_x),
            "NormY": out_num(gaze.norm_y),
            "ScaleX": out_num(gaze.scale_x),
            "ScaleY": out_num(gaze.scale_y),
            "IsBlinking": gaze.is_blinking(),
            "IsTrackingCursor": gaze.is_tracking_cursor,
        }));
    }
    Ok(result)
}

/// Port of `SimulateBlink` (`gaze/GazeUnit.cs:65`).
pub fn simulate_blink(
    double_blink: bool,
    mut dt_seconds: f64,
    steps: i64,
) -> Result<Vec<Value>, String> {
    if dt_seconds.is_nan() || dt_seconds.is_infinite() || dt_seconds <= 0.0 {
        dt_seconds = 0.016;
    }
    dt_seconds = dt_seconds.clamp(0.005, 0.05);
    let steps = steps.clamp(1, 40) as usize;

    let mut gaze = EyeGazeController::new(42);
    gaze.trigger_blink(double_blink);
    let mut result = Vec::with_capacity(steps);
    for i in 0..steps {
        gaze.step(
            dt_seconds,
            false,
            false,
            Some((100.0 + (i as f64) * 3.0, 100.0)),
            Some((0.0, 85.0)),
            15.0,
        );
        result.push(json!({
            "NormX": out_num(gaze.norm_x),
            "NormY": out_num(gaze.norm_y),
            "ScaleX": out_num(gaze.scale_x),
            "ScaleY": out_num(gaze.scale_y),
            "IsBlinking": gaze.is_blinking(),
            "IsTrackingCursor": gaze.is_tracking_cursor,
        }));
    }
    Ok(result)
}

/// Port of `ComputeRoboVisorEyes` (`gaze/GazeUnit.cs:82`).
#[expect(clippy::too_many_arguments, reason = "matches C# facade signature")]
pub fn compute_robo_visor_eyes(
    mut width: f64,
    mut height: f64,
    mut eye_shift_x: f64,
    mut eye_shift_y: f64,
    mut eye_scale_x: f64,
    mut eye_scale_y: f64,
    dim: bool,
    is_sleeping: bool,
) -> Result<Value, String> {
    if width.is_nan() || width.is_infinite() || width <= 0.0 {
        width = 120.0;
    }
    if height.is_nan() || height.is_infinite() || height <= 0.0 {
        height = 30.0;
    }
    eye_shift_x = clean_num(eye_shift_x, 0.0);
    eye_shift_y = clean_num(eye_shift_y, 0.0);
    eye_scale_x = clean_num(eye_scale_x, 1.0);
    eye_scale_y = clean_num(eye_scale_y, 1.0);

    let center_x = width / 2.0;
    let center_y = height / 2.0;
    let norm_x = (eye_shift_x / 14.0).clamp(-1.0, 1.0);
    let left_scale_w = 1.0 - 0.20 * (-norm_x).max(0.0);
    let right_scale_w = 1.0 - 0.20 * norm_x.max(0.0);

    let sx = eye_scale_x.clamp(0.2, 1.5);
    let sy = eye_scale_y.clamp(0.05, 1.4);

    let base_eye_w = 8.0 * sx;
    let base_eye_h = (14.5 * sy).max(1.4);
    let left_w = (base_eye_w * left_scale_w).max(1.2);
    let right_w = (base_eye_w * right_scale_w).max(1.2);

    let eye_spacing = 9.0 * (1.0 - 0.08 * norm_x * norm_x);
    let pos_x = center_x + eye_shift_x;
    let pos_y = center_y + eye_shift_y;

    let left_x = pos_x - left_w - eye_spacing / 2.0;
    let right_x = pos_x + eye_spacing / 2.0;
    let eye_y = pos_y - base_eye_h / 2.0;

    let left_radius = (left_w / 2.0).min(base_eye_h / 2.0);
    let right_radius = (right_w / 2.0).min(base_eye_h / 2.0);
    let halo_out_y = (base_eye_h * 0.35).min(2.0);
    let halo_in_y = (base_eye_h * 0.2).min(1.0);

    Ok(json!({
        "Left": {
            "X": out_num(left_x),
            "Y": out_num(eye_y),
            "Width": out_num(left_w),
            "Height": out_num(base_eye_h),
            "Radius": out_num(left_radius),
            "HaloOut": out_num(halo_out_y),
            "HaloIn": out_num(halo_in_y),
        },
        "Right": {
            "X": out_num(right_x),
            "Y": out_num(eye_y),
            "Width": out_num(right_w),
            "Height": out_num(base_eye_h),
            "Radius": out_num(right_radius),
            "HaloOut": out_num(halo_out_y),
            "HaloIn": out_num(halo_in_y),
        },
        "Opacity": if dim || is_sleeping { 0.55 } else { 1.0 },
    }))
}

/// Port of `ComputeNotchGeometry` (`gaze/GazeUnit.cs:132`).
pub fn compute_notch_geometry(
    mut width: f64,
    mut height: f64,
    mut radius: f64,
) -> Result<Value, String> {
    width = clean_num(width, 120.0).max(24.0);
    height = clean_num(height, 30.0).max(14.0);
    radius = clean_num(radius, 15.0).clamp(0.0, width.min(height) / 2.0);

    let e = 8.0;
    let ear_h = 6.0;
    Ok(json!({
        "TotalWidth": out_num(width + 2.0 * e),
        "Height": out_num(height),
        "Radius": out_num(radius),
        "Ear": e,
        "EarHeight": ear_h,
        "RightEarCp1X": out_num(width + e + 0.55 * e),
        "RightEarCp1Y": 0.0,
        "RightEarCp2X": out_num(width + e),
        "RightEarCp2Y": 0.45 * ear_h,
        "RightEarEndX": out_num(width + e),
        "RightEarEndY": ear_h,
        "BottomRightArcEndX": out_num(width + e - radius),
        "BottomRightArcEndY": out_num(height),
        "BottomLeftLineEndX": out_num(e + radius),
        "BottomLeftLineEndY": out_num(height),
        "BottomLeftArcEndX": e,
        "BottomLeftArcEndY": out_num(height - radius),
        "LeftEarCp1X": e,
        "LeftEarCp1Y": 0.45 * ear_h,
        "LeftEarCp2X": 0.45 * e,
        "LeftEarCp2Y": 0.0,
    }))
}

/// Port of `ComputeSoftShadowSlices` (`gaze/GazeUnit.cs:159`).
pub fn compute_soft_shadow_slices(
    mut width: f64,
    mut height: f64,
    mut blur_radius: f64,
    mut depth: f64,
) -> Result<Vec<Value>, String> {
    if width.is_nan() || width.is_infinite() || width < 0.0 {
        width = 120.0;
    }
    if height.is_nan() || height.is_infinite() || height < 0.0 {
        height = 30.0;
    }
    if blur_radius.is_nan() || blur_radius.is_infinite() || blur_radius < 0.0 {
        blur_radius = 20.0;
    }
    depth = clean_num(depth, 4.0);
    blur_radius = blur_radius.min(100.0);

    const CORNER: f64 = 20.0;
    const MIDDLE: i32 = 2;
    let pad = blur_radius.ceil();
    let corner_int = (pad + CORNER) as i32;
    let corner = pad + CORNER;

    let starts = [0, corner_int, corner_int + MIDDLE];
    let lengths = [corner_int, MIDDLE, corner_int];

    let outer_left = -pad;
    let outer_top = -pad + depth;
    let outer_width = width + 2.0 * pad;
    let outer_height = height + 2.0 * pad;
    let cx = corner.min(outer_width / 2.0);
    let cy = corner.min(outer_height / 2.0);
    let xs = [
        outer_left,
        outer_left + cx,
        outer_left + outer_width - cx,
        outer_left + outer_width,
    ];
    let ys = [
        outer_top,
        outer_top + cy,
        outer_top + outer_height - cy,
        outer_top + outer_height,
    ];

    let mut result = Vec::new();
    for row in 0..3 {
        for col in 0..3 {
            if row == 1 && col == 1 {
                continue;
            }
            let w = xs[col + 1] - xs[col];
            let h = ys[row + 1] - ys[row];
            if w > 0.0 && h > 0.0 {
                result.push(json!({
                    "Row": row,
                    "Col": col,
                    "SrcX": starts[col] as f64,
                    "SrcY": starts[row] as f64,
                    "SrcW": lengths[col] as f64,
                    "SrcH": lengths[row] as f64,
                    "DstX": out_num(xs[col]),
                    "DstY": out_num(ys[row]),
                    "DstW": out_num(w),
                    "DstH": out_num(h),
                }));
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_gaze_tracks_and_clamps() {
        let steps = simulate_cursor_gaze(0.033, false, false, 500.0, 500.0, 400.0, 200.0, 15.0, 10)
            .unwrap();
        assert_eq!(steps.len(), 10);
        assert_eq!(steps[0]["IsTrackingCursor"], true);
    }

    #[test]
    fn shadow_slices_for_closed_notch() {
        let slices = compute_soft_shadow_slices(120.0, 30.0, 20.0, 4.0).unwrap();
        assert_eq!(slices.len(), 6);
    }
}
