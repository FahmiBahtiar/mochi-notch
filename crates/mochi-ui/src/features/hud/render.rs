use chrono::Timelike;

use mochi_winrt::mochi_presence::port::AwayReport;
use mochi_winrt::paths::{Glyphs, HudKind, HudMessage};
use mochi_winrt::strings::{fmt1, fmt2};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_raven_sprite;
use crate::features::companion::state::RavenMode;
use crate::features::weather::render::draw_weather_sprite;
use crate::features::widgets::render::{draw_icon_view, rounded_rect_geom};
use crate::state::{NEWS_WIDTH, NotchApp};
use crate::theme::{
    CAMERA_BRUSH, MICROPHONE_BRUSH, TEXT_PRIMARY, TEXT_SECONDARY, TRACK, WARNING_BRUSH,
};

pub(crate) fn draw_compact_view(
    app: &mut NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    notch_x: f32,
    w: f32,
    h: f32,
) {
    let dots = if !app.privacy_shown() || app.ambient_shown {
        0
    } else {
        usize::from(!app.camera_apps.is_empty()) + usize::from(!app.mic_apps.is_empty())
    };
    let privacy_room = if dots == 0 {
        0.0
    } else {
        (dots as f32) * 6.0 + ((dots - 1) as f32) * 3.0 + 8.0
    };
    let margin_x = 12.0 + privacy_room;
    let inner_x = notch_x + margin_x;
    let inner_w = (w - 2.0 * margin_x).max(10.0);

    let has_media = app.media_state.is_some();
    let has_art = has_media && app.artwork_frame.is_some();
    let show_clock_raven = app.clock_raven.mode != RavenMode::Still
        && !has_media
        && !app.focus.is_active()
        && app.upcoming_text.is_none();

    let left_w: f32 = if has_art {
        20.0
    } else if show_clock_raven {
        24.0
    } else {
        0.0
    };

    let show_timer = app.focus.is_active() && !has_media;
    let show_weather = app.weather_report.is_some()
        && !has_media
        && !app.focus.is_active()
        && app.upcoming_text.is_none();
    let show_music_raven = app.music_raven.mode != RavenMode::Still && has_media;
    let show_eq = has_media && !show_music_raven;

    let timer_str = if show_timer {
        let rem = app.focus.remaining();
        format!("{:02}:{:02}", rem.as_secs() / 60, rem.as_secs() % 60)
    } else {
        String::new()
    };
    let weather_str = if show_weather {
        app.weather_report
            .as_ref()
            .map(|r| r.temperature_text.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let right_w = if show_timer {
        let tw = factories
            .measure_text(&timer_str, FontKind::Ui, 12.0, true, true, 100.0, false)
            .0;
        12.0 + 5.0 + tw
    } else if show_weather {
        let ww = factories
            .measure_text(&weather_str, FontKind::Ui, 12.0, true, true, 100.0, false)
            .0;
        10.0 + 6.0 + ww
    } else if show_music_raven {
        24.0
    } else if show_eq {
        20.0
    } else {
        0.0
    };

    let side = left_w.max(right_w);
    let gap = if side > 0.0 { 6.0 } else { 0.0 };
    let show_left = left_w > 0.0 && inner_w >= left_w + 16.0;
    let show_right =
        right_w > 0.0 && inner_w >= (if show_left { left_w + 12.0 } else { 16.0 }) + right_w;

    if show_left
        && has_art
        && let Some(ref frame) = app.artwork_frame.clone()
    {
        if let Some(bmp) = surf.get_or_upload_image(app.artwork_id, frame) {
            let ay = (h - 20.0) / 2.0;
            if let Some(rgeom) = rounded_rect_geom(factories, inner_x, ay, 20.0, 20.0, 5.0) {
                surf.push_layer(inner_x, ay, 20.0, 20.0, 1.0, Some(&rgeom), None);
                surf.draw_bitmap(&bmp, inner_x, ay, 20.0, 20.0, 1.0);
                surf.pop_layer();
            }
        }
    } else if show_left && show_clock_raven {
        let anim = app.clock_raven.clone();
        draw_raven_sprite(factories, surf, &anim, inner_x, (h - 20.0) / 2.0, 20.0);
    }

    let (text_x, text_w) = if inner_w >= 2.0 * (side + gap) + 24.0 {
        (
            inner_x + side + gap,
            (inner_w - 2.0 * (side + gap)).max(10.0),
        )
    } else {
        let used_l = if show_left { left_w + 6.0 } else { 0.0 };
        let used_r = if show_right { right_w + 6.0 } else { 0.0 };
        (inner_x + used_l, (inner_w - used_l - used_r).max(10.0))
    };
    let (swap_y, swap_op) = if app.compact_swap_phase > 0.0 && app.compact_swap_phase < 1.0 {
        let t = app.compact_swap_phase as f32;
        let ease = t * t * t;
        (-10.0 * ease, 1.0 - ease)
    } else if app.compact_swap_phase >= 1.0 {
        let t = (app.compact_swap_phase - 1.0) as f32;
        let ease = 1.0 - (1.0 - t).powi(3);
        (10.0 * (1.0 - ease), ease)
    } else {
        (0.0, 1.0)
    };
    surf.push_clip(text_x, 0.0, text_w, h);
    surf.draw_text(
        factories,
        &app.compact_text,
        text_x - app.compact_marquee_offset as f32,
        swap_y,
        text_w,
        h,
        FontKind::Ui,
        13.0,
        true,
        TEXT_PRIMARY.with_alpha(swap_op),
        HAlign::Center,
        VAlign::Center,
        true,
        false,
        true,
        false,
    );
    surf.pop_clip();

    let rx = inner_x + inner_w - right_w;
    if show_right && show_timer {
        let op = if app.focus.is_paused() { 0.55 } else { 1.0 };
        let glyph = if app.focus.is_paused() {
            "\u{F8AE}"
        } else {
            "\u{E916}"
        };
        surf.draw_text(
            factories,
            glyph,
            rx,
            0.0,
            14.0,
            h,
            FontKind::Icon,
            12.0,
            false,
            TEXT_PRIMARY.with_alpha(op),
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
        surf.draw_text(
            factories,
            &timer_str,
            rx + 17.0,
            0.0,
            right_w - 17.0,
            h,
            FontKind::Ui,
            12.0,
            true,
            TEXT_PRIMARY.with_alpha(op),
            HAlign::Left,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    } else if show_right
        && show_weather
        && let Some(ref rep) = app.weather_report
    {
        draw_weather_sprite(surf, &rep.icon, rx, (h - 9.0) / 2.0, 1.0);
        surf.draw_text(
            factories,
            &weather_str,
            rx + 16.0,
            0.0,
            right_w - 16.0,
            h,
            FontKind::Ui,
            12.0,
            true,
            TEXT_PRIMARY,
            HAlign::Left,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    } else if show_right && show_music_raven {
        let anim = app.music_raven.clone();
        draw_raven_sprite(factories, surf, &anim, rx, (h - 24.0) / 2.0, 24.0);
    } else if show_right && show_eq {
        let cy = h * 0.5;
        let bw = 2.4_f32;
        let rad = bw * 0.5;
        for (i, &level) in app.equalizer_bars.iter().enumerate() {
            let bh = (14.0 * (level as f32)).max(bw);
            let bx = rx + (i as f32) * 4.2;
            let by = cy - bh * 0.5;
            surf.fill_rounded_rect(bx, by, bw, bh, rad, rad, app.accent);
        }
    }
}

pub(crate) fn draw_hud_view(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    msg: &HudMessage,
    notch_x: f32,
    w: f32,
    h: f32,
) {
    let caw = msg.important && !msg.celebrate && msg.kind == HudKind::Reminder;
    let raven = msg.celebrate || caw;
    let icon_w = if raven { 26.4 } else { 15.0 };
    let dots = if !app.privacy_shown() || app.ambient_shown {
        0
    } else {
        usize::from(!app.camera_apps.is_empty()) + usize::from(!app.mic_apps.is_empty())
    };
    let privacy_room = if dots == 0 {
        0.0
    } else {
        (dots as f32) * 6.0 + ((dots - 1) as f32) * 3.0 + 14.0
    };
    let avail_x = notch_x + privacy_room;
    let avail_w = (w - 2.0 * privacy_room).max(40.0);
    let bar_w = if msg.level.is_some() {
        14.0 + 96.0
    } else {
        0.0
    };
    let val_w = if let Some(ref v) = msg.value {
        10.0 + factories
            .measure_text(v, FontKind::Ui, 12.0, false, true, 200.0, false)
            .0
            .max(22.0)
    } else {
        0.0
    };
    let max_text_w = (avail_w - icon_w - 10.0 - bar_w - val_w - 16.0).clamp(40.0, 320.0);
    let text_w = factories
        .measure_text(
            &msg.text,
            FontKind::Ui,
            13.0,
            true,
            false,
            max_text_w,
            false,
        )
        .0
        .min(max_text_w);
    let total_w = icon_w + 10.0 + text_w + bar_w + val_w;
    let mut x = avail_x + (avail_w - total_w) / 2.0;

    let fg = if msg.warning {
        WARNING_BRUSH
    } else if msg.kind == HudKind::Privacy {
        if msg.glyph == Glyphs::CAMERA {
            CAMERA_BRUSH
        } else {
            MICROPHONE_BRUSH
        }
    } else {
        TEXT_PRIMARY
    };

    if raven {
        draw_raven_sprite(factories, surf, &app.hud_raven, x, (h - 26.4) / 2.0, 26.4);
    } else {
        draw_icon_view(
            factories,
            surf,
            &msg.glyph,
            msg.badge.as_deref(),
            x,
            (h - 15.0) / 2.0,
            15.0,
            fg,
        );
    }
    x += icon_w + 10.0;

    surf.draw_text(
        factories,
        &msg.text,
        x,
        0.0,
        text_w + 2.0,
        h,
        FontKind::Ui,
        13.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    x += text_w;

    if msg.level.is_some() {
        x += 14.0;
        let by = (h - 4.0) / 2.0;
        surf.fill_rounded_rect(x, by, 96.0, 4.0, 2.0, 2.0, TRACK);
        surf.fill_rounded_rect(x, by, app.hud_bar_fill as f32, 4.0, 2.0, 2.0, TEXT_PRIMARY);
        x += 96.0;
    }

    if let Some(ref v) = msg.value {
        x += 10.0;
        surf.draw_text(
            factories,
            v,
            x,
            0.0,
            val_w - 10.0,
            h,
            FontKind::Ui,
            12.0,
            false,
            TEXT_SECONDARY,
            HAlign::Right,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    }
}

pub(crate) fn draw_news_view(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    report: &AwayReport,
    notch_x: f32,
    w: f32,
    _h: f32,
) {
    let x = notch_x + 20.0;
    let y = 12.0;
    let ease = 1.0 - (1.0 - app.news_flight_t as f32).powi(3);
    let shift_x = (NEWS_WIDTH as f32) * (1.0 - ease);
    let raven_s = 24.0 * 2.4;
    draw_raven_sprite(
        factories,
        surf,
        &app.news_raven,
        x + shift_x,
        y + 2.0,
        raven_s,
    );

    let body_op = ((app.news_flight_t as f32 - 0.6) / 0.4).clamp(0.0, 1.0);
    if body_op <= 0.001 {
        return;
    }
    let bx = x + raven_s + 16.0;
    let bw = (w - 40.0 - raven_s - 16.0).max(40.0);
    let mins = (report.duration.as_secs() / 60).max(1);
    let dur_str = if mins >= 60 {
        format!("{}h{:02}", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    };
    let s = app.lang.strings();
    let title = if report.items.len() > 4 {
        fmt2(
            s.away_title_count,
            &dur_str,
            &report.items.len().to_string(),
        )
    } else {
        fmt1(s.away_title, &dur_str)
    };
    surf.draw_text(
        factories,
        &title,
        bx,
        y,
        bw,
        18.0,
        FontKind::Ui,
        12.0,
        false,
        TEXT_SECONDARY.with_alpha(body_op),
        HAlign::Left,
        VAlign::Top,
        false,
        false,
        true,
        false,
    );

    let start = report.items.len().saturating_sub(4);
    for (i, item) in report.items[start..].iter().enumerate() {
        let ry = y + 22.0 + (i as f32) * 22.0;
        draw_icon_view(
            factories,
            surf,
            &item.glyph,
            None,
            bx,
            ry + 5.0,
            12.0,
            TEXT_PRIMARY.with_alpha(body_op),
        );
        let time_s = format!("{:02}:{:02}", item.at.hour(), item.at.minute());
        let tw = factories
            .measure_text(&time_s, FontKind::Ui, 11.0, false, true, 60.0, false)
            .0;
        surf.draw_text(
            factories,
            &item.text,
            bx + 22.0,
            ry,
            (bw - 22.0 - tw - 10.0).max(20.0),
            22.0,
            FontKind::Ui,
            13.0,
            false,
            TEXT_PRIMARY.with_alpha(body_op),
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
        surf.draw_text(
            factories,
            &time_s,
            bx + bw - tw,
            ry,
            tw,
            22.0,
            FontKind::Ui,
            11.0,
            false,
            TEXT_SECONDARY.with_alpha(body_op),
            HAlign::Right,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    }
}
