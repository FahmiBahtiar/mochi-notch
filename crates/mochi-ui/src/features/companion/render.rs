use serde_json::Value;

use mochi_winrt::mochi_gaze;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::state::RavenAnimator;
use crate::state::NotchApp;
use crate::theme::{NOTCH_BLACK, PURE_EYE_AMBER, PURE_EYE_CORAL, PURE_EYE_WHITE, TEXT_PRIMARY};

pub(crate) fn draw_robo_visor(
    app: &NotchApp,
    _factories: &D2dFactories,
    surf: &mut D2dSurface,
    notch_x: f32,
    w: f32,
    h: f32,
) {
    let shift_x = app.visor_gaze.norm_x * 14.0;
    let shift_y = app.visor_gaze.norm_y * 3.5;
    let Ok(eyes) = mochi_gaze::port::compute_robo_visor_eyes(
        f64::from(w),
        f64::from(h),
        shift_x,
        shift_y,
        app.visor_gaze.scale_x,
        app.visor_gaze.scale_y,
        app.visor_sleeping,
        app.visor_sleeping,
    ) else {
        return;
    };
    let op = if app.visor_sleeping { 0.55 } else { 1.0 };
    let col = PURE_EYE_WHITE.with_alpha(op);

    for key in ["Left", "Right"] {
        let Some(r) = eyes.get(key) else {
            continue;
        };
        let raw_x = notch_x + r.get("X").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let raw_y = r.get("Y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let raw_w = r.get("Width").and_then(Value::as_f64).unwrap_or(8.0) as f32;
        let raw_h = r.get("Height").and_then(Value::as_f64).unwrap_or(14.5) as f32;

        let ew = raw_w * 1.45;
        let eh = if app.visor_sleeping {
            raw_h.max(2.2)
        } else {
            raw_h * 1.06
        };
        let ex = raw_x + (raw_w - ew) * 0.5;
        let ey = raw_y + (raw_h - eh) * 0.5;
        let rad = if app.visor_sleeping || eh < 5.0 {
            ew.min(eh) * 0.5
        } else {
            ew.min(eh) * 0.36
        };
        surf.fill_rounded_rect(ex, ey, ew, eh, rad, rad, col);
    }
}

pub(crate) fn draw_raven_sprite(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    anim: &RavenAnimator,
    x: f32,
    y: f32,
    size: f32,
) {
    let s = size / 80.0;
    let op = if anim.dim { 0.55 } else { 1.0 };
    let state = anim.state.to_ascii_lowercase();
    let phase = (anim.frame_index as f32) / 36.0 * std::f32::consts::TAU;

    let (dx, dy) = match state.as_str() {
        "yes" => (0.0, -(phase * 2.0).sin().abs() * 2.5 * s),
        "jump" => (0.0, -phase.sin().abs() * 5.5 * s),
        "no" => ((phase * 3.0).sin() * 3.2 * s, 0.0),
        _ if anim.dim => (0.0, 2.5 * s),
        _ => (0.0, 0.0),
    };

    let col = match state.as_str() {
        "alert" => PURE_EYE_AMBER,
        "no" => PURE_EYE_CORAL,
        _ => PURE_EYE_WHITE,
    }
    .with_alpha(op);

    let cx = x + size * 0.5 + dx;
    let cy = y + size * 0.5 + dy;
    let gap = 16.5 * s;
    let ew = (18.5 * s).max(2.4);

    if anim.dim {
        let eh = (3.6 * s).max(1.4);
        let rad = eh * 0.5;
        for sign in [-1.0_f32, 1.0_f32] {
            let ex = cx + sign * gap - ew * 0.5;
            let ey = cy - eh * 0.5;
            surf.fill_rounded_rect(ex, ey, ew, eh, rad, rad, col);
        }
        if size >= 36.0 {
            surf.draw_text(
                factories,
                "z",
                cx + 24.0 * s,
                cy - 20.0 * s,
                14.0 * s,
                14.0 * s,
                FontKind::Ui,
                (11.0 * s).max(8.0),
                true,
                col,
                HAlign::Left,
                VAlign::Top,
                false,
                false,
                false,
                false,
            );
        }
    } else {
        match state.as_str() {
            "yes" => {
                for sign in [-1.0_f32, 1.0_f32] {
                    surf.fill_crescent_eye(
                        factories,
                        cx + sign * gap,
                        cy,
                        ew * 1.05,
                        (16.0 * s).max(3.0),
                        col,
                    );
                }
            }
            "no" => {
                let eh = (18.0 * s).max(3.0);
                let rad = ew.min(eh) * 0.34;
                let drop = eh * 0.48;
                for (idx, sign) in [-1.0_f32, 1.0_f32].iter().enumerate() {
                    let ex = cx + sign * gap - ew * 0.5;
                    let ey = cy - eh * 0.5;
                    let (dl, dr) = if idx == 0 { (0.0, drop) } else { (drop, 0.0) };
                    surf.fill_sculpted_eye(factories, ex, ey, ew, eh, rad, dl, dr, col);
                }
            }
            "alert" => {
                let eh = (24.0 * s).max(3.5);
                let rad = ew.min(eh) * 0.38;
                for sign in [-1.0_f32, 1.0_f32] {
                    let ex = cx + sign * gap - ew * 0.5;
                    let ey = cy - eh * 0.5;
                    surf.fill_rounded_rect(ex, ey, ew, eh, rad, rad, col);
                }
                if size >= 24.0 {
                    surf.draw_text(
                        factories,
                        "!",
                        cx + 27.0 * s,
                        cy - 24.0 * s,
                        14.0 * s,
                        18.0 * s,
                        FontKind::Ui,
                        (13.0 * s).max(9.0),
                        true,
                        col,
                        HAlign::Left,
                        VAlign::Top,
                        false,
                        false,
                        false,
                        false,
                    );
                }
            }
            "jump" => {
                let eh = (23.0 * s).max(3.5);
                let rad = ew.min(eh) * 0.36;
                for sign in [-1.0_f32, 1.0_f32] {
                    let ex = cx + sign * gap - ew * 0.5;
                    let ey = cy - eh * 0.5;
                    surf.fill_rounded_rect(ex, ey, ew, eh, rad, rad, col);
                }
            }
            "thinking" => {
                let lh = (9.5 * s).max(2.0);
                let lrad = ew.min(lh) * 0.42;
                surf.fill_rounded_rect(cx - gap - ew * 0.5, cy - 3.0 * s, ew, lh, lrad, lrad, col);

                let rw = ew * 1.02;
                let rh = (22.5 * s).max(3.2);
                let rrad = rw.min(rh) * 0.36;
                surf.fill_rounded_rect(
                    cx + gap - rw * 0.5 + 2.0 * s,
                    cy - 13.0 * s,
                    rw,
                    rh,
                    rrad,
                    rrad,
                    col,
                );
            }
            _ => {
                let mut sx = anim.eye_shift_x.clamp(-15.0, 15.0);
                let mut sy = anim.eye_shift_y.clamp(-10.0, 10.0);
                let mag = ((sx / 15.0).powi(2) + (sy / 10.0).powi(2)).sqrt();
                if mag > 1.0 {
                    sx /= mag;
                    sy /= mag;
                }
                let norm_x = sx / 15.0;
                let lsx = 1.0 - 0.18 * (-norm_x).max(0.0);
                let rsx = 1.0 - 0.18 * norm_x.max(0.0);
                let lsh = 1.0 - 0.08 * (-norm_x).max(0.0);
                let rsh = 1.0 - 0.08 * norm_x.max(0.0);
                let esx = anim.eye_scale_x.clamp(0.2, 1.5);
                let esy = anim.eye_scale_y.clamp(0.05, 1.4);
                let bw = (18.5 * esx * f64::from(s)).max(2.2);
                let bh = (20.5 * esy * f64::from(s)).max(1.2);
                let lw = (bw * lsx).max(2.0) as f32;
                let rw = (bw * rsx).max(2.0) as f32;
                let lh = (bh * lsh).max(1.2) as f32;
                let rh = (bh * rsh).max(1.2) as f32;
                let gx = cx + (sx as f32) * 0.45 * s;
                let gy = cy + (sy as f32) * 0.45 * s;
                let half = gap * (1.0 - 0.08 * (norm_x * norm_x) as f32);

                for (ex, ey, w_eye, h_eye) in [
                    (gx - half - lw * 0.5, gy - lh * 0.5, lw, lh),
                    (gx + half - rw * 0.5, gy - rh * 0.5, rw, rh),
                ] {
                    let rad = if h_eye > 7.0 * s {
                        w_eye.min(h_eye) * 0.36
                    } else {
                        w_eye.min(h_eye) * 0.5
                    };
                    surf.fill_rounded_rect(ex, ey, w_eye, h_eye, rad, rad, col);
                }
            }
        }
    }

    if anim.letter {
        let bs = (size * 0.34).clamp(12.0, 22.0);
        let bx = if anim.facing_left {
            x + 1.0
        } else {
            x + size - bs - 1.0
        };
        let by = y + size - bs - 1.0;
        surf.fill_ellipse(
            bx + bs / 2.0,
            by + bs / 2.0,
            bs / 2.0 + 1.0,
            bs / 2.0 + 1.0,
            NOTCH_BLACK,
        );
        surf.draw_text(
            factories,
            "\u{E715}",
            bx,
            by,
            bs,
            bs,
            FontKind::Icon,
            (bs * 0.72).round(),
            false,
            TEXT_PRIMARY,
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
    }
}
