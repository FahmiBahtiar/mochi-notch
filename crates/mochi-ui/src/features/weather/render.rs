use mochi_winrt::strings::{fmt1, fmt2};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_raven_sprite;
use crate::features::companion::state::RavenAnimator;
use crate::features::widgets::render::{draw_circle_button, draw_pill_button};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    Color, HOVER_FILL, SUBTLE_FILL, TEXT_PRIMARY, TEXT_SECONDARY, WATER_BRUSH, weather_runs,
};

pub(crate) fn draw_weather_sprite(surf: &mut D2dSurface, icon: &str, x: f32, y: f32, px: f32) {
    for (row, start, len) in weather_runs(icon) {
        surf.fill_rect(
            x + (start as f32) * px,
            y + (row as f32) * px,
            (len as f32) * px,
            px,
            TEXT_PRIMARY,
        );
    }
}

fn weather_palette(icon: &str, is_day: bool) -> (Color, Color) {
    match icon {
        "Clear" | "MostlyClear" if is_day => (
            Color::argb(0xFF, 0xFB, 0xBF, 0x24),
            Color::argb(0xFF, 0xFB, 0x92, 0x3C),
        ),
        "Clear" | "MostlyClear" => (
            Color::argb(0xFF, 0x81, 0x8C, 0xF8),
            Color::argb(0xFF, 0x38, 0xBD, 0xF8),
        ),
        "PartlyCloudy" if is_day => (
            Color::argb(0xFF, 0xFB, 0xBF, 0x24),
            Color::argb(0xFF, 0x60, 0xA5, 0xFA),
        ),
        "PartlyCloudy" => (
            Color::argb(0xFF, 0x60, 0xA5, 0xFA),
            Color::argb(0xFF, 0x81, 0x8C, 0xF8),
        ),
        "Thunderstorm" => (
            Color::argb(0xFF, 0xA7, 0x8B, 0xFA),
            Color::argb(0xFF, 0x38, 0xBD, 0xF8),
        ),
        "Rain" | "HeavyRain" | "Drizzle" | "Showers" => (
            Color::argb(0xFF, 0x38, 0xBD, 0xF8),
            Color::argb(0xFF, 0x3B, 0x82, 0xF6),
        ),
        "Snow" => (
            Color::argb(0xFF, 0xBA, 0xE6, 0xFD),
            Color::argb(0xFF, 0x60, 0xA5, 0xFA),
        ),
        _ => (
            Color::argb(0xFF, 0x94, 0xA3, 0xB8),
            Color::argb(0xFF, 0x38, 0xBD, 0xF8),
        ),
    }
}

