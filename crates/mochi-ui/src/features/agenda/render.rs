use chrono::{Datelike, Local, NaiveDate, Timelike};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_companion_sprite;
use crate::features::widgets::render::{draw_icon_button, draw_pill_button};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    DIVIDER, INVERSE_FILL, INVERSE_TEXT, SELECTED_FILL, SUBTLE_FILL, TEXT_PRIMARY, TEXT_SECONDARY,
};

#[expect(clippy::too_many_arguments, reason = "renders agenda page")]
pub(crate) fn draw_agenda_page(
    app: &mut NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    hits: &mut HitMap,
    px: f32,
    py: f32,
    pw: f32,
    ph: f32,
    interactive: bool,
) {
    let ax = px + 16.0;
    let ay = py + 2.0;
    let aw = pw - 32.0;
    let ah = ph - 14.0;
    let today = Local::now().date_naive();

    let cal_w = 266.0_f32;
    let cal_x = ax;
    let cal_y = ay;
    let month_title = format!(
        "{} {}",
        app.lang.month_long(app.calendar_view_month),
        app.calendar_view_year
    );
    let title_w = (cal_w - 50.0).max(60.0);
    if app.hover == HoverTarget::CalendarToday {
        surf.fill_rounded_rect(cal_x, cal_y, title_w, 20.0, 6.0, 6.0, SUBTLE_FILL);
    }
    surf.draw_text(
        factories,
        &month_title,
        cal_x + 6.0,
        cal_y,
        title_w - 8.0,
        20.0,
        FontKind::Ui,
        12.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    if interactive {
        hits.push(
            RectF {
                x: cal_x,
                y: cal_y,
                w: title_w,
                h: 20.0,
            },
            HoverTarget::CalendarToday,
        );
    }

    let prev_x = cal_x + cal_w - 44.0;
    let next_x = cal_x + cal_w - 20.0;
    draw_icon_button(
        factories,
        surf,
        prev_x,
        cal_y,
        20.0,
        "\u{E76B}",
        9.5,
        TEXT_SECONDARY,
        app.hover == HoverTarget::CalendarPrevMonth,
        app.pressed == HoverTarget::CalendarPrevMonth,
    );
    draw_icon_button(
        factories,
        surf,
        next_x,
        cal_y,
        20.0,
        "\u{E76C}",
        9.5,
        TEXT_SECONDARY,
        app.hover == HoverTarget::CalendarNextMonth,
        app.pressed == HoverTarget::CalendarNextMonth,
    );
    if interactive {
        hits.push(
            RectF {
                x: prev_x,
                y: cal_y,
                w: 20.0,
                h: 20.0,
            },
            HoverTarget::CalendarPrevMonth,
        );
        hits.push(
            RectF {
                x: next_x,
                y: cal_y,
                w: 20.0,
                h: 20.0,
            },
            HoverTarget::CalendarNextMonth,
        );
    }

    let col_w = cal_w / 7.0;
    let wd_y = cal_y + 21.0;
    let base_sun = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
    for d in 0..7 {
        let lbl = app
            .lang
            .weekday_initial(base_sun + chrono::Duration::days(d));
        surf.draw_text(
            factories,
            lbl,
            cal_x + (d as f32) * col_w,
            wd_y,
            col_w,
            14.0,
            FontKind::Ui,
            9.5,
            true,
            TEXT_SECONDARY,
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
    }

    let grid_y = wd_y + 14.0;
    let row_h = ((ah - 35.0) / 6.0).clamp(15.0, 18.0);
    if let Some(first) = NaiveDate::from_ymd_opt(app.calendar_view_year, app.calendar_view_month, 1)
    {
        let start_off = i64::from(first.weekday().num_days_from_sunday());
        for cell in 0..42_i64 {
            let date = first + chrono::Duration::days(cell - start_off);
            let col = (cell % 7) as f32;
            let row = (cell / 7) as f32;
            let cx = cal_x + col * col_w;
            let cy = grid_y + row * row_h;
            let pill_w = (col_w - 8.0).max(22.0);
            let pill_h = (row_h - 1.5).max(14.0);
            let pill_x = cx + (col_w - pill_w) * 0.5;
            let pill_y = cy + (row_h - pill_h) * 0.5;
            let pill_r = pill_h * 0.5;
            let in_month = date.month() == app.calendar_view_month;
            let is_today = date == today;
            let is_selected = date == app.calendar_selected_date;
            let target = HoverTarget::CalendarDay(date.year(), date.month(), date.day());
            let hovered = app.hover == target;

            let has_ev = app.calendar_has_event_on(date);
            if is_today {
                surf.fill_rounded_rect(
                    pill_x,
                    pill_y,
                    pill_w,
                    pill_h,
                    pill_r,
                    pill_r,
                    INVERSE_FILL,
                );
            } else if is_selected {
                surf.fill_rounded_rect(
                    pill_x,
                    pill_y,
                    pill_w,
                    pill_h,
                    pill_r,
                    pill_r,
                    SELECTED_FILL,
                );
                surf.draw_rounded_rect(
                    pill_x, pill_y, pill_w, pill_h, pill_r, pill_r, app.accent, 1.1, None,
                );
            } else if hovered {
                surf.fill_rounded_rect(pill_x, pill_y, pill_w, pill_h, pill_r, pill_r, SUBTLE_FILL);
            }

            let txt_col = if is_today {
                INVERSE_TEXT
            } else if in_month {
                TEXT_PRIMARY
            } else {
                TEXT_SECONDARY.with_alpha(0.4)
            };
            let day_str = date.day().to_string();
            let text_x = if has_ev { cx - 2.0 } else { cx };
            surf.draw_text(
                factories,
                &day_str,
                text_x,
                cy,
                col_w,
                row_h,
                FontKind::Ui,
                10.5,
                is_today || is_selected,
                txt_col,
                HAlign::Center,
                VAlign::Center,
                true,
                false,
                false,
                false,
            );

            if has_ev {
                let dot_col = if is_today { INVERSE_TEXT } else { app.accent };
                surf.fill_ellipse(pill_x + pill_w - 5.0, cy + row_h * 0.5, 1.6, 1.6, dot_col);
            }

            if interactive {
                hits.push(
                    RectF {
                        x: cx,
                        y: cy,
                        w: col_w,
                        h: row_h,
                    },
                    target,
                );
            }
        }
    }

    let div_x = cal_x + cal_w + 12.0;
    surf.fill_rect(div_x, ay + 4.0, 1.0, (ah - 8.0).max(20.0), DIVIDER);

    let rx = div_x + 14.0;
    let rw = (ax + aw - rx).max(120.0);
    let sel_label = app.lang.format_weekday_date(app.calendar_selected_date);
    surf.draw_text(
        factories,
        &sel_label,
        rx + 4.0,
        ay,
        rw - 8.0,
        20.0,
        FontKind::Ui,
        12.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );

    let list_y = ay + 24.0;
    let list_h = (ah - 24.0).max(40.0);
    let rows = app.build_agenda_rows_for_selected();
    if rows.is_empty() {
        let s = app.lang.strings();
        let signed_in = app.google_auth.lock().is_ok_and(|g| g.is_signed_in());
        let configured = app.google_auth.lock().is_ok_and(|g| g.is_configured());
        let (text, btn) = if !configured {
            (s.agenda_not_configured, None)
        } else if app.calendar_connecting {
            (s.agenda_connecting, Some(s.common_cancel))
        } else if !signed_in {
            (
                app.calendar_error.as_deref().unwrap_or(s.agenda_signed_out),
                Some(s.agenda_connect),
            )
        } else if let Some(ref err) = app.calendar_error {
            (err.as_str(), None)
        } else if app.calendar_selected_date == today && Local::now().hour() >= 18 {
            (s.agenda_nothing_left, None)
        } else {
            (s.agenda_free, None)
        };
        let cy = list_y + (list_h - if btn.is_some() { 96.0 } else { 64.0 }) * 0.5;
        draw_companion_sprite(
            factories,
            surf,
            &app.idle_companion,
            rx + (rw - 34.0) / 2.0,
            cy,
            34.0,
        );
        surf.draw_text(
            factories,
            text,
            rx + 8.0,
            cy + 36.0,
            (rw - 16.0).max(60.0),
            32.0,
            FontKind::Ui,
            12.0,
            false,
            TEXT_SECONDARY,
            HAlign::Center,
            VAlign::Top,
            false,
            true,
            false,
            false,
        );
        if let Some(lbl) = btn {
            let bw = (factories
                .measure_text(lbl, FontKind::Ui, 12.0, true, false, rw - 24.0, false)
                .0
                + 28.0)
                .max(90.0);
            let bx = rx + (rw - bw) / 2.0;
            let by = cy + 70.0;
            draw_pill_button(
                factories,
                surf,
                bx,
                by,
                bw,
                26.0,
                None,
                lbl,
                true,
                app.hover == HoverTarget::AgendaEmptyBtn,
                app.pressed == HoverTarget::AgendaEmptyBtn,
            );
            if interactive {
                hits.push(
                    RectF {
                        x: bx,
                        y: by,
                        w: bw,
                        h: 26.0,
                    },
                    HoverTarget::AgendaEmptyBtn,
                );
            }
        }
        return;
    }

    if interactive {
        hits.scroll_rect = Some(RectF {
            x: rx,
            y: list_y,
            w: rw,
            h: list_h,
        });
    }
    let total_h: f32 = rows
        .iter()
        .map(|r| if r.is_header { 24.0 } else { 38.0 })
        .sum();
    let max_scroll = (total_h - list_h).max(0.0) as f64;
    app.agenda_scroll = app.agenda_scroll.clamp(0.0, max_scroll);
    let scroll = app.agenda_scroll as f32;

    let above = scroll > 0.5;
    let below = scroll < max_scroll as f32 - 0.5;
    let mask = if (above || below) && list_h > 56.0 {
        surf.create_linear_mask(
            rx,
            list_y,
            rx,
            list_y + list_h,
            &[
                (0.0, if above { 0.0 } else { 1.0 }),
                (28.0 / list_h, 1.0),
                (1.0 - 28.0 / list_h, 1.0),
                (1.0, if below { 0.0 } else { 1.0 }),
            ],
        )
    } else {
        None
    };
    surf.push_clip(rx, list_y, rw, list_h);
    if mask.is_some() {
        surf.push_layer(rx, list_y, rw, list_h, 1.0, None, mask.as_ref());
    }
    let mut ry = list_y - scroll;
    for (i, row) in rows.iter().enumerate() {
        let rh = if row.is_header { 24.0 } else { 38.0 };
        if ry + rh >= list_y && ry <= list_y + list_h {
            if row.is_header {
                surf.draw_text(
                    factories,
                    &row.title,
                    rx + 4.0,
                    ry + 8.0,
                    rw - 8.0,
                    16.0,
                    FontKind::Ui,
                    11.0,
                    false,
                    TEXT_SECONDARY,
                    HAlign::Left,
                    VAlign::Top,
                    false,
                    false,
                    true,
                    false,
                );
            } else {
                if row.is_now {
                    surf.fill_rounded_rect(rx, ry, rw, 38.0, 10.0, 10.0, SUBTLE_FILL);
                }
                surf.draw_text(
                    factories,
                    &row.time,
                    rx + 6.0,
                    ry,
                    42.0,
                    38.0,
                    FontKind::Ui,
                    11.5,
                    false,
                    if row.is_now {
                        TEXT_PRIMARY
                    } else {
                        TEXT_SECONDARY
                    },
                    HAlign::Left,
                    VAlign::Center,
                    true,
                    false,
                    false,
                    false,
                );
                let mut join_w = 0.0;
                if row.meeting_url.is_some() && !row.is_past {
                    let lbl = app.lang.strings().agenda_join;
                    let bw = factories
                        .measure_text(lbl, FontKind::Ui, 11.5, row.is_now, false, 90.0, false)
                        .0
                        + 34.0;
                    let bh = 26.0;
                    let bx = rx + rw - 4.0 - bw;
                    let by = ry + (38.0 - bh) / 2.0;
                    draw_pill_button(
                        factories,
                        surf,
                        bx,
                        by,
                        bw,
                        bh,
                        Some("\u{E714}"),
                        lbl,
                        row.is_now,
                        app.hover == HoverTarget::AgendaJoin(i),
                        app.pressed == HoverTarget::AgendaJoin(i),
                    );
                    if interactive {
                        hits.push(
                            RectF {
                                x: bx,
                                y: by,
                                w: bw,
                                h: bh,
                            },
                            HoverTarget::AgendaJoin(i),
                        );
                    }
                    join_w = bw + 8.0;
                }
                let tw = (rw - 50.0 - join_w - 6.0).max(20.0);
                surf.draw_text(
                    factories,
                    &row.title,
                    rx + 50.0,
                    ry + 4.0,
                    tw,
                    18.0,
                    FontKind::Ui,
                    12.5,
                    true,
                    if row.is_past {
                        TEXT_SECONDARY
                    } else {
                        TEXT_PRIMARY
                    },
                    HAlign::Left,
                    VAlign::Top,
                    false,
                    false,
                    true,
                    false,
                );
                surf.draw_text(
                    factories,
                    &row.detail,
                    rx + 50.0,
                    ry + 20.0,
                    tw,
                    15.0,
                    FontKind::Ui,
                    11.0,
                    false,
                    TEXT_SECONDARY,
                    HAlign::Left,
                    VAlign::Top,
                    false,
                    false,
                    true,
                    false,
                );
            }
        }
        ry += rh;
    }
    if mask.is_some() {
        surf.pop_layer();
    }
    surf.pop_clip();
}
