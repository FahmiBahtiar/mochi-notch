use chrono::Local;

use mochi_winrt::mochi_prayer::port::{PrayerMethod, format_countdown, format_hm};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_raven_sprite;
use crate::features::companion::state::RavenAnimator;
use crate::features::widgets::render::{draw_circle_button, draw_pill_button};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{Color, HOVER_FILL, SUBTLE_FILL, TEXT_PRIMARY, TEXT_SECONDARY};

#[expect(
    clippy::too_many_arguments,
    reason = "renders 1B Celestial Horizon prayer times page"
)]
pub(crate) fn draw_pray_page(
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
    let wx = px + 16.0;
    let wy = py + 4.0;
    let ww = pw - 32.0;
    let wh = (ph - 12.0).max(110.0);
    let s = app.lang.strings();

    let emerald = Color::argb(0xFF, 0x34, 0xD3, 0x99);
    let gold = Color::argb(0xFF, 0xFB, 0xBF, 0x24);
    let sunset = Color::argb(0xFF, 0xFB, 0x92, 0x3C);
    let dusk_violet = Color::argb(0xFF, 0xA7, 0x8B, 0xFA);

    if app.pray_picker_open {
        surf.fill_rounded_rect_radial_glow(
            wx,
            wy,
            ww,
            wh,
            16.0,
            wx + ww * 0.25,
            wy + wh * 0.3,
            ww * 0.55,
            wh * 0.9,
            emerald.with_alpha(0.16),
            Color::argb(0, 0, 0, 0),
        );
        surf.fill_rounded_rect(
            wx,
            wy,
            ww,
            wh,
            16.0,
            16.0,
            Color::argb(0x10, 0xFF, 0xFF, 0xFF),
        );
        surf.draw_rounded_rect(
            wx,
            wy,
            ww,
            wh,
            16.0,
            16.0,
            Color::argb(0x1E, 0xFF, 0xFF, 0xFF),
            1.0,
            None,
        );

        let pad = 14.0;
        let row_y = wy + 12.0;
        let close_w = 34.0;
        let loc_w = 148.0;
        let search_btn_w = 34.0;
        let input_w = (ww - 2.0 * pad - loc_w - search_btn_w - close_w - 18.0).max(160.0);

        let input_bg = if app.pray_search_focused {
            Color::argb(0x1E, 0xFF, 0xFF, 0xFF)
        } else if app.hover == HoverTarget::PraySearchInput {
            HOVER_FILL
        } else {
            Color::argb(0x14, 0xFF, 0xFF, 0xFF)
        };
        let input_border = if app.pray_search_focused {
            emerald.with_alpha(0.75)
        } else {
            Color::argb(0x22, 0xFF, 0xFF, 0xFF)
        };
        surf.fill_rounded_rect(wx + pad, row_y, input_w, 30.0, 15.0, 15.0, input_bg);
        surf.draw_rounded_rect(
            wx + pad,
            row_y,
            input_w,
            30.0,
            15.0,
            15.0,
            input_border,
            1.0,
            None,
        );
        let show_text = if app.pray_search_input.is_empty() {
            s.weather_search_city
        } else {
            app.pray_search_input.as_str()
        };
        let text_col = if app.pray_search_input.is_empty() {
            TEXT_SECONDARY
        } else {
            TEXT_PRIMARY
        };
        surf.draw_text(
            factories,
            show_text,
            wx + pad + 12.0,
            row_y,
            input_w - 24.0,
            30.0,
            FontKind::Ui,
            12.0,
            false,
            text_col,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
        if app.pray_search_focused {
            let tw = if app.pray_search_input.is_empty() {
                0.0
            } else {
                factories
                    .measure_text(
                        &app.pray_search_input,
                        FontKind::Ui,
                        12.0,
                        false,
                        false,
                        input_w - 24.0,
                        false,
                    )
                    .0
            };
            let cx_caret = (wx + pad + 12.0 + tw + 1.0).min(wx + pad + input_w - 10.0);
            surf.fill_rect(cx_caret, row_y + 7.0, 1.5, 16.0, TEXT_PRIMARY);
        }
        if interactive {
            hits.push(
                RectF {
                    x: wx + pad,
                    y: row_y,
                    w: input_w,
                    h: 30.0,
                },
                HoverTarget::PraySearchInput,
            );
        }

        let sbx = wx + pad + input_w + 6.0;
        draw_circle_button(
            factories,
            surf,
            sbx,
            row_y,
            30.0,
            "\u{E721}",
            12.0,
            SUBTLE_FILL,
            TEXT_PRIMARY,
            app.hover == HoverTarget::PraySearchBtn,
            app.pressed == HoverTarget::PraySearchBtn,
        );
        if interactive {
            hits.push(
                RectF {
                    x: sbx,
                    y: row_y,
                    w: 30.0,
                    h: 30.0,
                },
                HoverTarget::PraySearchBtn,
            );
        }

        let lbx = sbx + 36.0;
        draw_pill_button(
            factories,
            surf,
            lbx,
            row_y,
            loc_w,
            30.0,
            Some("\u{E81D}"),
            s.weather_locate,
            false,
            app.hover == HoverTarget::PrayLocateBtn,
            app.pressed == HoverTarget::PrayLocateBtn,
        );
        if interactive {
            hits.push(
                RectF {
                    x: lbx,
                    y: row_y,
                    w: loc_w,
                    h: 30.0,
                },
                HoverTarget::PrayLocateBtn,
            );
        }

        let cbx = wx + ww - pad - 30.0;
        draw_circle_button(
            factories,
            surf,
            cbx,
            row_y,
            30.0,
            "\u{E711}",
            10.5,
            SUBTLE_FILL,
            TEXT_SECONDARY,
            app.hover == HoverTarget::PrayClosePicker,
            app.pressed == HoverTarget::PrayClosePicker,
        );
        if interactive {
            hits.push(
                RectF {
                    x: cbx,
                    y: row_y,
                    w: 30.0,
                    h: 30.0,
                },
                HoverTarget::PrayClosePicker,
            );
        }

        let body_y = row_y + 40.0;
        let body_h = (wy + wh - 10.0 - body_y).max(40.0);
        if app.pray_places.is_empty() {
            let mut perch = RavenAnimator::default();
            perch.perch();
            draw_raven_sprite(
                factories,
                surf,
                &perch,
                wx + (ww - 36.0) * 0.5,
                body_y + 4.0,
                36.0,
            );
            surf.draw_text(
                factories,
                s.weather_setup_hint,
                wx + pad,
                body_y + 44.0,
                ww - 2.0 * pad,
                20.0,
                FontKind::Ui,
                12.0,
                false,
                TEXT_SECONDARY,
                HAlign::Center,
                VAlign::Top,
                false,
                false,
                true,
                false,
            );
        } else {
            let count = app.pray_places.len().min(3);
            let gap = 8.0;
            let card_w =
                ((ww - 2.0 * pad - gap * ((count - 1) as f32)) / (count as f32)).max(120.0);
            for (i, place) in app.pray_places.iter().take(3).enumerate() {
                let cx = wx + pad + (i as f32) * (card_w + gap);
                let hovered = app.hover == HoverTarget::PrayPickPlace(i);
                let bg = if hovered {
                    HOVER_FILL
                } else {
                    Color::argb(0x12, 0xFF, 0xFF, 0xFF)
                };
                surf.fill_rounded_rect(cx, body_y, card_w, body_h.min(64.0), 12.0, 12.0, bg);
                surf.draw_rounded_rect(
                    cx,
                    body_y,
                    card_w,
                    body_h.min(64.0),
                    12.0,
                    12.0,
                    if hovered {
                        emerald.with_alpha(0.55)
                    } else {
                        Color::argb(0x1A, 0xFF, 0xFF, 0xFF)
                    },
                    1.0,
                    None,
                );
                surf.draw_text(
                    factories,
                    &place.name,
                    cx + 12.0,
                    body_y + 10.0,
                    card_w - 24.0,
                    18.0,
                    FontKind::Ui,
                    12.5,
                    true,
                    TEXT_PRIMARY,
                    HAlign::Left,
                    VAlign::Center,
                    false,
                    false,
                    true,
                    false,
                );
                surf.draw_text(
                    factories,
                    &place.detail,
                    cx + 12.0,
                    body_y + 30.0,
                    card_w - 24.0,
                    16.0,
                    FontKind::Ui,
                    11.0,
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
                            x: cx,
                            y: body_y,
                            w: card_w,
                            h: body_h.min(64.0),
                        },
                        HoverTarget::PrayPickPlace(i),
                    );
                }
            }
        }
        return;
    }

    if app.pray_schedule.is_none() {
        app.recompute_prayer_schedule();
    }
    let Some(sched) = app.pray_schedule.clone() else {
        return;
    };
    let now = Local::now().naive_local();
    let (_, _, place_name) = app.pray_coords_and_place();
    let method = PrayerMethod::from_str_lossy(&app.store.data.settings.pray_method);
    let next_entry = &sched.prayers[sched.next_idx];
    let countdown = format_countdown(now, sched.next_time);
    let next_hm = format_hm(sched.next_time);
    let total_span = (sched.next_time - sched.prev_time).num_seconds().max(1) as f32;
    let elapsed_span = (now - sched.prev_time)
        .num_seconds()
        .clamp(0, total_span as i64) as f32;
    let prog = (elapsed_span / total_span).clamp(0.05, 0.98);

    let hero_w = 312.0_f32;
    let (sky_a, sky_b) = match sched.next_idx {
        0 => (dusk_violet, emerald),
        1 | 2 => (gold, emerald),
        3 => (sunset, dusk_violet),
        _ => (dusk_violet, Color::argb(0xFF, 0x38, 0xBD, 0xF8)),
    };
    surf.fill_rounded_rect_radial_glow(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        wx + hero_w * 0.5,
        wy + wh * 1.02,
        hero_w * 0.72,
        wh * 0.95,
        sky_a.with_alpha(0.28),
        Color::argb(0, 0, 0, 0),
    );
    surf.fill_rounded_rect_radial_glow(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        wx + hero_w * 0.22,
        wy + wh * 0.22,
        hero_w * 0.55,
        wh * 0.75,
        sky_b.with_alpha(0.14),
        Color::argb(0, 0, 0, 0),
    );
    surf.fill_rounded_rect(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        16.0,
        Color::argb(0x0E, 0xFF, 0xFF, 0xFF),
    );
    surf.draw_rounded_rect(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        16.0,
        sky_a.lerp(TEXT_PRIMARY, 0.2).with_alpha(0.30),
        1.0,
        None,
    );

    let horizon_y = wy + 82.0;
    surf.draw_line(
        wx + 14.0,
        horizon_y,
        wx + hero_w - 14.0,
        horizon_y,
        Color::argb(0x22, 0xFF, 0xFF, 0xFF),
        1.0,
        None,
    );
    let arc_x0 = wx + 24.0;
    let arc_x1 = wx + hero_w - 24.0;
    let arc_cx = (arc_x0 + arc_x1) * 0.5;
    let arc_cy = wy + 18.0;
    let eval_arc = |t: f32| -> (f32, f32) {
        let u = 1.0 - t;
        let x = u * u * arc_x0 + 2.0 * u * t * arc_cx + t * t * arc_x1;
        let y = u * u * horizon_y + 2.0 * u * t * arc_cy + t * t * horizon_y;
        (x, y)
    };
    for seg in 0..24 {
        if seg % 2 == 1 {
            continue;
        }
        let t0 = (seg as f32) / 24.0;
        let t1 = ((seg + 1) as f32) / 24.0;
        let p0 = eval_arc(t0);
        let p1 = eval_arc(t1);
        surf.draw_line(
            p0.0,
            p0.1,
            p1.0,
            p1.1,
            gold.with_alpha(0.24),
            1.4,
            Some(&factories.round_stroke),
        );
    }
    let steps = 20usize;
    let mut prev_pt = eval_arc(0.0);
    for s_i in 1..=steps {
        let t = prog * (s_i as f32) / (steps as f32);
        let pt = eval_arc(t);
        surf.draw_line(
            prev_pt.0,
            prev_pt.1,
            pt.0,
            pt.1,
            gold.with_alpha(0.88),
            2.2,
            Some(&factories.round_stroke),
        );
        prev_pt = pt;
    }
    let orb = eval_arc(prog);
    let phase = app.pray_anim_phase as f32;
    let orb_pulse = if app.reduce_motion {
        1.0
    } else {
        1.0 + 0.12 * phase.sin()
    };
    surf.fill_radial_glow(
        orb.0,
        orb.1,
        12.0 * orb_pulse,
        12.0 * orb_pulse,
        gold.with_alpha(0.34),
        Color::argb(0, 0, 0, 0),
    );
    surf.fill_ellipse(orb.0, orb.1, 4.8, 4.8, gold);
    surf.fill_ellipse(
        orb.0 - 1.1,
        orb.1 - 1.1,
        1.8,
        1.8,
        TEXT_PRIMARY.with_alpha(0.75),
    );
    if sched.next_idx == 0 || sched.next_idx == 4 {
        let tw = 0.45 + 0.55 * (phase * 1.4).sin().abs();
        surf.fill_sparkle(
            factories,
            wx + 52.0,
            wy + 48.0,
            2.4,
            phase * 10.0,
            TEXT_PRIMARY.with_alpha(tw),
        );
        surf.fill_sparkle(
            factories,
            wx + hero_w - 52.0,
            wy + 50.0,
            2.0,
            -phase * 8.0,
            sky_a.with_alpha(tw),
        );
    }

    let city_w = (22.0
        + factories
            .measure_text(&place_name, FontKind::Ui, 11.0, true, false, 136.0, false)
            .0
        + 14.0)
        .min(172.0);
    let city_hov = app.hover == HoverTarget::PrayCityBtn;
    surf.fill_rounded_rect(
        wx + 12.0,
        wy + 10.0,
        city_w,
        22.0,
        11.0,
        11.0,
        if city_hov {
            HOVER_FILL
        } else {
            Color::argb(0x48, 0x00, 0x00, 0x00)
        },
    );
    surf.draw_rounded_rect(
        wx + 12.0,
        wy + 10.0,
        city_w,
        22.0,
        11.0,
        11.0,
        Color::argb(0x1E, 0xFF, 0xFF, 0xFF),
        1.0,
        None,
    );
    surf.draw_text(
        factories,
        "\u{E707}",
        wx + 17.0,
        wy + 10.0,
        12.0,
        22.0,
        FontKind::Icon,
        10.0,
        false,
        emerald,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    surf.draw_text(
        factories,
        &place_name,
        wx + 30.0,
        wy + 10.0,
        (city_w - 26.0).max(20.0),
        22.0,
        FontKind::Ui,
        11.0,
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
                x: wx + 12.0,
                y: wy + 10.0,
                w: city_w,
                h: 22.0,
            },
            HoverTarget::PrayCityBtn,
        );
    }

    let alerts_on = app.store.data.settings.pray_alerts_enabled;
    let pre_mins = app.store.data.settings.pray_pre_alert_minutes;
    let pre_lbl = if pre_mins > 0 {
        format!("{pre_mins}m")
    } else {
        "0m".to_string()
    };
    let pre_w = 34.0_f32;
    let pre_x = wx + hero_w - 12.0 - pre_w;
    let pre_hov = app.hover == HoverTarget::PrayPreAlertToggle;
    surf.fill_rounded_rect(
        pre_x,
        wy + 10.0,
        pre_w,
        22.0,
        11.0,
        11.0,
        if pre_hov {
            HOVER_FILL
        } else if alerts_on && pre_mins > 0 {
            emerald.with_alpha(0.18)
        } else {
            Color::argb(0x48, 0x00, 0x00, 0x00)
        },
    );
    surf.draw_rounded_rect(
        pre_x,
        wy + 10.0,
        pre_w,
        22.0,
        11.0,
        11.0,
        if alerts_on && pre_mins > 0 {
            emerald.with_alpha(0.45)
        } else {
            Color::argb(0x1E, 0xFF, 0xFF, 0xFF)
        },
        1.0,
        None,
    );
    surf.draw_text(
        factories,
        &pre_lbl,
        pre_x,
        wy + 10.0,
        pre_w,
        22.0,
        FontKind::Ui,
        10.5,
        true,
        if alerts_on && pre_mins > 0 {
            emerald.lerp(TEXT_PRIMARY, 0.35)
        } else {
            TEXT_SECONDARY
        },
        HAlign::Center,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );
    if interactive {
        hits.push(
            RectF {
                x: pre_x,
                y: wy + 10.0,
                w: pre_w,
                h: 22.0,
            },
            HoverTarget::PrayPreAlertToggle,
        );
    }

    let alert_lbl = if alerts_on {
        s.pray_alert_on
    } else {
        s.pray_alert_off
    };
    let alert_w = (20.0
        + factories
            .measure_text(alert_lbl, FontKind::Ui, 10.5, true, false, 60.0, false)
            .0
        + 10.0)
        .max(46.0);
    let alert_x = pre_x - 5.0 - alert_w;
    let alert_hov = app.hover == HoverTarget::PrayAlertToggle;
    surf.fill_rounded_rect(
        alert_x,
        wy + 10.0,
        alert_w,
        22.0,
        11.0,
        11.0,
        if alert_hov {
            HOVER_FILL
        } else if alerts_on {
            emerald.with_alpha(0.18)
        } else {
            Color::argb(0x48, 0x00, 0x00, 0x00)
        },
    );
    surf.draw_rounded_rect(
        alert_x,
        wy + 10.0,
        alert_w,
        22.0,
        11.0,
        11.0,
        if alerts_on {
            emerald.with_alpha(0.45)
        } else {
            Color::argb(0x1E, 0xFF, 0xFF, 0xFF)
        },
        1.0,
        None,
    );
    surf.draw_text(
        factories,
        if alerts_on { "\u{EA8F}" } else { "\u{E7ED}" },
        alert_x + 6.0,
        wy + 10.0,
        12.0,
        22.0,
        FontKind::Icon,
        10.0,
        false,
        if alerts_on { emerald } else { TEXT_SECONDARY },
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    surf.draw_text(
        factories,
        alert_lbl,
        alert_x + 20.0,
        wy + 10.0,
        (alert_w - 26.0).max(16.0),
        22.0,
        FontKind::Ui,
        10.5,
        true,
        if alerts_on {
            emerald.lerp(TEXT_PRIMARY, 0.35)
        } else {
            TEXT_SECONDARY
        },
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
                x: alert_x,
                y: wy + 10.0,
                w: alert_w,
                h: 22.0,
            },
            HoverTarget::PrayAlertToggle,
        );
    }

    let band_y = wy + wh - 40.0;
    let hm_w = factories
        .measure_text(&next_hm, FontKind::Display, 26.0, true, true, 120.0, false)
        .0;
    surf.draw_text(
        factories,
        &next_hm,
        wx + 14.0,
        band_y,
        hm_w + 4.0,
        32.0,
        FontKind::Display,
        26.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );
    let cd_str = format!("-{countdown}");
    let cd_w = (factories
        .measure_text(&cd_str, FontKind::Ui, 11.0, true, true, 96.0, false)
        .0
        + 16.0)
        .max(68.0);
    let cd_x = wx + hero_w - 14.0 - cd_w;
    let cd_y = band_y + 5.0;
    surf.fill_rounded_rect(cd_x, cd_y, cd_w, 22.0, 11.0, 11.0, gold.with_alpha(0.16));
    surf.draw_rounded_rect(
        cd_x,
        cd_y,
        cd_w,
        22.0,
        11.0,
        11.0,
        gold.with_alpha(0.42),
        1.0,
        None,
    );
    surf.draw_text(
        factories,
        &cd_str,
        cd_x,
        cd_y,
        cd_w,
        22.0,
        FontKind::Ui,
        11.0,
        true,
        gold.lerp(TEXT_PRIMARY, 0.35),
        HAlign::Center,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );
    let name_x = wx + 14.0 + hm_w + 10.0;
    let name_w = (cd_x - 8.0 - name_x).max(20.0);
    surf.draw_text(
        factories,
        &next_entry.name,
        name_x,
        band_y + 4.0,
        name_w,
        24.0,
        FontKind::Ui,
        12.5,
        true,
        gold.lerp(TEXT_PRIMARY, 0.30),
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );

    let rx = wx + hero_w + 12.0;
    let rw = (ww - hero_w - 12.0).max(260.0);

    let hijri_w = factories
        .measure_text(&sched.hijri, FontKind::Ui, 11.0, true, false, 180.0, false)
        .0
        .min(180.0);
    surf.draw_text(
        factories,
        &sched.hijri,
        rx + 2.0,
        wy + 2.0,
        hijri_w + 4.0,
        20.0,
        FontKind::Ui,
        11.0,
        true,
        TEXT_PRIMARY.with_alpha(0.92),
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );

    let meth_lbl = method.label();
    let meth_w = (factories
        .measure_text(meth_lbl, FontKind::Ui, 10.0, true, false, 100.0, false)
        .0
        + 14.0)
        .max(56.0);
    let meth_x = rx + hijri_w + 10.0;
    let meth_hov = app.hover == HoverTarget::PrayMethodCycle;
    surf.fill_rounded_rect(
        meth_x,
        wy + 3.0,
        meth_w,
        18.0,
        9.0,
        9.0,
        if meth_hov {
            HOVER_FILL
        } else {
            Color::argb(0x14, 0xFF, 0xFF, 0xFF)
        },
    );
    surf.draw_text(
        factories,
        meth_lbl,
        meth_x,
        wy + 3.0,
        meth_w,
        18.0,
        FontKind::Ui,
        10.0,
        true,
        TEXT_SECONDARY.lerp(TEXT_PRIMARY, if meth_hov { 1.0 } else { 0.2 }),
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
                x: meth_x,
                y: wy + 3.0,
                w: meth_w,
                h: 18.0,
            },
            HoverTarget::PrayMethodCycle,
        );
    }

    let syuruq_s = format!("Syuruq {}", format_hm(sched.sunrise));
    surf.draw_text(
        factories,
        &syuruq_s,
        rx + rw - 100.0,
        wy + 2.0,
        98.0,
        20.0,
        FontKind::Ui,
        10.5,
        false,
        TEXT_SECONDARY,
        HAlign::Right,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );

    let pills_y = wy + 26.0;
    let pills_h = (wh - 26.0).max(80.0);
    let gap = 6.0_f32;
    let pill_w = ((rw - 4.0 * gap) / 5.0).max(52.0);
    let all_passed_today = sched.next_time.date() > sched.date;

    for (i, entry) in sched.prayers.iter().enumerate() {
        let px_i = rx + (i as f32) * (pill_w + gap);
        let is_next = i == sched.next_idx;
        let is_past = !is_next && (all_passed_today || entry.time <= now);
        let muted = app.is_prayer_muted(&entry.name);
        let card_op = if is_past { 0.48 } else { 1.0 };

        if is_next {
            surf.fill_rounded_rect_radial_glow(
                px_i,
                pills_y,
                pill_w,
                pills_h,
                14.0,
                px_i + pill_w * 0.5,
                pills_y + 10.0,
                pill_w * 0.85,
                pills_h * 0.75,
                gold.with_alpha(0.24),
                Color::argb(0, 0, 0, 0),
            );
            surf.fill_rounded_rect(
                px_i,
                pills_y,
                pill_w,
                pills_h,
                14.0,
                14.0,
                emerald.with_alpha(0.14),
            );
            surf.draw_rounded_rect(
                px_i,
                pills_y,
                pill_w,
                pills_h,
                14.0,
                14.0,
                gold.lerp(emerald, 0.4).with_alpha(0.60),
                1.2,
                None,
            );
        } else {
            surf.fill_rounded_rect(
                px_i,
                pills_y,
                pill_w,
                pills_h,
                14.0,
                14.0,
                Color::argb(0x0E, 0xFF, 0xFF, 0xFF).with_alpha(card_op),
            );
            surf.draw_rounded_rect(
                px_i,
                pills_y,
                pill_w,
                pills_h,
                14.0,
                14.0,
                Color::argb(0x1A, 0xFF, 0xFF, 0xFF).with_alpha(card_op),
                1.0,
                None,
            );
        }

        surf.draw_text(
            factories,
            &entry.name,
            px_i + 4.0,
            pills_y + 10.0,
            pill_w - 8.0,
            16.0,
            FontKind::Ui,
            10.5,
            is_next,
            if is_next {
                gold.lerp(TEXT_PRIMARY, 0.35)
            } else {
                TEXT_SECONDARY.with_alpha(card_op)
            },
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );

        let hm = format_hm(entry.time);
        surf.draw_text(
            factories,
            &hm,
            px_i + 4.0,
            pills_y + (pills_h - 22.0) * 0.5,
            pill_w - 8.0,
            22.0,
            FontKind::Ui,
            if is_next { 14.5 } else { 13.0 },
            true,
            TEXT_PRIMARY.with_alpha(card_op),
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );

        let bell_y = pills_y + pills_h - 24.0;
        let bell_hov = app.hover == HoverTarget::PrayMuteToggle(i);
        if bell_hov {
            surf.fill_rounded_rect(
                px_i + (pill_w - 28.0) * 0.5,
                bell_y,
                28.0,
                18.0,
                9.0,
                9.0,
                HOVER_FILL,
            );
        }
        if is_next && !bell_hov && !muted {
            surf.draw_text(
                factories,
                s.pray_next,
                px_i + 4.0,
                bell_y,
                pill_w - 8.0,
                18.0,
                FontKind::Ui,
                9.5,
                true,
                emerald.lerp(TEXT_PRIMARY, 0.25),
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
        } else if is_past && !bell_hov && !muted {
            surf.draw_text(
                factories,
                s.pray_done,
                px_i + 4.0,
                bell_y,
                pill_w - 8.0,
                18.0,
                FontKind::Ui,
                9.5,
                false,
                TEXT_SECONDARY.with_alpha(0.7),
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
        } else {
            surf.draw_text(
                factories,
                if muted { "\u{E7ED}" } else { "\u{EA8F}" },
                px_i + (pill_w - 16.0) * 0.5,
                bell_y,
                16.0,
                18.0,
                FontKind::Icon,
                10.0,
                false,
                if muted {
                    TEXT_SECONDARY.with_alpha(0.55)
                } else if is_next {
                    emerald
                } else {
                    TEXT_SECONDARY
                },
                HAlign::Center,
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
                    x: px_i + 6.0,
                    y: bell_y - 2.0,
                    w: pill_w - 12.0,
                    h: 22.0,
                },
                HoverTarget::PrayMuteToggle(i),
            );
        }
    }
}