#[expect(clippy::too_many_arguments, reason = "renders animated weather scene")]
fn draw_weather_scene(
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    icon: &str,
    is_day: bool,
    phase: f32,
    x: f32,
    y: f32,
    size: f32,
) {
    let cx = x + size * 0.5;
    let cy = y + size * 0.5;
    let (primary, secondary) = weather_palette(icon, is_day);
    let drift = phase.sin() * 2.2;

    surf.push_clip(x - 4.0, y - 4.0, size + 8.0, size + 8.0);
    surf.fill_radial_glow(
        cx,
        cy,
        size * 0.55,
        size * 0.55,
        primary.with_alpha(0.28),
        Color::argb(0, 0, 0, 0),
    );

    let has_sun = is_day && matches!(icon, "Clear" | "MostlyClear" | "PartlyCloudy");
    let has_moon = !is_day && matches!(icon, "Clear" | "MostlyClear" | "PartlyCloudy");
    let has_cloud = !matches!(icon, "Clear");
    let has_rain = matches!(
        icon,
        "Rain" | "HeavyRain" | "Drizzle" | "Showers" | "Thunderstorm"
    );
    let has_storm = icon == "Thunderstorm";
    let has_snow = icon == "Snow";

    if has_sun {
        let sx = if has_cloud { cx - 7.0 } else { cx };
        let sy = if has_cloud { cy - 7.0 } else { cy };
        let r = if has_cloud { 9.0 } else { 12.0 };
        let ray_r1 = r + 3.5;
        let ray_r2 = r + 7.0 + 1.2 * (phase * 1.5).sin();
        for k in 0..8 {
            let ang = (k as f32) * (std::f32::consts::TAU / 8.0) + phase * 0.25;
            surf.draw_line(
                sx + ang.cos() * ray_r1,
                sy + ang.sin() * ray_r1,
                sx + ang.cos() * ray_r2,
                sy + ang.sin() * ray_r2,
                primary.with_alpha(0.85),
                1.8,
                None,
            );
        }
        surf.fill_ellipse(sx, sy, r, r, primary);
        surf.fill_ellipse(
            sx - r * 0.25,
            sy - r * 0.25,
            r * 0.45,
            r * 0.45,
            Color::argb(0x55, 0xFF, 0xFF, 0xFF),
        );
    }

    if has_moon {
        let mx = if has_cloud { cx - 7.0 } else { cx };
        let my = if has_cloud { cy - 6.0 } else { cy };
        let r = if has_cloud { 8.5 } else { 11.5 };
        surf.fill_ellipse(mx, my, r, r, Color::argb(0xFF, 0xE0, 0xE7, 0xFF));
        surf.fill_ellipse(
            mx + r * 0.42,
            my - r * 0.28,
            r * 0.82,
            r * 0.82,
            Color::argb(0xFF, 0x12, 0x14, 0x1E),
        );
        let tw1 = 0.45 + 0.55 * (phase * 1.7).sin().abs();
        let tw2 = 0.45 + 0.55 * (phase * 1.3 + 1.4).cos().abs();
        surf.fill_sparkle(
            factories,
            mx + 14.0,
            my - 10.0,
            3.2,
            phase * 12.0,
            secondary.with_alpha(tw1),
        );
        surf.fill_sparkle(
            factories,
            mx - 14.0,
            my + 8.0,
            2.4,
            -phase * 10.0,
            TEXT_PRIMARY.with_alpha(tw2),
        );
    }

    if has_cloud {
        let cl_x = cx + drift;
        let cl_y = if has_rain || has_snow {
            cy - 2.0
        } else {
            cy + 4.0
        };
        let cloud_col = if has_storm {
            Color::argb(0xFF, 0x94, 0xA3, 0xB8)
        } else if has_rain {
            Color::argb(0xFF, 0xCB, 0xD5, 0xE1)
        } else {
            Color::argb(0xFF, 0xF1, 0xF5, 0xF9)
        };
        surf.fill_ellipse(cl_x - 8.0, cl_y + 1.0, 8.5, 7.5, cloud_col.with_alpha(0.92));
        surf.fill_ellipse(cl_x + 1.0, cl_y - 3.0, 10.5, 9.5, cloud_col);
        surf.fill_ellipse(
            cl_x + 10.0,
            cl_y + 2.0,
            7.5,
            6.5,
            cloud_col.with_alpha(0.95),
        );
        surf.fill_rounded_rect(cl_x - 16.0, cl_y + 1.0, 33.0, 9.0, 4.5, 4.5, cloud_col);
    }

    if has_rain {
        let drops = if icon == "HeavyRain" || has_storm {
            5
        } else {
            3
        };
        for d in 0..drops {
            let t = ((phase * 0.55 + (d as f32) * 0.27) % 1.0 + 1.0) % 1.0;
            let dx = cx - 11.0 + (d as f32) * (22.0 / (drops.max(2) - 1) as f32) + drift * 0.5;
            let dy = cy + 9.0 + t * 14.0;
            let op = (1.0 - t).clamp(0.15, 0.95);
            surf.draw_line(
                dx,
                dy,
                dx - 2.2,
                dy + 5.5,
                secondary.with_alpha(op),
                1.8,
                None,
            );
        }
    }

    if has_storm {
        let flash = ((phase * 1.8).sin() > 0.72) as i32 as f32;
        let bolt_col = Color::argb(0xFF, 0xFD, 0xE0, 0x47).with_alpha(0.55 + 0.45 * flash);
        surf.draw_line(cx + 1.0, cy + 7.0, cx - 3.0, cy + 14.0, bolt_col, 2.0, None);
        surf.draw_line(
            cx - 3.0,
            cy + 14.0,
            cx + 2.0,
            cy + 14.0,
            bolt_col,
            2.0,
            None,
        );
        surf.draw_line(
            cx + 2.0,
            cy + 14.0,
            cx - 2.0,
            cy + 22.0,
            bolt_col,
            2.0,
            None,
        );
    }

    if has_snow {
        for d in 0..4 {
            let t = ((phase * 0.35 + (d as f32) * 0.25) % 1.0 + 1.0) % 1.0;
            let dx = cx - 11.0 + (d as f32) * 7.5 + (phase + d as f32).sin() * 2.0;
            let dy = cy + 9.0 + t * 14.0;
            surf.fill_ellipse(dx, dy, 1.8, 1.8, primary.with_alpha(1.0 - t * 0.7));
        }
    }

    surf.pop_clip();
}

