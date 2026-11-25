use windows_numerics::{Matrix3x2, Vector2};

use mochi_winrt::strings::fmt1;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_raven_sprite;
use crate::features::companion::state::{RavenAnimator, RavenMode};
use crate::features::widgets::render::{
    draw_circle_button, draw_pill_button, draw_undo_toast, rounded_rect_geom,
};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    CARD_BG, Color, DecodedFrame, INVERSE_TEXT, OUTLINE, OVERLAY_BUTTON_FILL, PURE_EYE_AMBER,
    SUBTLE_FILL, SURFACE, TEXT_PRIMARY, TEXT_SECONDARY,
};

#[expect(clippy::too_many_arguments, reason = "renders shelf page")]
pub(crate) fn draw_shelf_page(
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
    let sx = px + 16.0;
    let sy = py + 8.0;
    let sw = pw - 32.0;
    let sh = ph - 24.0;
    let s = app.lang.strings();

    let drop_active = app.drop_hinting || app.drag_anim > 0.01;
    let da = if drop_active {
        app.drag_anim
            .max(if app.drop_hinting { 0.18 } else { 0.0 })
            .clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    if drop_active && da > 0.96 {
        draw_shelf_drop_overlay(app, factories, surf, sx, sy, sw, sh, da);
        return;
    }

    if app.shelf_items.is_empty() && !drop_active {
        surf.draw_rounded_rect(
            sx,
            sy,
            sw,
            sh,
            14.0,
            14.0,
            OUTLINE,
            1.5,
            Some(&factories.dashed_stroke),
        );
        let cy = sy + (sh - 74.0) / 2.0;
        let mut perch = RavenAnimator::default();
        perch.perch();
        let rx = sx + sw / 2.0 - 38.0;
        draw_raven_sprite(factories, surf, &perch, rx, cy, 48.0);
        surf.draw_text(
            factories,
            "\u{E7B8}",
            rx + 56.0,
            cy + 22.0,
            26.0,
            26.0,
            FontKind::Icon,
            22.0,
            false,
            TEXT_SECONDARY,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
        surf.draw_text(
            factories,
            s.shelf_drop_here,
            sx,
            cy + 54.0,
            sw,
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
        draw_shelf_drop_burst(app, factories, surf, sx, sy, sw, sh);
        return;
    }

    if !app.shelf_items.is_empty() {
        let grid_op = if drop_active {
            (1.0 - 0.85 * da).max(0.05)
        } else {
            1.0
        };
        if grid_op < 0.999 {
            surf.push_layer(sx, sy, sw, sh, grid_op, None, None);
        }

        let footer_h = 32.0;
        let grid_h = (sh - footer_h).max(40.0);
        let cols = 8usize;
        let col_w = sw / (cols as f32);
        let rows = app.shelf_items.len().div_ceil(cols);
        let total_h = (rows as f32) * 84.0;
        let max_scroll = (total_h - grid_h).max(0.0) as f64;
        app.shelf_scroll = app.shelf_scroll.clamp(0.0, max_scroll);
        let scroll = app.shelf_scroll as f32;
        if interactive && !app.drop_hinting {
            hits.scroll_rect = Some(RectF {
                x: sx,
                y: sy,
                w: sw,
                h: grid_h,
            });
        }

        surf.push_clip(sx, sy, sw, grid_h);
        for i in 0..app.shelf_items.len() {
            let col = i % cols;
            let row = i / cols;
            let tx = sx + (col as f32) * col_w + 1.0;
            let base_ty = sy - scroll + (row as f32) * 84.0 + 1.0;
            let tw = col_w - 2.0;
            let th = 82.0;
            if base_ty + th < sy || base_ty > sy + grid_h {
                continue;
            }
            let item = app.shelf_items[i].clone();
            let hovered = !app.drop_hinting
                && (app.hover == HoverTarget::ShelfTile(i)
                    || app.hover == HoverTarget::ShelfRemove(i));
            let flash = if i < app.shelf_flash_count && app.shelf_flash_t > 0.005 {
                app.shelf_flash_t as f32
            } else {
                0.0
            };
            let (ty, tile_scale) = if flash > 0.005 {
                let p = (1.0 - flash).clamp(0.0, 1.0);
                let ease_out = 1.0 - (1.0 - p).powi(3);
                let elastic = if p < 0.45 {
                    let u = p / 0.45;
                    0.45 + 0.70 * (1.0 - (1.0 - u).powi(2))
                } else {
                    let u = (p - 0.45) / 0.55;
                    1.15 - 0.15
                        * (1.0
                            - (1.0 - u).powi(2) * (1.0 - 0.35 * (u * std::f32::consts::PI).sin()))
                };
                (base_ty + 8.0 * (1.0 - ease_out), elastic)
            } else {
                (base_ty, 1.0)
            };
            if flash > 0.005 {
                surf.set_transform(Matrix3x2::scale_around(
                    tile_scale,
                    tile_scale,
                    Vector2::new(tx + tw * 0.5, ty + th * 0.5),
                ));
                let glow = app.accent.lerp(TEXT_PRIMARY, 0.35);
                surf.fill_rounded_rect(tx, ty, tw, th, 12.0, 12.0, glow.with_alpha(0.26 * flash));
                surf.draw_rounded_rect(
                    tx,
                    ty,
                    tw,
                    th,
                    12.0,
                    12.0,
                    glow.with_alpha(0.78 * flash),
                    1.6,
                    None,
                );
            } else if hovered {
                surf.fill_rounded_rect(tx, ty, tw, th, 12.0, 12.0, SUBTLE_FILL);
            }
            let op = if item.available { 1.0 } else { 0.45 };
            let ix = tx + (tw - 44.0) / 2.0;
            let iy = ty + 8.0;
            if let Some(ref dec) = item.decoded {
                if let Some(bmp) = surf.get_or_upload_image(item.image_id, dec)
                    && let Some(rgeom) = rounded_rect_geom(factories, ix, iy, 44.0, 40.0, 8.0)
                {
                    surf.push_layer(ix, iy, 44.0, 40.0, op, Some(&rgeom), None);
                    surf.draw_bitmap(&bmp, ix, iy, 44.0, 40.0, 1.0);
                    surf.pop_layer();
                }
            } else if let Some(ref icon) = item.icon {
                let frame = DecodedFrame {
                    width: icon.width,
                    height: icon.height,
                    bgra_premul: icon.bgra.clone(),
                };
                if let Some(bmp) = surf.get_or_upload_image(item.image_id, &frame) {
                    surf.draw_bitmap(&bmp, ix + 6.0, iy + 4.0, 32.0, 32.0, op);
                }
            }
            surf.draw_text(
                factories,
                &item.name,
                tx + 4.0,
                ty + 53.0,
                tw - 8.0,
                28.0,
                FontKind::Ui,
                11.0,
                false,
                TEXT_PRIMARY.with_alpha(op),
                HAlign::Center,
                VAlign::Top,
                false,
                true,
                true,
                false,
            );
            if flash > 0.005 {
                let p = (1.0 - flash).clamp(0.0, 1.0);
                if (0.12..=0.82).contains(&p) {
                    let sp = (p - 0.12) / 0.70;
                    let sx_glow = tx + tw * (-0.2 + 1.4 * sp);
                    surf.fill_rounded_rect_radial_glow(
                        tx,
                        ty,
                        tw,
                        th,
                        12.0,
                        sx_glow,
                        ty + th * 0.45,
                        26.0,
                        44.0,
                        TEXT_PRIMARY.with_alpha(0.32 * (1.0 - (sp * 2.0 - 1.0).abs())),
                        Color::argb(0, 0, 0, 0),
                    );
                }
                surf.set_transform(Matrix3x2::identity());
            }
            if hovered && interactive {
                let rx = tx + tw - 22.0;
                let ry = ty + 2.0;
                draw_circle_button(
                    factories,
                    surf,
                    rx,
                    ry,
                    20.0,
                    "\u{E711}",
                    8.0,
                    OVERLAY_BUTTON_FILL,
                    TEXT_PRIMARY,
                    app.hover == HoverTarget::ShelfRemove(i),
                    app.pressed == HoverTarget::ShelfRemove(i),
                );
                hits.push(
                    RectF {
                        x: tx,
                        y: ty,
                        w: tw,
                        h: th,
                    },
                    HoverTarget::ShelfTile(i),
                );
                hits.push(
                    RectF {
                        x: rx,
                        y: ry,
                        w: 20.0,
                        h: 20.0,
                    },
                    HoverTarget::ShelfRemove(i),
                );
            } else if interactive && !app.drop_hinting {
                hits.push(
                    RectF {
                        x: tx,
                        y: ty,
                        w: tw,
                        h: th,
                    },
                    HoverTarget::ShelfTile(i),
                );
            }
        }
        surf.pop_clip();

        if let Some(ref undo) = app.undo {
            draw_undo_toast(
                app,
                factories,
                surf,
                hits,
                sx,
                sy + grid_h - 36.0,
                sw,
                &undo.message,
                interactive && !app.drop_hinting,
            );
        }

        let fy = sy + grid_h + 4.0;
        let count = app.shelf_items.len();
        let count_txt = if count == 1 {
            s.shelf_count_one.to_string()
        } else {
            fmt1(s.shelf_count, &count.to_string())
        };
        surf.draw_text(
            factories,
            &count_txt,
            sx + 8.0,
            fy,
            sw - 100.0,
            28.0,
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
        let cw = (factories
            .measure_text(
                s.shelf_clear,
                FontKind::Ui,
                12.0,
                false,
                false,
                120.0,
                false,
            )
            .0
            + 24.0)
            .max(64.0);
        let cx = sx + sw - cw;
        draw_pill_button(
            factories,
            surf,
            cx,
            fy,
            cw,
            28.0,
            None,
            s.shelf_clear,
            false,
            app.hover == HoverTarget::ShelfClear,
            app.pressed == HoverTarget::ShelfClear,
        );
        if interactive && !app.drop_hinting {
            hits.push(
                RectF {
                    x: cx,
                    y: fy,
                    w: cw,
                    h: 28.0,
                },
                HoverTarget::ShelfClear,
            );
        }

        if grid_op < 0.999 {
            surf.pop_layer();
        }
    }

    if drop_active {
        draw_shelf_drop_overlay(app, factories, surf, sx, sy, sw, sh, da);
    }
    draw_shelf_drop_burst(app, factories, surf, sx, sy, sw, sh);
}

#[expect(
    clippy::too_many_arguments,
    reason = "renders animated shelf drop zone"
)]
fn draw_shelf_drop_overlay(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
    da: f32,
) {
    let s = app.lang.strings();
    let phase = app.drag_phase as f32;
    let glow_col = app.accent.lerp(TEXT_PRIMARY, 0.35);
    let spot_x = app.drag_cursor_dip.map_or(sx + sw * 0.5, |(x, _)| {
        (x as f32).clamp(sx + 20.0, sx + sw - 20.0)
    });
    let spot_y = app.drag_cursor_dip.map_or(sy + sh * 0.5, |(_, y)| {
        (y as f32).clamp(sy + 16.0, sy + sh - 16.0)
    });

    surf.push_layer(sx - 4.0, sy - 4.0, sw + 8.0, sh + 8.0, da, None, None);

    surf.fill_rounded_rect(sx, sy, sw, sh, 14.0, 14.0, glow_col.with_alpha(0.08));
    surf.fill_rounded_rect_radial_glow(
        sx,
        sy,
        sw,
        sh,
        14.0,
        spot_x,
        spot_y,
        220.0,
        120.0,
        glow_col.with_alpha(0.28),
        Color::argb(0, 0, 0, 0),
    );

    let dash_offset = -(phase * 3.2) % 9.0;
    let dash_style = factories.create_dashed_stroke(dash_offset);
    surf.draw_rounded_rect(
        sx,
        sy,
        sw,
        sh,
        14.0,
        14.0,
        glow_col.lerp(TEXT_PRIMARY, 0.35).with_alpha(0.88),
        2.0,
        dash_style.as_ref().or(Some(&factories.dashed_stroke)),
    );

    let inset = 8.0 + 2.5 * (phase * 1.5).sin();
    let arm = 11.0_f32;
    let bc = TEXT_PRIMARY.with_alpha(0.72);
    let corners = [
        (sx + inset, sy + inset, 1.0_f32, 1.0_f32),
        (sx + sw - inset, sy + inset, -1.0, 1.0),
        (sx + inset, sy + sh - inset, 1.0, -1.0),
        (sx + sw - inset, sy + sh - inset, -1.0, -1.0),
    ];
    for (cx, cy, dx, dy) in corners {
        surf.draw_line(
            cx,
            cy,
            cx + dx * arm,
            cy,
            bc,
            2.0,
            Some(&factories.round_stroke),
        );
        surf.draw_line(
            cx,
            cy,
            cx,
            cy + dy * arm,
            bc,
            2.0,
            Some(&factories.round_stroke),
        );
    }

    let raw_sub = if app.drag_file_count <= 1 {
        s.shelf_count_one.to_string()
    } else {
        fmt1(s.shelf_count, &app.drag_file_count.to_string())
    };
    let sub_txt = raw_sub.split(" · ").next().unwrap_or(&raw_sub).to_string();
    let title_w = factories
        .measure_text(
            s.shelf_drop_to_keep,
            FontKind::Ui,
            15.0,
            true,
            false,
            260.0,
            false,
        )
        .0;
    let sub_w = factories
        .measure_text(&sub_txt, FontKind::Ui, 12.0, false, false, 260.0, false)
        .0;
    let text_w = title_w.max(sub_w).clamp(110.0, 260.0);
    let hero_w = 50.0 + 18.0 + 56.0 + 18.0 + text_w;
    let hero_x = sx + (sw - hero_w) * 0.5;
    let mid_y = sy + sh * 0.5;

    let mut drop_anim = app.drop_raven.clone();
    if drop_anim.mode == RavenMode::Still {
        drop_anim.perch();
    }
    drop_anim.eye_scale_x = drop_anim.eye_scale_x.max(1.10);
    drop_anim.eye_scale_y = drop_anim.eye_scale_y.max(1.16);
    let cozmo_bob = -(phase * 2.2).sin().abs() * 2.5;
    draw_raven_sprite(
        factories,
        surf,
        &drop_anim,
        hero_x,
        mid_y - 25.0 + cozmo_bob,
        50.0,
    );

    let bob = (phase * 1.4).sin() * 3.0;
    let pull_x = ((spot_x - (sx + sw * 0.5)) / (sw * 0.5)).clamp(-1.0, 1.0) * 10.0;
    let pull_y = ((spot_y - mid_y) / (sh * 0.5)).clamp(-1.0, 1.0) * 5.0;
    let tilt = ((spot_x - (sx + sw * 0.5)) / (sw * 0.5)).clamp(-1.0, 1.0) * 8.0;
    let card_cx = hero_x + 50.0 + 18.0 + 28.0 + pull_x;
    let card_cy = mid_y + bob + pull_y;

    surf.push_clip(sx, sy, sw, sh);
    for k in 0..3 {
        let u = (phase * 0.38 + (k as f32) / 3.0).fract();
        let r = 108.0 - 82.0 * u;
        let ring_a = (u * std::f32::consts::PI).sin() * 0.34;
        surf.draw_ellipse(
            card_cx,
            card_cy,
            r,
            r * 0.62,
            glow_col.with_alpha(ring_a),
            1.4,
        );
    }
    for k in 0..6 {
        let u = (phase * 0.32 + (k as f32) / 6.0).fract();
        let ang = (k as f32) * (std::f32::consts::TAU / 6.0) + phase * 0.45;
        let dist = 92.0 * (1.0 - u) + 24.0 * u;
        let px = card_cx + ang.cos() * dist * 1.25;
        let py = card_cy + ang.sin() * dist * 0.68;
        let sp_a = (u * std::f32::consts::PI).sin() * 0.75;
        let sp_r = 3.2 + 2.2 * (1.0 - u);
        surf.fill_sparkle(
            factories,
            px,
            py,
            sp_r,
            phase * 35.0 + (k as f32) * 25.0,
            glow_col.lerp(TEXT_PRIMARY, 0.5).with_alpha(sp_a),
        );
    }
    surf.fill_radial_glow(
        card_cx,
        card_cy,
        56.0,
        44.0,
        glow_col.with_alpha(0.32),
        Color::argb(0, 0, 0, 0),
    );
    surf.pop_clip();

    for (ox, oy, ang, cw, ch, fill, border) in [
        (
            -9.0_f32,
            -2.0_f32,
            -9.0 + tilt * 0.5,
            36.0_f32,
            44.0_f32,
            SURFACE,
            OUTLINE,
        ),
        (9.0, -1.0, 8.0 + tilt * 0.5, 36.0, 44.0, SURFACE, OUTLINE),
        (
            0.0,
            0.0,
            tilt * 0.35,
            42.0,
            50.0,
            CARD_BG,
            glow_col.with_alpha(0.9),
        ),
    ] {
        let rx = card_cx + ox - cw * 0.5;
        let ry = card_cy + oy - ch * 0.5;
        surf.set_transform(Matrix3x2::rotation_around(
            ang,
            Vector2::new(card_cx + ox, card_cy + oy),
        ));
        surf.fill_rounded_rect(rx, ry, cw, ch, 9.0, 9.0, fill);
        surf.draw_rounded_rect(rx, ry, cw, ch, 9.0, 9.0, border, 1.4, None);
        surf.set_transform(Matrix3x2::identity());
    }
    surf.set_transform(Matrix3x2::rotation_around(
        tilt * 0.35,
        Vector2::new(card_cx, card_cy),
    ));
    surf.draw_text(
        factories,
        "\u{E896}",
        card_cx - 14.0,
        card_cy - 14.0,
        28.0,
        28.0,
        FontKind::Icon,
        19.0,
        false,
        glow_col.lerp(TEXT_PRIMARY, 0.45),
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    surf.set_transform(Matrix3x2::identity());

    if app.drag_file_count > 1 {
        let badge = format!("+{}", app.drag_file_count);
        let bw = (factories
            .measure_text(&badge, FontKind::Ui, 10.5, true, true, 48.0, false)
            .0
            + 10.0)
            .max(22.0);
        let bx = card_cx + 10.0;
        let by = card_cy - 29.0;
        surf.fill_rounded_rect(bx, by, bw, 18.0, 9.0, 9.0, glow_col);
        surf.draw_text(
            factories,
            &badge,
            bx,
            by,
            bw,
            18.0,
            FontKind::Ui,
            10.5,
            true,
            INVERSE_TEXT,
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    }

    let tx = hero_x + 50.0 + 18.0 + 56.0 + 18.0;
    surf.draw_text(
        factories,
        s.shelf_drop_to_keep,
        tx,
        mid_y - 19.0,
        text_w + 4.0,
        22.0,
        FontKind::Ui,
        15.0,
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
        &sub_txt,
        tx,
        mid_y + 3.0,
        text_w + 4.0,
        18.0,
        FontKind::Ui,
        12.0,
        false,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Center,
        true,
        false,
        true,
        false,
    );

    surf.pop_layer();
}

fn draw_shelf_drop_burst(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
) {
    if app.drop_burst_t >= 1.0 {
        return;
    }
    let t = app.drop_burst_t.clamp(0.0, 1.0) as f32;
    let ease = 1.0 - (1.0 - t).powi(3);
    let alpha = (1.0 - t).powi(2) * 0.88;
    let (bx, by) = app
        .drop_burst_pos
        .map_or((sx + sw * 0.5, sy + sh * 0.5), |(x, y)| {
            ((x as f32).clamp(sx, sx + sw), (y as f32).clamp(sy, sy + sh))
        });
    let ring_col = app.accent.lerp(TEXT_PRIMARY, 0.4);
    surf.push_clip(sx, sy, sw, sh);

    if t < 0.48 {
        let ft = t / 0.48;
        let fa = (1.0 - ft).powi(2) * 0.52;
        let fr = 28.0 + 190.0 * (1.0 - (1.0 - ft).powi(2));
        surf.fill_radial_glow(
            bx,
            by,
            fr,
            fr * 0.72,
            ring_col.with_alpha(fa),
            Color::argb(0, 0, 0, 0),
        );
    }

    surf.draw_ellipse(
        bx,
        by,
        12.0 + 215.0 * ease,
        12.0 + 215.0 * ease,
        ring_col.with_alpha(alpha),
        2.6,
    );
    surf.draw_ellipse(
        bx,
        by,
        6.0 + 135.0 * ease,
        6.0 + 135.0 * ease,
        TEXT_PRIMARY.with_alpha(alpha * 0.62),
        1.6,
    );

    let palette = [
        app.accent,
        TEXT_PRIMARY,
        PURE_EYE_AMBER,
        app.accent.lerp(TEXT_PRIMARY, 0.5),
    ];
    for k in 0..12 {
        let kf = k as f32;
        let ang = kf * (std::f32::consts::TAU / 12.0) + 0.22;
        let speed = 78.0 + (k % 3) as f32 * 34.0;
        let dist = speed * ease;
        let px = bx + ang.cos() * dist * 1.25;
        let py = by + ang.sin() * dist * 0.72 + 20.0 * t * t;
        let p_alpha = ((1.0 - t) * 1.25).clamp(0.0, 1.0).powi(2);
        let col = palette[k % palette.len()].with_alpha(p_alpha);
        if k % 2 == 0 {
            let r = (6.5 - 3.2 * t).max(1.5);
            surf.fill_sparkle(factories, px, py, r, t * 140.0 + kf * 30.0, col);
        } else {
            let pw = (7.5 - 3.0 * t).max(2.5);
            let ph = 2.8_f32;
            surf.set_transform(Matrix3x2::rotation_around(
                ang.to_degrees(),
                Vector2::new(px, py),
            ));
            surf.fill_rounded_rect(px - pw * 0.5, py - ph * 0.5, pw, ph, 1.4, 1.4, col);
            surf.set_transform(Matrix3x2::identity());
        }
    }

    let pill_op = if t < 0.14 {
        t / 0.14
    } else if t > 0.72 {
        ((1.0 - t) / 0.28).clamp(0.0, 1.0)
    } else {
        1.0
    };
    if pill_op > 0.01 {
        let pop = if t < 0.22 {
            let u = t / 0.22;
            0.75 + 0.32 * (1.0 - (1.0 - u).powi(2))
        } else if t < 0.36 {
            let u = (t - 0.22) / 0.14;
            1.07 - 0.07 * u
        } else {
            1.0
        };
        let badge_txt = format!("+{}", app.drop_celebrate_count.max(1));
        let bw = (factories
            .measure_text(&badge_txt, FontKind::Ui, 12.0, true, true, 60.0, false)
            .0
            + 44.0)
            .max(68.0);
        let bh = 28.0_f32;
        let px = sx + (sw - bw) * 0.5;
        let py = sy + sh - bh - 4.0;
        surf.set_transform(Matrix3x2::scale_around(
            pop,
            pop,
            Vector2::new(px + bw * 0.5, py + bh * 0.5),
        ));
        surf.fill_rounded_rect(
            px,
            py,
            bw,
            bh,
            14.0,
            14.0,
            CARD_BG.with_alpha(0.94 * pill_op),
        );
        surf.draw_rounded_rect(
            px,
            py,
            bw,
            bh,
            14.0,
            14.0,
            ring_col.with_alpha(0.85 * pill_op),
            1.4,
            None,
        );
        let mut happy = RavenAnimator::default();
        happy.nod();
        happy.frame_index = ((t * 36.0) as usize) % 36;
        draw_raven_sprite(factories, surf, &happy, px + 6.0, py + 3.0, 22.0);
        surf.draw_text(
            factories,
            &badge_txt,
            px + 30.0,
            py,
            bw - 38.0,
            bh,
            FontKind::Ui,
            12.0,
            true,
            TEXT_PRIMARY.with_alpha(pill_op),
            HAlign::Center,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
        surf.set_transform(Matrix3x2::identity());
    }

    surf.pop_clip();
}
