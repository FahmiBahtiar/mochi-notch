use windows::Win32::Graphics::Direct2D::ID2D1Geometry;
use windows_core::Interface;
use windows_numerics::Matrix3x2;

use mochi_winrt::strings::Language;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    CARD_BG, CLAUDE_ORANGE, COPILOT_BLUE, Color, DIVIDER, HOVER_FILL, INVERSE_FILL,
    INVERSE_FILL_PRESSED, INVERSE_HOVER_OVERLAY, INVERSE_TEXT, NOTCH_BLACK, PILL_FILL,
    PILL_FILL_PRESSED, PILL_FOREGROUND, PILL_HOVER_OVERLAY, PRESSED_FILL, SUBTLE_FILL,
    TEXT_PRIMARY, TEXT_SECONDARY,
};

#[expect(clippy::too_many_arguments, reason = "renders undo toast")]
pub(crate) fn draw_undo_toast(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    hits: &mut HitMap,
    x: f32,
    y: f32,
    w: f32,
    msg: &str,
    interactive: bool,
) {
    let s = app.lang.strings();
    let uw = factories
        .measure_text(s.undo_action, FontKind::Ui, 12.0, true, false, 100.0, false)
        .0
        + 20.0;
    let mw = factories
        .measure_text(msg, FontKind::Ui, 12.0, false, false, w - uw - 40.0, false)
        .0;
    let tw = (mw + uw + 32.0).min(w);
    let tx = x + (w - tw) / 2.0;
    surf.fill_rounded_rect(tx, y, tw, 32.0, 16.0, 16.0, CARD_BG);
    surf.draw_rounded_rect(tx, y, tw, 32.0, 16.0, 16.0, DIVIDER, 1.0, None);
    surf.draw_text(
        factories,
        msg,
        tx + 14.0,
        y,
        mw + 4.0,
        32.0,
        FontKind::Ui,
        12.0,
        false,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    let bx = tx + tw - uw - 6.0;
    if app.hover == HoverTarget::UndoBtn {
        surf.fill_rounded_rect(bx, y + 4.0, uw, 24.0, 12.0, 12.0, SUBTLE_FILL);
    }
    surf.draw_text(
        factories,
        s.undo_action,
        bx,
        y + 4.0,
        uw,
        24.0,
        FontKind::Ui,
        12.0,
        true,
        TEXT_PRIMARY,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    if interactive {
        hits.push(
            RectF {
                x: bx,
                y: y + 4.0,
                w: uw,
                h: 24.0,
            },
            HoverTarget::UndoBtn,
        );
    }
}

#[expect(clippy::too_many_arguments, reason = "renders 7-day bar chart")]
pub(crate) fn draw_day_bars(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    lang: Language,
    x: f32,
    y: f32,
    w: f32,
    bar_h: f32,
    days: &[(chrono::NaiveDate, f64)],
    max_val: f64,
    today_col: Color,
    past_col: Color,
) {
    let cell_w = w / (days.len().max(1) as f32);
    for (i, &(day, val)) in days.iter().enumerate() {
        let cx = x + (i as f32) * cell_w + 3.0;
        let bw = (cell_w - 6.0).max(2.0);
        let bh = ((bar_h as f64) * val / max_val.max(1.0)).max(3.0) as f32;
        let col = if i + 1 == days.len() {
            today_col
        } else {
            past_col
        };
        surf.fill_rounded_rect(cx, y + (bar_h - bh), bw, bh, 2.0, 2.0, col);
        let lbl = lang.weekday_initial(day);
        surf.draw_text(
            factories,
            lbl,
            cx - 2.0,
            y + bar_h + 3.0,
            bw + 4.0,
            14.0,
            FontKind::Ui,
            10.0,
            false,
            TEXT_SECONDARY,
            HAlign::Center,
            VAlign::Top,
            false,
            false,
            false,
            false,
        );
    }
}

#[expect(clippy::too_many_arguments, reason = "renders icon button")]
pub(crate) fn draw_icon_button(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    x: f32,
    y: f32,
    size: f32,
    glyph: &str,
    font_size: f32,
    fg: Color,
    hovered: bool,
    pressed: bool,
) {
    let r = size / 2.0;
    if pressed {
        surf.fill_ellipse(x + r, y + r, r, r, PRESSED_FILL);
    } else if hovered {
        surf.fill_ellipse(x + r, y + r, r, r, HOVER_FILL);
    }
    surf.draw_text(
        factories,
        glyph,
        x,
        y,
        size,
        size,
        FontKind::Icon,
        font_size,
        false,
        fg,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
}

#[expect(clippy::too_many_arguments, reason = "renders circular button")]
pub(crate) fn draw_circle_button(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    x: f32,
    y: f32,
    size: f32,
    glyph: &str,
    font_size: f32,
    bg: Color,
    fg: Color,
    hovered: bool,
    pressed: bool,
) {
    let r = size / 2.0;
    surf.fill_ellipse(x + r, y + r, r, r, bg);
    if pressed {
        surf.fill_ellipse(x + r, y + r, r, r, PRESSED_FILL);
    } else if hovered {
        surf.fill_ellipse(x + r, y + r, r, r, HOVER_FILL);
    }
    surf.draw_text(
        factories,
        glyph,
        x,
        y,
        size,
        size,
        FontKind::Icon,
        font_size,
        false,
        fg,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
}

#[expect(clippy::too_many_arguments, reason = "renders pill button")]
pub(crate) fn draw_pill_button(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    glyph: Option<&str>,
    label: &str,
    primary: bool,
    hovered: bool,
    pressed: bool,
) {
    let r = h / 2.0;
    let bg = if primary {
        if pressed {
            INVERSE_FILL_PRESSED
        } else {
            INVERSE_FILL
        }
    } else if pressed {
        PILL_FILL_PRESSED
    } else {
        PILL_FILL
    };
    surf.fill_rounded_rect(x, y, w, h, r, r, bg);
    if hovered && !pressed {
        surf.fill_rounded_rect(
            x,
            y,
            w,
            h,
            r,
            r,
            if primary {
                INVERSE_HOVER_OVERLAY
            } else {
                PILL_HOVER_OVERLAY
            },
        );
    }
    let fg = if primary {
        INVERSE_TEXT
    } else {
        PILL_FOREGROUND
    };
    if let Some(g) = glyph {
        let lw = factories
            .measure_text(label, FontKind::Ui, 12.0, primary, false, w, false)
            .0;
        let total = 10.0 + 6.0 + lw;
        let sx = x + (w - total) / 2.0;
        surf.draw_text(
            factories,
            g,
            sx,
            y,
            12.0,
            h,
            FontKind::Icon,
            10.0,
            false,
            fg,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
        surf.draw_text(
            factories,
            label,
            sx + 16.0,
            y,
            lw + 2.0,
            h,
            FontKind::Ui,
            12.0,
            primary,
            fg,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
    } else {
        surf.draw_text(
            factories,
            label,
            x,
            y,
            w,
            h,
            FontKind::Ui,
            12.0,
            primary,
            fg,
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "renders icon or brand mark with optional badge"
)]
pub(crate) fn draw_icon_view(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    glyph: &str,
    badge: Option<&str>,
    x: f32,
    y: f32,
    size: f32,
    fg: Color,
) {
    match glyph {
        "mark:claude" => {
            let cx = x + size / 2.0;
            let cy = y + size / 2.0;
            let sw = (size * 0.13).max(1.4);
            let r_long = size * 0.40;
            let r_short = size * 0.30;
            for i in 0..8 {
                let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
                let r = if i % 2 == 0 { r_long } else { r_short };
                surf.draw_line(
                    cx,
                    cy,
                    cx + r * angle.cos(),
                    cy + r * angle.sin(),
                    CLAUDE_ORANGE,
                    sw,
                    Some(&factories.round_stroke),
                );
            }
            surf.fill_ellipse(cx, cy, sw * 0.65, sw * 0.65, CLAUDE_ORANGE);
        }
        "mark:cursor" => {
            let ms = size * 0.92;
            let mx = x + (size - ms) / 2.0;
            let my = y + (size - ms) / 2.0;
            let scale = ms / 24.0;
            let m = Matrix3x2::scale(scale, scale) * Matrix3x2::translation(mx, my);
            surf.set_transform(m);
            let g: ID2D1Geometry = factories.pointer_geom.cast().unwrap();
            surf.fill_geometry(&g, TEXT_PRIMARY);
            surf.set_transform(Matrix3x2::identity());
        }
        "mark:codex" => {
            let pad = size * 0.08;
            let bw = size - 2.0 * pad;
            let sw = (size * 0.10).max(1.1);
            let rad = (bw * 0.24).max(2.0);
            surf.draw_rounded_rect(x + pad, y + pad, bw, bw, rad, rad, TEXT_PRIMARY, sw, None);
            let cy = y + size * 0.50;
            let lx = x + size * 0.28;
            let mx = x + size * 0.46;
            let dy = size * 0.14;
            surf.draw_line(
                lx,
                cy - dy,
                mx,
                cy,
                TEXT_PRIMARY,
                sw,
                Some(&factories.round_stroke),
            );
            surf.draw_line(
                lx,
                cy + dy,
                mx,
                cy,
                TEXT_PRIMARY,
                sw,
                Some(&factories.round_stroke),
            );
            surf.draw_line(
                x + size * 0.54,
                cy + dy,
                x + size * 0.72,
                cy + dy,
                TEXT_PRIMARY,
                sw,
                Some(&factories.round_stroke),
            );
        }
        "mark:copilot" => {
            let cx = x + size * 0.5;
            let cy = y + size * 0.52;
            let vw = size * 0.76;
            let vh = size * 0.54;
            let vx = cx - vw * 0.5;
            let vy = cy - vh * 0.5;
            let ear_w = (size * 0.14).max(1.6);
            let ear_h = vh * 0.56;
            let ear_y = cy - ear_h * 0.5;
            surf.fill_rounded_rect(
                vx - ear_w * 0.65,
                ear_y,
                ear_w,
                ear_h,
                ear_w * 0.5,
                ear_w * 0.5,
                COPILOT_BLUE,
            );
            surf.fill_rounded_rect(
                vx + vw - ear_w * 0.35,
                ear_y,
                ear_w,
                ear_h,
                ear_w * 0.5,
                ear_w * 0.5,
                COPILOT_BLUE,
            );
            surf.draw_quad_bezier(
                factories,
                cx - vw * 0.34,
                vy + 1.0,
                cx,
                vy - size * 0.22,
                cx + vw * 0.34,
                vy + 1.0,
                COPILOT_BLUE,
                (size * 0.10).max(1.2),
            );
            let rad = (vh * 0.42).max(2.0);
            surf.fill_rounded_rect(vx, vy, vw, vh, rad, rad, COPILOT_BLUE);
            let pad = (size * 0.09).max(1.1);
            let irad = ((vh - 2.0 * pad) * 0.38).max(1.4);
            surf.fill_rounded_rect(
                vx + pad,
                vy + pad,
                vw - 2.0 * pad,
                vh - 2.0 * pad,
                irad,
                irad,
                NOTCH_BLACK,
            );
            let ew = (size * 0.13).max(1.5);
            let eh = (size * 0.22).max(2.2);
            let gap = size * 0.14;
            for sign in [-1.0_f32, 1.0_f32] {
                surf.fill_rounded_rect(
                    cx + sign * gap - ew * 0.5,
                    cy - eh * 0.5,
                    ew,
                    eh,
                    ew * 0.5,
                    ew * 0.5,
                    TEXT_PRIMARY,
                );
            }
        }
        _ => {
            surf.draw_text(
                factories,
                glyph,
                x,
                y,
                size,
                size,
                FontKind::Icon,
                size,
                false,
                fg,
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
        }
    }
    if let Some(b) = badge
        && !b.is_empty()
    {
        let bs = (size * 0.75).round();
        let bx = x + size - bs + bs * 0.45;
        let by = y + size - bs + bs * 0.35;
        surf.fill_ellipse(
            bx + bs / 2.0,
            by + bs / 2.0,
            bs / 2.0,
            bs / 2.0,
            NOTCH_BLACK,
        );
        surf.draw_text(
            factories,
            b,
            bx,
            by,
            bs,
            bs,
            FontKind::Icon,
            (bs * 0.8).round(),
            false,
            fg,
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
    }
}

pub(crate) fn rounded_rect_geom(
    factories: &D2dFactories,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
) -> Option<ID2D1Geometry> {
    let rr = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
        rect: windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        },
        radiusX: r,
        radiusY: r,
    };
    // SAFETY: `factories.d2d` is valid.
    let g = unsafe { factories.d2d.CreateRoundedRectangleGeometry(&rr) }.ok()?;
    Some(g.cast().unwrap())
}