#[expect(
    clippy::too_many_arguments,
    reason = "renders Atmospheric Horizon weather page"
)]
pub(crate) fn draw_weather_page(
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

    if app.weather_report.is_none() || app.weather_picker_open {
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
            app.accent.with_alpha(0.16),
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
        let close_w = if app.weather_report.is_some() {
            34.0
        } else {
            0.0
        };
        let loc_w = 148.0;
        let search_btn_w = 34.0;
        let input_w = (ww - 2.0 * pad - loc_w - search_btn_w - close_w - 18.0).max(160.0);

        let input_bg = if app.weather_search_focused {
            Color::argb(0x1E, 0xFF, 0xFF, 0xFF)
        } else if app.hover == HoverTarget::WeatherSearchInput {
            HOVER_FILL
        } else {
            Color::argb(0x14, 0xFF, 0xFF, 0xFF)
        };
        let input_border = if app.weather_search_focused {
            app.accent.with_alpha(0.75)
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
        let show_text = if app.weather_search_input.is_empty() {
            s.weather_search_city
        } else {
            app.weather_search_input.as_str()
        };
        let text_col = if app.weather_search_input.is_empty() {
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
        if app.weather_search_focused {
            let tw = if app.weather_search_input.is_empty() {
                0.0
            } else {
                factories
                    .measure_text(
                        &app.weather_search_input,
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
                HoverTarget::WeatherSearchInput,
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
            app.hover == HoverTarget::WeatherSearchBtn,
            app.pressed == HoverTarget::WeatherSearchBtn,
        );
        if interactive {
            hits.push(
                RectF {
                    x: sbx,
                    y: row_y,
                    w: 30.0,
                    h: 30.0,
                },
                HoverTarget::WeatherSearchBtn,
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
            app.hover == HoverTarget::WeatherLocateBtn,
            app.pressed == HoverTarget::WeatherLocateBtn,
        );
        if interactive {
            hits.push(
                RectF {
                    x: lbx,
                    y: row_y,
                    w: loc_w,
                    h: 30.0,
                },
                HoverTarget::WeatherLocateBtn,
            );
        }

        if app.weather_report.is_some() {
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
                app.hover == HoverTarget::WeatherClosePicker,
                app.pressed == HoverTarget::WeatherClosePicker,
            );
            if interactive {
                hits.push(
                    RectF {
                        x: cbx,
                        y: row_y,
                        w: 30.0,
                        h: 30.0,
                    },
                    HoverTarget::WeatherClosePicker,
                );
            }
        }

        let body_y = row_y + 40.0;
        let body_h = (wy + wh - 10.0 - body_y).max(40.0);
        if app.weather_places.is_empty() {
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
            let count = app.weather_places.len().min(3);
            let gap = 8.0;
            let card_w =
                ((ww - 2.0 * pad - gap * ((count - 1) as f32)) / (count as f32)).max(120.0);
            for (i, place) in app.weather_places.iter().take(3).enumerate() {
                let cx = wx + pad + (i as f32) * (card_w + gap);
                let hovered = app.hover == HoverTarget::WeatherPickPlace(i);
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
                        app.accent.with_alpha(0.55)
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
                        HoverTarget::WeatherPickPlace(i),
                    );
                }
            }
        }
        return;
    }

    let Some(rep) = app.weather_report.clone() else {
        return;
    };
    let (glow_a, glow_b) = weather_palette(&rep.icon, rep.is_day);

    let hero_w = 296.0_f32;
    surf.fill_rounded_rect_radial_glow(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        wx + hero_w * 0.78,
        wy + wh * 0.28,
        hero_w * 0.65,
        wh * 0.95,
        glow_a.with_alpha(0.24),
        Color::argb(0, 0, 0, 0),
    );
    surf.fill_rounded_rect_radial_glow(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        wx + hero_w * 0.18,
        wy + wh * 0.85,
        hero_w * 0.55,
        wh * 0.75,
        glow_b.with_alpha(0.14),
        Color::argb(0, 0, 0, 0),
    );
    surf.fill_rounded_rect(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        16.0,
        Color::argb(0x10, 0xFF, 0xFF, 0xFF),
    );
    surf.draw_rounded_rect(
        wx,
        wy,
        hero_w,
        wh,
        16.0,
        16.0,
        glow_a.lerp(TEXT_PRIMARY, 0.25).with_alpha(0.28),
        1.0,
        None,
    );

    let city_name = app
        .store
        .data
        .settings
        .weather_place
        .as_deref()
        .unwrap_or("Local");
    let city_w = (22.0
        + factories
            .measure_text(city_name, FontKind::Ui, 12.0, true, false, 170.0, false)
            .0
        + 22.0)
        .min(200.0);
    let city_hovered = app.hover == HoverTarget::WeatherCityBtn;
    surf.fill_rounded_rect(
        wx + 12.0,
        wy + 10.0,
        city_w,
        22.0,
        11.0,
        11.0,
        if city_hovered {
            HOVER_FILL
        } else {
            Color::argb(0x18, 0x00, 0x00, 0x00)
        },
    );
    surf.draw_text(
        factories,
        "\u{E707}",
        wx + 18.0,
        wy + 10.0,
        12.0,
        22.0,
        FontKind::Icon,
        10.5,
        false,
        glow_a,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    surf.draw_text(
        factories,
        city_name,
        wx + 32.0,
        wy + 10.0,
        (city_w - 42.0).max(20.0),
        22.0,
        FontKind::Ui,
        11.5,
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
        "\u{E721}",
        wx + 12.0 + city_w - 18.0,
        wy + 10.0,
        12.0,
        22.0,
        FontKind::Icon,
        9.5,
        false,
        if city_hovered {
            TEXT_PRIMARY
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
    if interactive {
        hits.push(
            RectF {
                x: wx + 12.0,
                y: wy + 10.0,
                w: city_w,
                h: 22.0,
            },
            HoverTarget::WeatherCityBtn,
        );
    }

    let hl = match (rep.high, rep.low) {
        (Some(h), Some(l)) => format!(
            "{} · H:{}° L:{}°",
            rep.description,
            h.round() as i64,
            l.round() as i64
        ),
        _ => rep.description.clone(),
    };
    surf.draw_text(
        factories,
        &hl,
        wx + 14.0,
        wy + 36.0,
        hero_w - 84.0,
        18.0,
        FontKind::Ui,
        11.5,
        false,
        TEXT_PRIMARY.with_alpha(0.82),
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );

    draw_weather_scene(
        factories,
        surf,
        &rep.icon,
        rep.is_day,
        app.weather_anim_phase as f32,
        wx + hero_w - 68.0,
        wy + 10.0,
        56.0,
    );

    surf.draw_text(
        factories,
        &rep.temperature_text,
        wx + 14.0,
        wy + wh - 52.0,
        104.0,
        44.0,
        FontKind::Display,
        36.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );

    let mut chips: Vec<String> = Vec::new();
    if let Some(fl) = rep.feels_like {
        chips.push(fmt1(s.weather_feels, &(fl.round() as i64).to_string()));
    }
    if let Some(w) = rep.wind_kmh {
        chips.push(fmt1(s.weather_wind, &(w.round() as i64).to_string()));
    }
    if let Some(hum) = rep.humidity {
        chips.push(format!("{hum}%"));
    }
    let mut chip_x = wx + hero_w - 12.0;
    let chip_y = wy + wh - 32.0;
    for chip in chips.iter().rev().take(3) {
        let cw = (factories
            .measure_text(chip, FontKind::Ui, 10.5, false, true, 100.0, false)
            .0
            + 14.0)
            .max(36.0);
        chip_x -= cw;
        if chip_x < wx + 108.0 {
            break;
        }
        surf.fill_rounded_rect(
            chip_x,
            chip_y,
            cw,
            20.0,
            10.0,
            10.0,
            Color::argb(0x38, 0x00, 0x00, 0x00),
        );
        surf.draw_rounded_rect(
            chip_x,
            chip_y,
            cw,
            20.0,
            10.0,
            10.0,
            Color::argb(0x1A, 0xFF, 0xFF, 0xFF),
            1.0,
            None,
        );
        surf.draw_text(
            factories,
            chip,
            chip_x,
            chip_y,
            cw,
            20.0,
            FontKind::Ui,
            10.5,
            false,
            TEXT_PRIMARY.with_alpha(0.88),
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
        chip_x -= 5.0;
    }

    let cx = wx + hero_w + 12.0;
    let cw = (ww - hero_w - 12.0).max(240.0);
    surf.fill_rounded_rect(
        cx,
        wy,
        cw,
        wh,
        16.0,
        16.0,
        Color::argb(0x0E, 0xFF, 0xFF, 0xFF),
    );
    surf.draw_rounded_rect(
        cx,
        wy,
        cw,
        wh,
        16.0,
        16.0,
        Color::argb(0x1A, 0xFF, 0xFF, 0xFF),
        1.0,
        None,
    );

    surf.draw_text(
        factories,
        s.weather_hourly,
        cx + 14.0,
        wy + 8.0,
        160.0,
        18.0,
        FontKind::Ui,
        11.0,
        true,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );

    let peak_rain = rep
        .hourly
        .iter()
        .max_by_key(|h| h.rain_chance)
        .filter(|h| h.rain_chance >= 25);
    let (rain_lbl, rain_col) = if let Some(pk) = peak_rain {
        (
            fmt2(s.weather_rain_peak, &pk.rain_chance.to_string(), &pk.label),
            WATER_BRUSH,
        )
    } else {
        (s.weather_dry.to_string(), TEXT_SECONDARY)
    };
    surf.draw_text(
        factories,
        &rain_lbl,
        cx + cw - 190.0,
        wy + 8.0,
        176.0,
        18.0,
        FontKind::Ui,
        11.0,
        peak_rain.is_some(),
        rain_col,
        HAlign::Right,
        VAlign::Center,
        true,
        false,
        true,
        false,
    );

    let pts_data: Vec<(String, f64, i64)> = if rep.hourly.len() >= 2 {
        rep.hourly
            .iter()
            .take(6)
            .map(|h| (h.label.clone(), h.temp, h.rain_chance))
            .collect()
    } else {
        let base = rep.temperature;
        vec![
            (s.weather_now.to_string(), base, rep.rain_chance),
            ("15:00".to_string(), base + 1.5, rep.rain_chance),
            (
                "18:00".to_string(),
                base - 0.5,
                (rep.rain_chance + 20).min(90),
            ),
            ("21:00".to_string(), base - 2.5, rep.rain_chance),
            ("00:00".to_string(), base - 3.5, 10),
            ("03:00".to_string(), base - 4.0, 10),
        ]
    };

    let n = pts_data.len();
    let min_t = pts_data.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
    let max_t = pts_data
        .iter()
        .map(|x| x.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (max_t - min_t).max(3.0);

    let plot_x = cx + 28.0;
    let plot_w = (cw - 56.0).max(120.0);
    let plot_top = wy + 42.0;
    let plot_bot = wy + 82.0;
    let baseline_y = wy + 90.0;

    let mut curve_pts = Vec::with_capacity(n);
    for (i, (_, temp, _)) in pts_data.iter().enumerate() {
        let frac_x = if n > 1 {
            (i as f32) / ((n - 1) as f32)
        } else {
            0.5
        };
        let norm_y = ((*temp - min_t) / span).clamp(0.0, 1.0) as f32;
        let px_i = plot_x + frac_x * plot_w;
        let py_i = plot_bot - norm_y * (plot_bot - plot_top);
        curve_pts.push((px_i, py_i));
    }

    surf.draw_smooth_curve_with_fill(
        factories,
        &curve_pts,
        baseline_y,
        glow_a,
        2.2,
        glow_a.with_alpha(0.30),
        glow_b.with_alpha(0.0),
    );

    for (i, ((px_i, py_i), (lbl, temp, rain))) in curve_pts.iter().zip(pts_data.iter()).enumerate()
    {
        if i == 0 {
            surf.fill_ellipse(*px_i, *py_i, 5.5, 5.5, glow_a.with_alpha(0.35));
            surf.fill_ellipse(*px_i, *py_i, 3.5, 3.5, TEXT_PRIMARY);
        } else {
            surf.fill_ellipse(
                *px_i,
                *py_i,
                3.0,
                3.0,
                glow_a.lerp(glow_b, (i as f32) / 5.0),
            );
        }
        let t_lbl = format!("{}°", temp.round() as i64);
        surf.draw_text(
            factories,
            &t_lbl,
            *px_i - 20.0,
            *py_i - 18.0,
            40.0,
            14.0,
            FontKind::Ui,
            11.0,
            i == 0,
            if i == 0 {
                TEXT_PRIMARY
            } else {
                TEXT_PRIMARY.with_alpha(0.85)
            },
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );

        surf.draw_text(
            factories,
            lbl,
            *px_i - 24.0,
            wy + 93.0,
            48.0,
            14.0,
            FontKind::Ui,
            10.0,
            i == 0,
            if i == 0 { TEXT_PRIMARY } else { TEXT_SECONDARY },
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );

        let r_col = if *rain >= 25 {
            WATER_BRUSH
        } else {
            TEXT_SECONDARY.with_alpha(0.65)
        };
        let r_lbl = format!("{rain}%");
        surf.draw_text(
            factories,
            &r_lbl,
            *px_i - 20.0,
            wy + 108.0,
            40.0,
            13.0,
            FontKind::Ui,
            9.5,
            *rain >= 25,
            r_col,
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    }
}
