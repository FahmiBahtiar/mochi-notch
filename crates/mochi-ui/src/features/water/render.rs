use chrono::Local;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::widgets::render::{draw_circle_button, draw_day_bars, draw_pill_button};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    TEXT_PRIMARY, TEXT_SECONDARY, TRACK, WATER_BRUSH, WATER_PAST_BRUSH, WATER_SUBTLE_FILL,
};

#[expect(clippy::too_many_arguments, reason = "renders water page")]
pub(crate) fn draw_water_page(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    hits: &mut HitMap,
    px: f32,
    py: f32,
    pw: f32,
    ph: f32,
    interactive: bool,
) {
    let wx = px + 24.0;
    let wy = py;
    let ww = pw - 48.0;
    let wh = ph - 16.0;
    let s = app.lang.strings();

    let right_w = 84.0;
    let rx = wx + ww - right_w;
    let ry = wy + (wh - 114.0) / 2.0;
    let btn_x = rx + (right_w - 58.0) / 2.0;
    draw_circle_button(
        factories,
        surf,
        btn_x,
        ry,
        58.0,
        "\u{EB42}",
        24.0,
        WATER_SUBTLE_FILL,
        WATER_BRUSH,
        app.hover == HoverTarget::WaterDrink,
        app.pressed == HoverTarget::WaterDrink,
    );
    if interactive {
        hits.push(
            RectF {
                x: btn_x,
                y: ry,
                w: 58.0,
                h: 58.0,
            },
            HoverTarget::WaterDrink,
        );
    }
    let cup_s = format!("+{} ml", app.store.data.water.cup_ml);
    surf.draw_text(
        factories,
        &cup_s,
        rx,
        ry + 62.0,
        right_w,
        16.0,
        FontKind::Ui,
        12.0,
        false,
        TEXT_PRIMARY,
        HAlign::Center,
        VAlign::Top,
        false,
        false,
        false,
        false,
    );
    let undo_w = (factories
        .measure_text(s.water_undo, FontKind::Ui, 12.0, false, false, 100.0, false)
        .0
        + 24.0)
        .max(64.0);
    let ux = rx + (right_w - undo_w) / 2.0;
    let uy = ry + 86.0;
    draw_pill_button(
        factories,
        surf,
        ux,
        uy,
        undo_w,
        28.0,
        None,
        s.water_undo,
        false,
        app.hover == HoverTarget::WaterUndo,
        app.pressed == HoverTarget::WaterUndo,
    );
    if interactive {
        hits.push(
            RectF {
                x: ux,
                y: uy,
                w: undo_w,
                h: 28.0,
            },
            HoverTarget::WaterUndo,
        );
    }

    let lw = (rx - 24.0 - wx).max(100.0);
    let ly = wy + (wh - 104.0) / 2.0;
    let today_ml = app.water_today_ml();
    let goal_ml = app.store.data.water.goal_ml;
    let today_s = app.lang.format_int(i64::from(today_ml));
    let goal_s = format!(" / {} ml", app.lang.format_int(i64::from(goal_ml)));
    let tw = factories
        .measure_text(&today_s, FontKind::Display, 26.0, true, true, lw, false)
        .0;
    surf.draw_text(
        factories,
        &today_s,
        wx,
        ly,
        tw + 4.0,
        34.0,
        FontKind::Display,
        26.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Top,
        true,
        false,
        false,
        false,
    );
    surf.draw_text(
        factories,
        &goal_s,
        wx + tw + 2.0,
        ly + 12.0,
        (lw - tw - 2.0).max(20.0),
        20.0,
        FontKind::Ui,
        13.0,
        false,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Top,
        false,
        false,
        false,
        false,
    );

    let bar_y = ly + 40.0;
    surf.fill_rounded_rect(wx, bar_y, lw, 6.0, 3.0, 3.0, TRACK);
    let fill_w = lw * (f64::from(today_ml) / f64::from(goal_ml.max(1))).min(1.0) as f32;
    surf.fill_rounded_rect(wx, bar_y, fill_w, 6.0, 3.0, 3.0, WATER_BRUSH);

    let today_date = Local::now().date_naive();
    let mut wdays = Vec::with_capacity(7);
    for i in 0..7 {
        let d = today_date - chrono::Duration::days(6 - i);
        let k = d.format("%Y-%m-%d").to_string();
        let val = f64::from(app.store.data.water.days.get(&k).copied().unwrap_or(0));
        wdays.push((d, val));
    }
    let wmax = wdays.iter().map(|x| x.1).fold(f64::from(goal_ml), f64::max);
    draw_day_bars(
        factories,
        surf,
        app.lang,
        wx - 3.0,
        bar_y + 16.0,
        lw + 6.0,
        30.0,
        &wdays,
        wmax,
        WATER_BRUSH,
        WATER_PAST_BRUSH,
    );
}
