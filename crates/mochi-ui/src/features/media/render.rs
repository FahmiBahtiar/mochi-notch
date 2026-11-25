use chrono::{Local, Timelike};
use windows_numerics::{Matrix3x2, Vector2};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_companion_sprite;
use crate::features::weather::render::draw_weather_sprite;
use crate::features::widgets::render::{draw_circle_button, draw_icon_button, rounded_rect_geom};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    DIVIDER, INVERSE_FILL, INVERSE_FILL_PRESSED, INVERSE_TEXT, LYRIC_DIM, OVERLAY_BUTTON_FILL,
    SURFACE, TEXT_PLACEHOLDER, TEXT_PRIMARY, TEXT_SECONDARY, TRACK,
};

#[expect(clippy::too_many_arguments, reason = "renders media page and lyrics")]
pub(crate) fn draw_media_page(
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
    if let Some(ref media) = app.media_state.clone() {
        let has_lyrics = app.lyrics.is_some();
        let mx = px + 24.0;
        let mw = if has_lyrics {
            416.0_f32.min(pw - 48.0)
        } else {
            pw - 48.0
        };
        let art_x = mx;
        let art_y = py + (136.0 - 108.0) / 2.0;
        surf.fill_rounded_rect(art_x, art_y, 108.0, 108.0, 16.0, 16.0, SURFACE);
        if let Some(ref frame) = app.artwork_frame.clone()
            && let Some(bmp) = surf.get_or_upload_image(app.artwork_id, frame)
            && let Some(rgeom) = rounded_rect_geom(factories, art_x, art_y, 108.0, 108.0, 16.0)
        {
            surf.push_layer(art_x, art_y, 108.0, 108.0, 1.0, Some(&rgeom), None);
            surf.draw_bitmap(&bmp, art_x, art_y, 108.0, 108.0, 1.0);
            surf.pop_layer();
        } else {
            surf.draw_text(
                factories,
                "\u{E8D6}",
                art_x,
                art_y,
                108.0,
                108.0,
                FontKind::Icon,
                32.0,
                false,
                TEXT_PLACEHOLDER,
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
        }

        let over_art = RectF {
            x: art_x,
            y: art_y,
            w: 108.0,
            h: 108.0,
        }
        .contains(app.cursor_dip.0 as f32, app.cursor_dip.1 as f32);
        if over_art && interactive {
            let mut bx = art_x + 108.0 - 6.0 - 26.0;
            let by = art_y + 6.0;
            if !media.source_app.is_empty() {
                draw_circle_button(
                    factories,
                    surf,
                    bx,
                    by,
                    26.0,
                    "\u{E711}",
                    10.0,
                    OVERLAY_BUTTON_FILL,
                    TEXT_PRIMARY,
                    app.hover == HoverTarget::RemovePlayerBtn,
                    app.pressed == HoverTarget::RemovePlayerBtn,
                );
                hits.push(
                    RectF {
                        x: bx,
                        y: by,
                        w: 26.0,
                        h: 26.0,
                    },
                    HoverTarget::RemovePlayerBtn,
                );
                bx -= 30.0;
            }
            if app.next_source.is_some() {
                draw_circle_button(
                    factories,
                    surf,
                    bx,
                    by,
                    26.0,
                    "\u{E8AB}",
                    11.0,
                    OVERLAY_BUTTON_FILL,
                    TEXT_PRIMARY,
                    app.hover == HoverTarget::SwitchPlayerBtn,
                    app.pressed == HoverTarget::SwitchPlayerBtn,
                );
                hits.push(
                    RectF {
                        x: bx,
                        y: by,
                        w: 26.0,
                        h: 26.0,
                    },
                    HoverTarget::SwitchPlayerBtn,
                );
            }
        }

        let rx = art_x + 108.0 + 16.0;
        let rw = mw - 108.0 - 16.0;
        let ry = py + 10.0;
        surf.push_clip(rx, ry, rw, 24.0);
        surf.draw_text(
            factories,
            &media.title,
            rx - app.title_marquee_offset as f32,
            ry,
            rw,
            24.0,
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
        surf.pop_clip();
        if !media.artist.is_empty() {
            surf.draw_text(
                factories,
                &media.artist,
                rx,
                ry + 23.0,
                rw,
                20.0,
                FontKind::Ui,
                13.0,
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

        let prog_y = py + 136.0 - 14.0 - 38.0 - 2.0 - 14.0;
        if media.duration_secs > 0.0 {
            let pos = if app.seeking {
                app.seek_fraction * media.duration_secs
            } else {
                media.current_position_secs()
            };
            let frac = (pos / media.duration_secs).clamp(0.0, 1.0) as f32;
            surf.draw_text(
                factories,
                &format_media_time(pos),
                rx,
                prog_y,
                40.0,
                14.0,
                FontKind::Ui,
                11.0,
                false,
                TEXT_SECONDARY,
                HAlign::Left,
                VAlign::Center,
                true,
                false,
                false,
                false,
            );
            surf.draw_text(
                factories,
                &format_media_time(media.duration_secs),
                rx + rw - 40.0,
                prog_y,
                40.0,
                14.0,
                FontKind::Ui,
                11.0,
                false,
                TEXT_SECONDARY,
                HAlign::Right,
                VAlign::Center,
                true,
                false,
                false,
                false,
            );
            let tx = rx + 40.0;
            let tw = (rw - 80.0).max(10.0);
            let track_rect = RectF {
                x: tx,
                y: prog_y,
                w: tw,
                h: 14.0,
            };
            let active_track = app.seeking || app.hover == HoverTarget::ProgressTrack;
            let bar_h = if active_track { 6.0 } else { 4.0 };
            let bar_r = bar_h / 2.0;
            let bar_y = prog_y + (14.0 - bar_h) / 2.0;
            surf.fill_rounded_rect(tx, bar_y, tw, bar_h, bar_r, bar_r, TRACK);
            surf.fill_rounded_rect(tx, bar_y, tw * frac, bar_h, bar_r, bar_r, app.accent);
            if active_track {
                surf.fill_ellipse(tx + tw * frac, prog_y + 7.0, 6.0, 6.0, TEXT_PRIMARY);
            }
            if interactive {
                hits.progress_rect = Some(track_rect);
                hits.push(track_rect, HoverTarget::ProgressTrack);
            }
        }

        let ctrl_y = py + 136.0 - 14.0 - 38.0;
        let ctrl_cx = rx + rw / 2.0;
        let prev_x = ctrl_cx - 19.0 - 12.0 - 36.0;
        let play_x = ctrl_cx - 19.0;
        let next_x = ctrl_cx + 19.0 + 12.0;
        draw_icon_button(
            factories,
            surf,
            prev_x,
            ctrl_y + 1.0,
            36.0,
            "\u{F8AC}",
            16.0,
            TEXT_PRIMARY,
            app.hover == HoverTarget::PrevBtn,
            app.pressed == HoverTarget::PrevBtn,
        );
        let play_glyph = if media.is_playing {
            "\u{F8AE}"
        } else {
            "\u{F5B0}"
        };
        let play_bg = if app.pressed == HoverTarget::PlayPauseBtn {
            INVERSE_FILL_PRESSED
        } else {
            INVERSE_FILL
        };
        draw_circle_button(
            factories,
            surf,
            play_x,
            ctrl_y,
            38.0,
            play_glyph,
            15.0,
            play_bg,
            INVERSE_TEXT,
            app.hover == HoverTarget::PlayPauseBtn,
            false,
        );
        draw_icon_button(
            factories,
            surf,
            next_x,
            ctrl_y + 1.0,
            36.0,
            "\u{F8AD}",
            16.0,
            TEXT_PRIMARY,
            app.hover == HoverTarget::NextBtn,
            app.pressed == HoverTarget::NextBtn,
        );
        if interactive {
            hits.push(
                RectF {
                    x: prev_x,
                    y: ctrl_y + 1.0,
                    w: 36.0,
                    h: 36.0,
                },
                HoverTarget::PrevBtn,
            );
            hits.push(
                RectF {
                    x: play_x,
                    y: ctrl_y,
                    w: 38.0,
                    h: 38.0,
                },
                HoverTarget::PlayPauseBtn,
            );
            hits.push(
                RectF {
                    x: next_x,
                    y: ctrl_y + 1.0,
                    w: 36.0,
                    h: 36.0,
                },
                HoverTarget::NextBtn,
            );
        }

        if let Some(ref lyrics) = app.lyrics.clone() {
            let div_x = mx + mw + 16.0;
            let div_y = py + 8.0;
            let div_h = (ph - 24.0).max(20.0);
            surf.fill_rect(div_x, div_y, 1.0, div_h, DIVIDER);
            let lx = div_x + 17.0;
            let ly = div_y;
            let lw = (px + pw - 24.0 - lx).max(80.0);
            let lh = div_h;

            let hkey = (app.lyrics_track.clone(), lw.to_bits());
            if app.lyric_heights_key != hkey || app.lyric_line_heights.len() != lyrics.lines.len() {
                app.lyric_line_heights.clear();
                app.lyric_line_heights.reserve(lyrics.lines.len());
                let mut y_acc = 0.0_f32;
                for line in &lyrics.lines {
                    let txt = if line.text.trim().is_empty() {
                        "♪"
                    } else {
                        &line.text
                    };
                    let (_, th) = factories.measure_text(
                        txt,
                        FontKind::Ui,
                        14.0,
                        true,
                        false,
                        lw - 10.0,
                        true,
                    );
                    let row_h = th + 6.0;
                    app.lyric_line_heights.push((y_acc, row_h));
                    y_acc += row_h;
                }
                app.lyric_heights_key = hkey;
            }
            let active_idx = app.lyric_index.max(0) as usize;
            let target_center = app
                .lyric_line_heights
                .get(active_idx)
                .map_or(0.0, |&(oy, rh)| oy + rh / 2.0);
            let desired_scroll = f64::from(lh / 2.0 - target_center);
            if (desired_scroll - app.lyric_scroll_to).abs() > 0.5 {
                app.lyric_scroll_from = app.lyric_scroll_y;
                app.lyric_scroll_to = desired_scroll;
                if app.reduce_motion || app.shots_dir.is_some() {
                    app.lyric_scroll_y = desired_scroll;
                    app.lyric_scroll_t = 1.0;
                } else {
                    app.lyric_scroll_t = 0.0;
                }
            }

            let mask = surf.create_linear_mask(
                lx,
                ly,
                lx,
                ly + lh,
                &[(0.0, 0.0), (0.28, 1.0), (0.72, 1.0), (1.0, 0.0)],
            );
            surf.push_clip(lx, ly, lw, lh);
            surf.push_layer(lx, ly, lw, lh, 1.0, None, mask.as_ref());
            let scroll_y = app.lyric_scroll_y as f32;
            for (i, line) in lyrics.lines.iter().enumerate() {
                let (oy, row_h) = app.lyric_line_heights[i];
                let draw_y = ly + scroll_y + oy;
                if draw_y + row_h < ly || draw_y > ly + lh {
                    continue;
                }
                let (bright, scale) = app.lyric_line_anim.get(i).copied().unwrap_or((0.0, 1.0));
                let col = LYRIC_DIM.lerp(TEXT_PRIMARY, bright as f32);
                let txt = if line.text.trim().is_empty() {
                    "♪"
                } else {
                    &line.text
                };
                if (scale - 1.0).abs() > 0.001 {
                    surf.set_transform(Matrix3x2::scale_around(
                        scale as f32,
                        scale as f32,
                        Vector2::new(lx, draw_y + row_h / 2.0),
                    ));
                }
                surf.draw_text(
                    factories,
                    txt,
                    lx,
                    draw_y + 3.0,
                    lw - 10.0,
                    row_h - 6.0,
                    FontKind::Ui,
                    14.0,
                    true,
                    col,
                    HAlign::Left,
                    VAlign::Top,
                    false,
                    true,
                    false,
                    false,
                );
                surf.set_transform(Matrix3x2::identity());
                if interactive && draw_y >= ly && draw_y + row_h <= ly + lh {
                    hits.push(
                        RectF {
                            x: lx,
                            y: draw_y,
                            w: lw,
                            h: row_h,
                        },
                        HoverTarget::LyricLine(i),
                    );
                }
            }
            surf.pop_layer();
            surf.pop_clip();
        }
    } else {
        let now = Local::now();
        let time_s = format!("{:02}:{:02}", now.hour(), now.minute());
        let tw = factories
            .measure_text(&time_s, FontKind::Display, 36.0, true, true, 200.0, false)
            .0;
        let companion_s = 24.0 * 2.8;
        let row_w = tw + 14.0 + companion_s;
        let has_weather = app.weather_report.is_some();
        let block_h = if has_weather { 96.0 } else { 70.0 };
        let by = py + (ph - 16.0 - block_h) / 2.0;
        let rx = px + (pw - row_w) / 2.0;
        surf.draw_text(
            factories,
            &time_s,
            rx,
            by,
            tw + 4.0,
            48.0,
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
        draw_companion_sprite(
            factories,
            surf,
            &app.idle_companion,
            rx + tw + 14.0,
            by + 48.0 - 6.0 - companion_s,
            companion_s,
        );
        surf.draw_text(
            factories,
            app.lang.strings().media_nothing_playing,
            px + 24.0,
            by + 48.0,
            pw - 48.0,
            20.0,
            FontKind::Ui,
            13.0,
            false,
            TEXT_SECONDARY,
            HAlign::Center,
            VAlign::Top,
            false,
            false,
            true,
            false,
        );
        if let Some(ref rep) = app.weather_report {
            let mut line = format!("{} · {}", rep.temperature_text, rep.description);
            if let (Some(hi), Some(lo)) = (rep.high, rep.low) {
                line.push_str(&format!(" · {:.0}° / {:.0}°", hi.round(), lo.round()));
            }
            let lw = factories
                .measure_text(&line, FontKind::Ui, 12.0, false, false, pw - 80.0, false)
                .0;
            let wx = px + (pw - (15.0 + 8.0 + lw)) / 2.0;
            let wy = by + 76.0;
            draw_weather_sprite(surf, &rep.icon, wx, wy + 2.0, 1.5);
            surf.draw_text(
                factories,
                &line,
                wx + 23.0,
                wy,
                lw + 4.0,
                18.0,
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
        }
    }
}

fn format_media_time(secs: f64) -> String {
    let total = secs.max(0.0).floor() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
