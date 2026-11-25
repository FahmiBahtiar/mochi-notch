use chrono::Local;
use windows::Win32::Graphics::Direct2D::ID2D1Geometry;
use windows_core::Interface;
use windows_numerics::Matrix3x2;

use mochi_winrt::mochi_focus;
use mochi_winrt::strings::{fmt1, fmt2};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::focus::state::{FocusPhase, FocusState};
use crate::features::widgets::render::{draw_day_bars, draw_icon_button, draw_pill_button};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{FOCUS_PAST_BRUSH, SUBTLE_FILL, TEXT_PRIMARY, TEXT_SECONDARY, TRACK};

#[expect(clippy::too_many_arguments, reason = "renders focus page")]
pub(crate) fn draw_focus_page(
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
    let fx = px + 24.0;
    let fy = py;
    let fw = pw - 48.0;
    let fh = ph - 16.0;
    let s = app.lang.strings();

    if app.focus_picker_open {
        surf.draw_text(
            factories,
            s.focus_pick_task,
            fx,
            fy,
            fw - 30.0,
            24.0,
            FontKind::Ui,
            12.0,
            false,
            TEXT_SECONDARY,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
        draw_icon_button(
            factories,
            surf,
            fx + fw - 24.0,
            fy,
            24.0,
            "\u{E711}",
            9.0,
            TEXT_PRIMARY,
            app.hover == HoverTarget::FocusPickClose,
            app.pressed == HoverTarget::FocusPickClose,
        );
        if interactive {
            hits.push(
                RectF {
                    x: fx + fw - 24.0,
                    y: fy,
                    w: 24.0,
                    h: 24.0,
                },
                HoverTarget::FocusPickClose,
            );
        }
        let ly = fy + 28.0;
        let lh = (fh - 28.0).max(20.0);
        let open_tasks: Vec<(usize, String, String)> = app
            .store
            .data
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.done)
            .map(|(i, t)| (i, t.id.clone(), t.text.clone()))
            .collect();
        let mut row_idx = 0usize;
        let mut ry = ly - app.focus_picker_scroll as f32;
        surf.push_clip(fx, ly, fw, lh);
        if app.store.data.focus.task_id.is_some() {
            if app.hover == HoverTarget::FocusPickRow(row_idx) {
                surf.fill_rounded_rect(fx, ry, fw, 28.0, 6.0, 6.0, SUBTLE_FILL);
            }
            surf.draw_text(
                factories,
                s.focus_no_task,
                fx + 8.0,
                ry,
                fw - 16.0,
                28.0,
                FontKind::Ui,
                13.0,
                false,
                TEXT_SECONDARY,
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
                        x: fx,
                        y: ry,
                        w: fw,
                        h: 28.0,
                    },
                    HoverTarget::FocusPickRow(row_idx),
                );
            }
            ry += 28.0;
            row_idx += 1;
        }
        if open_tasks.is_empty() {
            surf.draw_text(
                factories,
                s.focus_no_open_tasks,
                fx,
                ry + 6.0,
                fw,
                40.0,
                FontKind::Ui,
                12.0,
                false,
                TEXT_SECONDARY,
                HAlign::Left,
                VAlign::Top,
                false,
                true,
                false,
                false,
            );
        } else {
            for (_, tid, ttext) in &open_tasks {
                if app.hover == HoverTarget::FocusPickRow(row_idx) {
                    surf.fill_rounded_rect(fx, ry, fw, 28.0, 6.0, 6.0, SUBTLE_FILL);
                }
                let selected = app.store.data.focus.task_id.as_deref() == Some(tid.as_str());
                surf.draw_text(
                    factories,
                    ttext,
                    fx + 8.0,
                    ry,
                    fw - 36.0,
                    28.0,
                    FontKind::Ui,
                    13.0,
                    false,
                    TEXT_PRIMARY,
                    HAlign::Left,
                    VAlign::Center,
                    false,
                    false,
                    true,
                    false,
                );
                if selected {
                    surf.draw_text(
                        factories,
                        "\u{E73E}",
                        fx + fw - 24.0,
                        ry,
                        16.0,
                        28.0,
                        FontKind::Icon,
                        11.0,
                        false,
                        TEXT_PRIMARY,
                        HAlign::Right,
                        VAlign::Center,
                        false,
                        false,
                        false,
                        false,
                    );
                }
                if interactive {
                    hits.push(
                        RectF {
                            x: fx,
                            y: ry,
                            w: fw,
                            h: 28.0,
                        },
                        HoverTarget::FocusPickRow(row_idx),
                    );
                }
                ry += 28.0;
                row_idx += 1;
            }
        }
        surf.pop_clip();
        return;
    }

    let ring_y = fy + (fh - 112.0) / 2.0;
    let arc_op = if app.focus.is_paused() { 0.4 } else { 1.0 };
    surf.draw_ellipse(fx + 56.0, ring_y + 56.0, 53.0, 53.0, TRACK, 6.0);
    let prog = app.focus.progress(&app.store.data.settings);
    if let Some(arc) = factories.build_arc_geometry(prog, 112.0, 6.0) {
        surf.set_transform(Matrix3x2::translation(fx, ring_y));
        let ag: ID2D1Geometry = arc.cast().unwrap();
        surf.draw_geometry(
            &ag,
            TEXT_PRIMARY.with_alpha(arc_op),
            6.0,
            Some(&factories.round_stroke),
        );
        surf.set_transform(Matrix3x2::identity());
    }
    let rem = app.focus.remaining();
    let time_s = format!("{:02}:{:02}", rem.as_secs() / 60, rem.as_secs() % 60);
    let time_op = if app.focus.is_paused() { 0.55 } else { 1.0 };
    surf.draw_text(
        factories,
        &time_s,
        fx,
        ring_y,
        112.0,
        112.0,
        FontKind::Display,
        26.0,
        true,
        TEXT_PRIMARY.with_alpha(time_op),
        HAlign::Center,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );

    let right_x = fx + fw - 112.0;
    let right_y = fy + (fh - 112.0) / 2.0;
    let today_secs = app.focus_today_seconds();
    let today_mins = today_secs / 60;
    let today_fmt = mochi_focus::port::format_focus_duration(today_mins);
    let goal_mins = i64::from(app.store.data.settings.focus_goal_minutes);
    let goal_fmt = mochi_focus::port::format_focus_duration(goal_mins);
    let goal_line = if today_mins >= goal_mins {
        s.focus_goal_met.to_string()
    } else {
        fmt1(s.focus_of_goal, &goal_fmt)
    };
    surf.draw_text(
        factories,
        s.focus_today,
        right_x,
        right_y,
        112.0,
        15.0,
        FontKind::Ui,
        11.0,
        false,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Top,
        false,
        false,
        false,
        false,
    );
    surf.draw_text(
        factories,
        &today_fmt,
        right_x,
        right_y + 15.0,
        112.0,
        26.0,
        FontKind::Display,
        20.0,
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
        &goal_line,
        right_x,
        right_y + 42.0,
        112.0,
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

    let today_date = Local::now().date_naive();
    let mut fdays = Vec::with_capacity(7);
    for i in 0..7 {
        let d = today_date - chrono::Duration::days(6 - i);
        let k = d.format("%Y-%m-%d").to_string();
        let val = if d == today_date {
            today_secs as f64 / 60.0
        } else {
            f64::from(app.store.data.focus.days.get(&k).copied().unwrap_or(0)) / 60.0
        };
        fdays.push((d, val));
    }
    let fmax = fdays.iter().map(|x| x.1).fold(goal_mins as f64, f64::max);
    draw_day_bars(
        factories,
        surf,
        app.lang,
        right_x - 3.0,
        right_y + 65.0,
        118.0,
        28.0,
        &fdays,
        fmax,
        TEXT_PRIMARY,
        FOCUS_PAST_BRUSH,
    );

    let mx = fx + 112.0 + 16.0;
    let mw = (right_x - 16.0 - mx).max(60.0);
    let my = fy + (fh - 116.0) / 2.0;
    let phase_txt = match app.focus.phase {
        FocusPhase::ShortBreak => s.focus_short_break,
        FocusPhase::LongBreak => s.focus_long_break,
        FocusPhase::Focus => s.focus_focus,
    };
    surf.draw_text(
        factories,
        phase_txt,
        mx,
        my,
        mw,
        22.0,
        FontKind::Ui,
        16.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Top,
        false,
        false,
        true,
        false,
    );
    let spc = app.focus.sessions_per_cycle(&app.store.data.settings);
    let sess = (app.focus.completed_sessions + 1).min(spc);
    let status_txt = match (app.focus.state, app.focus.is_break()) {
        (FocusState::Ready, _) => fmt2(s.focus_ready, &sess.to_string(), &spc.to_string()),
        (FocusState::Paused, false) => fmt2(s.focus_paused, &sess.to_string(), &spc.to_string()),
        (FocusState::Paused, true) => s.focus_break_on_hold.to_string(),
        (_, true) => s.focus_break_tip.to_string(),
        _ => {
            let end_dt =
                Local::now().naive_local() + chrono::Duration::seconds(rem.as_secs() as i64);
            let t_str = app.lang.format_short_time(end_dt);
            format!(
                "{}{}",
                fmt2(s.focus_session, &sess.to_string(), &spc.to_string()),
                fmt1(s.focus_break_at, &t_str)
            )
        }
    };
    surf.draw_text(
        factories,
        &status_txt,
        mx,
        my + 23.0,
        mw,
        16.0,
        FontKind::Ui,
        12.0,
        false,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Top,
        false,
        false,
        true,
        false,
    );

    for i in 0..spc {
        let dx = mx + (i as f32) * 14.0 + 4.0;
        let dy = my + 49.0;
        let col = if i < app.focus.completed_sessions {
            TEXT_PRIMARY
        } else {
            TRACK
        };
        surf.fill_ellipse(dx, dy, 4.0, 4.0, col);
    }

    let btn_y = my + 63.0;
    let (pglyph, plabel) = match app.focus.state {
        FocusState::Ready => ("\u{F5B0}", s.focus_start),
        FocusState::Running => ("\u{F8AE}", s.focus_pause),
        FocusState::Paused => ("\u{F5B0}", s.focus_resume),
    };
    let pbw = factories
        .measure_text(plabel, FontKind::Ui, 12.0, true, false, 140.0, false)
        .0
        + 44.0;
    draw_pill_button(
        factories,
        surf,
        mx,
        btn_y,
        pbw,
        30.0,
        Some(pglyph),
        plabel,
        true,
        app.hover == HoverTarget::FocusPrimary,
        app.pressed == HoverTarget::FocusPrimary,
    );
    if interactive {
        hits.push(
            RectF {
                x: mx,
                y: btn_y,
                w: pbw,
                h: 30.0,
            },
            HoverTarget::FocusPrimary,
        );
    }
    if app.focus.is_active() {
        let (sglyph, slabel) = if app.focus.is_break() {
            ("\u{E893}", s.focus_skip_break)
        } else {
            ("\u{E71A}", s.focus_stop)
        };
        let sbw = factories
            .measure_text(slabel, FontKind::Ui, 12.0, false, false, 140.0, false)
            .0
            + 40.0;
        let sbx = mx + pbw + 8.0;
        draw_pill_button(
            factories,
            surf,
            sbx,
            btn_y,
            sbw,
            30.0,
            Some(sglyph),
            slabel,
            false,
            app.hover == HoverTarget::FocusSecondary,
            app.pressed == HoverTarget::FocusSecondary,
        );
        if interactive {
            hits.push(
                RectF {
                    x: sbx,
                    y: btn_y,
                    w: sbw,
                    h: 30.0,
                },
                HoverTarget::FocusSecondary,
            );
        }
    }

    let ty = btn_y + 38.0;
    let picked = app
        .store
        .data
        .focus
        .task_id
        .as_deref()
        .and_then(|id| app.store.data.tasks.iter().find(|t| t.id == id && !t.done));
    let mut tx = mx;
    if picked.is_some() {
        draw_icon_button(
            factories,
            surf,
            mx - 5.0,
            ty,
            24.0,
            "\u{E739}",
            13.0,
            TEXT_PRIMARY,
            app.hover == HoverTarget::FocusTaskCheck,
            app.pressed == HoverTarget::FocusTaskCheck,
        );
        if interactive {
            hits.push(
                RectF {
                    x: mx - 5.0,
                    y: ty,
                    w: 24.0,
                    h: 24.0,
                },
                HoverTarget::FocusTaskCheck,
            );
        }
        tx += 22.0;
    }
    let pick_lbl = picked.map_or(s.focus_pick_task_short, |t| t.text.as_str());
    let pick_w = (factories
        .measure_text(pick_lbl, FontKind::Ui, 12.0, false, false, 160.0, false)
        .0
        .min(160.0)
        + 32.0)
        .min(mx + mw - tx);
    if app.hover == HoverTarget::FocusTaskPick {
        surf.fill_rounded_rect(tx - 6.0, ty, pick_w + 6.0, 24.0, 6.0, 6.0, SUBTLE_FILL);
    }
    if picked.is_none() {
        surf.draw_text(
            factories,
            "\u{E710}",
            tx,
            ty,
            12.0,
            24.0,
            FontKind::Icon,
            10.0,
            false,
            TEXT_SECONDARY,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
        tx += 16.0;
    }
    let lbl_w = (pick_w - 24.0).max(20.0);
    surf.draw_text(
        factories,
        pick_lbl,
        tx,
        ty,
        lbl_w,
        24.0,
        FontKind::Ui,
        12.0,
        false,
        if picked.is_some() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        },
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    surf.draw_text(
        factories,
        "\u{E70D}",
        tx + lbl_w + 4.0,
        ty + 1.0,
        10.0,
        24.0,
        FontKind::Icon,
        8.0,
        false,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    if interactive {
        hits.push(
            RectF {
                x: tx - 6.0,
                y: ty,
                w: pick_w + 6.0,
                h: 24.0,
            },
            HoverTarget::FocusTaskPick,
        );
    }
}
