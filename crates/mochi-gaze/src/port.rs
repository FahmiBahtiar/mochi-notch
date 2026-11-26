//! Pure companion eye gaze, blink curves, and visor eye geometry.

#![forbid(unsafe_code)]

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

    pub fn is_animating(&self, sleeping: bool, reduced_motion: bool) -> bool {
        if reduced_motion {
            return false;
        }
        if sleeping {
            return true;
        }
        self.blink_elapsed >= 0.0
            || self.attentive_cooldown > 0.0
            || self.is_tracking_cursor
            || (self.norm_x - self.target_x).abs() > 0.002
            || (self.norm_y - self.target_y).abs() > 0.002
            || (self.scale_x - 1.0).abs() > 0.002
            || (self.scale_y - 1.0).abs() > 0.002
    }

    pub fn advance_idle(&mut self, dt_seconds: f64, cursor_screen_px: Option<(f64, f64)>) -> bool {
        let dt = dt_seconds.clamp(0.0, 1.0);
        if let Some((mx, my)) = cursor_screen_px {
            if !self.has_mouse_sample {
                self.last_mouse_x = mx;
                self.last_mouse_y = my;
                self.has_mouse_sample = true;
                self.mouse_still_seconds = 0.0;
                self.is_tracking_cursor = true;
                return true;
            }
            if (mx - self.last_mouse_x).abs() > 2.0 || (my - self.last_mouse_y).abs() > 2.0 {
                self.last_mouse_x = mx;
                self.last_mouse_y = my;
                self.mouse_still_seconds = 0.0;
                self.is_tracking_cursor = true;
                return true;
            }
        }
        self.mouse_still_seconds += dt;
        self.glance_cooldown -= dt;
        self.blink_cooldown -= dt;
        let mut woke = false;
        if self.glance_cooldown <= 0.0 {
            self.pick_autonomous_glance();
            woke = true;
        }
        if self.blink_cooldown <= 0.0 {
            self.trigger_blink(false);
            woke = true;
        }
        woke
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
            if cursor_local_offset_dip.is_none() && self.is_tracking_cursor {
                self.target_x = 0.0;
                self.target_y = 0.0;
            }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    pub halo_out: f64,
    pub halo_in: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoboVisorEyes {
    pub left: EyeRect,
    pub right: EyeRect,
    pub opacity: f64,
}

#[expect(clippy::too_many_arguments, reason = "visor eye layout parameters")]
pub fn compute_robo_visor_eyes(
    mut width: f64,
    mut height: f64,
    mut eye_shift_x: f64,
    mut eye_shift_y: f64,
    mut eye_scale_x: f64,
    mut eye_scale_y: f64,
    dim: bool,
    is_sleeping: bool,
) -> RoboVisorEyes {
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

    RoboVisorEyes {
        left: EyeRect {
            x: left_x,
            y: eye_y,
            width: left_w,
            height: base_eye_h,
            radius: left_radius,
            halo_out: halo_out_y,
            halo_in: halo_in_y,
        },
        right: EyeRect {
            x: right_x,
            y: eye_y,
            width: right_w,
            height: base_eye_h,
            radius: right_radius,
            halo_out: halo_out_y,
            halo_in: halo_in_y,
        },
        opacity: if dim || is_sleeping { 0.55 } else { 1.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_gaze_tracks_and_clamps() {
        let mut gaze = EyeGazeController::new(42);
        gaze.step(
            0.033,
            false,
            false,
            Some((500.0, 500.0)),
            Some((400.0, 200.0)),
            15.0,
        );
        assert!(gaze.is_tracking_cursor);
        assert!(gaze.norm_x.abs() <= 1.0);
        assert!(gaze.norm_y.abs() <= 1.0);
    }

    #[test]
    fn visor_eyes_dim_when_sleeping() {
        let eyes = compute_robo_visor_eyes(120.0, 30.0, 0.0, 0.0, 1.0, 1.0, false, true);
        assert_eq!(eyes.opacity, 0.55);
        assert!(eyes.left.x < eyes.right.x);
    }

    #[test]
    fn gaze_settles_when_still_and_wakes_on_cursor_or_blink() {
        let mut gaze = EyeGazeController::new(7);
        let mut settled = false;
        for _ in 0..200 {
            gaze.step(
                0.033,
                false,
                false,
                Some((100.0, 100.0)),
                Some((0.0, 0.0)),
                15.0,
            );
            if !gaze.is_animating(false, false) {
                settled = true;
                break;
            }
        }
        assert!(settled, "gaze should settle when cursor stays still");

        // Still cursor does not wake until cooldown expires
        assert!(!gaze.advance_idle(0.05, Some((100.0, 100.0))));
        assert!(!gaze.is_animating(false, false));

        // Moving cursor wakes immediately
        assert!(gaze.advance_idle(0.05, Some((140.0, 100.0))));
        assert!(gaze.is_animating(false, false));

        // Settle again, then advance until blink/glance wakes it
        let mut settled_again = false;
        for _ in 0..200 {
            gaze.step(
                0.033,
                false,
                false,
                Some((140.0, 100.0)),
                Some((0.0, 0.0)),
                15.0,
            );
            if !gaze.is_animating(false, false) {
                settled_again = true;
                break;
            }
        }
        assert!(settled_again);
        let mut woke = false;
        for _ in 0..60 {
            if gaze.advance_idle(0.1, Some((140.0, 100.0))) {
                woke = true;
                break;
            }
        }
        assert!(woke);
    }
}
