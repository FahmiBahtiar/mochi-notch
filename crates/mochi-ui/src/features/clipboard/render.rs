use chrono::Timelike;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::clipboard::state::ClipboardKind;
use crate::features::companion::render::draw_companion_sprite;
use crate::features::companion::state::CompanionAnimator;
use crate::features::widgets::render::{draw_icon_button, draw_pill_button, rounded_rect_geom};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{CAMERA_BRUSH, Color, DIVIDER, HOVER_FILL, TEXT_PRIMARY, TEXT_SECONDARY};

#[expect(clippy::too_many_arguments, reason = "renders clipboard history page")]
pub(crate) fn draw_clipboard_page(
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
    let cx = px + 16.0;
    let cy = py + 6.0;
    let cw = pw - 32.0;
    let ch = ph - 20.0;
    let s = app.lang.strings();

    let list_w = (cw - 318.0).max(260.0);
    let list_h = ch;

    if app.clipboard_items.is_empty() {
        let ry = cy + (list_h - 64.0) / 2.0;
        let mut perch = CompanionAnimator::default();
        perch.perch();
        draw_companion_sprite(
            factories,
            surf,
            &perch,
            cx + (list_w - 40.0) / 2.0,
            ry,
            40.0,
        );
        surf.draw_text(
            factories,
            s.clip_empty,
            cx,
            ry + 44.0,
            list_w,
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
    } else {
        if interactive {
            hits.scroll_rect = Some(RectF {
                x: cx,
                y: cy,
                w: list_w,
                h: list_h,
            });
        }
        let row_h = 40.0_f32;
        let total_h = (app.clipboard_items.len() as f32) * row_h;
        let max_scroll = (total_h - list_h).max(0.0) as f64;
        app.clipboard_scroll = app.clipboard_scroll.clamp(0.0, max_scroll);
        let scroll = app.clipboard_scroll as f32;

        let above = scroll > 0.5;
        let below = scroll < max_scroll as f32 - 0.5;
        let mask = if (above || below) && list_h > 56.0 {
            surf.create_linear_mask(
                cx,
                cy,
                cx,
                cy + list_h,
                &[
                    (0.0, if above { 0.0 } else { 1.0 }),
                    (18.0 / list_h, 1.0),
                    (1.0 - 18.0 / list_h, 1.0),
                    (1.0, if below { 0.0 } else { 1.0 }),
                ],
            )
        } else {
            None
        };
        surf.push_clip(cx, cy, list_w, list_h);
        if mask.is_some() {
            surf.push_layer(cx, cy, list_w, list_h, 1.0, None, mask.as_ref());
        }
        for i in 0..app.clipboard_items.len() {
            let ry = cy - scroll + (i as f32) * row_h;
            if ry + row_h < cy || ry > cy + list_h {
                continue;
            }
            let item = app.clipboard_items[i].clone();
            let card_h = row_h - 4.0;
            let card_y = ry + 2.0;
            let selected = i == app.clipboard_selected;
            let copied_flash = app.clipboard_copied_id == Some(item.id);
            let hovered = app.hover == HoverTarget::ClipItem(i)
                || app.hover == HoverTarget::ClipPin(i)
                || app.hover == HoverTarget::ClipRemove(i);

            let bg = if copied_flash {
                CAMERA_BRUSH.with_alpha(0.20)
            } else if selected {
                app.accent.with_alpha(0.18)
            } else if hovered {
                HOVER_FILL
            } else {
                Color::argb(0x0A, 0xFF, 0xFF, 0xFF)
            };
            surf.fill_rounded_rect(cx, card_y, list_w, card_h, 10.0, 10.0, bg);
            if copied_flash {
                surf.draw_rounded_rect(
                    cx,
                    card_y,
                    list_w,
                    card_h,
                    10.0,
                    10.0,
                    CAMERA_BRUSH.with_alpha(0.65),
                    1.0,
                    None,
                );
            } else if selected {
                surf.draw_rounded_rect(
                    cx,
                    card_y,
                    list_w,
                    card_h,
                    10.0,
                    10.0,
                    app.accent.with_alpha(0.45),
                    1.0,
                    None,
                );
            }

            let thumb_x = cx + 8.0;
            let thumb_y = card_y + (card_h - 24.0) * 0.5;
            if let Some(ref dec) = item.decoded
                && let Some(bmp) = surf.get_or_upload_image(item.image_id, dec)
                && let Some(rgeom) = rounded_rect_geom(factories, thumb_x, thumb_y, 24.0, 24.0, 5.0)
            {
                surf.push_layer(thumb_x, thumb_y, 24.0, 24.0, 1.0, Some(&rgeom), None);
                surf.draw_bitmap(&bmp, thumb_x, thumb_y, 24.0, 24.0, 1.0);
                surf.pop_layer();
            } else {
                surf.fill_rounded_rect(
                    thumb_x,
                    thumb_y,
                    24.0,
                    24.0,
                    6.0,
                    6.0,
                    Color::argb(0x14, 0xFF, 0xFF, 0xFF),
                );
                let (glyph, gcol) = match item.kind {
                    ClipboardKind::Files => ("\u{E8B7}", app.accent.lerp(TEXT_PRIMARY, 0.25)),
                    ClipboardKind::Image { .. } => ("\u{EB9F}", app.accent),
                    ClipboardKind::Text => (
                        if item.pinned { "\u{E718}" } else { "\u{E8C8}" },
                        if item.pinned {
                            app.accent
                        } else {
                            TEXT_SECONDARY
                        },
                    ),
                };
                surf.draw_text(
                    factories,
                    glyph,
                    thumb_x,
                    thumb_y,
                    24.0,
                    24.0,
                    FontKind::Icon,
                    12.0,
                    false,
                    gcol,
                    HAlign::Center,
                    VAlign::Center,
                    false,
                    false,
                    false,
                    false,
                );
            }

            let rem_x = cx + list_w - 26.0;
            let pin_x = cx + list_w - 50.0;
            let right_reserved = if copied_flash {
                64.0
            } else if hovered {
                56.0
            } else if item.pinned {
                30.0
            } else {
                44.0
            };

            if copied_flash {
                let badge_w = 54.0;
                let badge_x = cx + list_w - badge_w - 8.0;
                let badge_y = card_y + (card_h - 20.0) * 0.5;
                surf.fill_rounded_rect(
                    badge_x,
                    badge_y,
                    badge_w,
                    20.0,
                    10.0,
                    10.0,
                    CAMERA_BRUSH.with_alpha(0.28),
                );
                surf.draw_text(
                    factories,
                    s.clip_copied,
                    badge_x,
                    badge_y,
                    badge_w,
                    20.0,
                    FontKind::Ui,
                    10.5,
                    true,
                    CAMERA_BRUSH.lerp(TEXT_PRIMARY, 0.45),
                    HAlign::Center,
                    VAlign::Center,
                    false,
                    false,
                    false,
                    false,
                );
            } else if hovered {
                draw_icon_button(
                    factories,
                    surf,
                    pin_x,
                    card_y + (card_h - 22.0) * 0.5,
                    22.0,
                    "\u{E718}",
                    10.0,
                    if item.pinned {
                        app.accent
                    } else {
                        TEXT_SECONDARY
                    },
                    app.hover == HoverTarget::ClipPin(i),
                    app.pressed == HoverTarget::ClipPin(i),
                );
                draw_icon_button(
                    factories,
                    surf,
                    rem_x,
                    card_y + (card_h - 22.0) * 0.5,
                    22.0,
                    "\u{E711}",
                    9.5,
                    TEXT_SECONDARY,
                    app.hover == HoverTarget::ClipRemove(i),
                    app.pressed == HoverTarget::ClipRemove(i),
                );
            } else if item.pinned {
                surf.draw_text(
                    factories,
                    "\u{E718}",
                    cx + list_w - 26.0,
                    card_y,
                    18.0,
                    card_h,
                    FontKind::Icon,
                    10.5,
                    false,
                    app.accent,
                    HAlign::Center,
                    VAlign::Center,
                    false,
                    false,
                    false,
                    false,
                );
            } else {
                let time_s = format!("{:02}:{:02}", item.at.hour(), item.at.minute());
                surf.draw_text(
                    factories,
                    &time_s,
                    cx + list_w - 44.0,
                    card_y,
                    36.0,
                    card_h,
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
            }

            let text_x = cx + 40.0;
            let text_w = (list_w - 40.0 - right_reserved).max(40.0);
            surf.draw_text(
                factories,
                &item.title,
                text_x,
                card_y + 3.0,
                text_w,
                16.0,
                FontKind::Ui,
                12.0,
                selected || item.pinned,
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
                &item.subtitle,
                text_x,
                card_y + 19.0,
                text_w,
                14.0,
                FontKind::Ui,
                10.5,
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
                        y: card_y,
                        w: list_w,
                        h: card_h,
                    },
                    HoverTarget::ClipItem(i),
                );
                if hovered {
                    hits.push(
                        RectF {
                            x: pin_x,
                            y: card_y + (card_h - 22.0) * 0.5,
                            w: 22.0,
                            h: 22.0,
                        },
                        HoverTarget::ClipPin(i),
                    );
                    hits.push(
                        RectF {
                            x: rem_x,
                            y: card_y + (card_h - 22.0) * 0.5,
                            w: 22.0,
                            h: 22.0,
                        },
                        HoverTarget::ClipRemove(i),
                    );
                }
            }
        }
        if mask.is_some() {
            surf.pop_layer();
        }
        surf.pop_clip();
    }

    let div_x = cx + list_w + 12.0;
    surf.fill_rect(div_x, cy + 4.0, 1.0, (ch - 8.0).max(20.0), DIVIDER);
    let rx = div_x + 14.0;
    let rw = (cx + cw - rx).max(140.0);

    let selected_item = app.clipboard_items.get(app.clipboard_selected).cloned();
    let header_label = if let Some(ref item) = selected_item {
        let time_s = format!("{:02}:{:02}", item.at.hour(), item.at.minute());
        format!("{} · {time_s}", item.subtitle)
    } else {
        s.tab_clipboard.to_string()
    };
    let clear_w = if app.clipboard_items.is_empty() {
        0.0
    } else {
        68.0
    };
    surf.draw_text(
        factories,
        &header_label,
        rx + 2.0,
        cy + 2.0,
        (rw - clear_w - 8.0).max(40.0),
        24.0,
        FontKind::Ui,
        11.5,
        true,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Center,
        true,
        false,
        true,
        false,
    );
    if !app.clipboard_items.is_empty() {
        let bx = rx + rw - clear_w;
        draw_pill_button(
            factories,
            surf,
            bx,
            cy + 1.0,
            clear_w,
            24.0,
            None,
            s.clip_clear,
            false,
            app.hover == HoverTarget::ClipClear,
            app.pressed == HoverTarget::ClipClear,
        );
        if interactive {
            hits.push(
                RectF {
                    x: bx,
                    y: cy + 1.0,
                    w: clear_w,
                    h: 24.0,
                },
                HoverTarget::ClipClear,
            );
        }
    }

    let box_y = cy + 30.0;
    let box_h = (ch - 32.0).max(40.0);
    surf.fill_rounded_rect(
        rx,
        box_y,
        rw,
        box_h,
        10.0,
        10.0,
        Color::argb(0x10, 0xFF, 0xFF, 0xFF),
    );
    surf.draw_rounded_rect(
        rx,
        box_y,
        rw,
        box_h,
        10.0,
        10.0,
        Color::argb(0x1A, 0xFF, 0xFF, 0xFF),
        1.0,
        None,
    );

    if let Some(ref item) = selected_item {
        if let Some(ref dec) = item.decoded
            && let Some(bmp) = surf.get_or_upload_image(item.image_id, dec)
        {
            let pad = 8.0;
            let avail_w = (rw - 2.0 * pad).max(20.0);
            let avail_h = (box_h - 2.0 * pad).max(20.0);
            let iw = (dec.width.max(1)) as f32;
            let ih = (dec.height.max(1)) as f32;
            let scale = (avail_w / iw).min(avail_h / ih);
            let dw = iw * scale;
            let dh = ih * scale;
            let dx = rx + (rw - dw) * 0.5;
            let dy = box_y + (box_h - dh) * 0.5;
            if let Some(rgeom) = rounded_rect_geom(factories, dx, dy, dw, dh, 6.0) {
                surf.push_layer(dx, dy, dw, dh, 1.0, Some(&rgeom), None);
                surf.draw_bitmap(&bmp, dx, dy, dw, dh, 1.0);
                surf.pop_layer();
            }
        } else {
            surf.push_clip(rx + 10.0, box_y + 8.0, rw - 20.0, box_h - 16.0);
            surf.draw_text(
                factories,
                &item.preview,
                rx + 10.0,
                box_y + 8.0,
                rw - 20.0,
                box_h - 16.0,
                FontKind::Ui,
                11.5,
                false,
                TEXT_PRIMARY.with_alpha(0.90),
                HAlign::Left,
                VAlign::Top,
                false,
                true,
                false,
                false,
            );
            surf.pop_clip();
        }
    } else {
        surf.draw_text(
            factories,
            s.clip_hint,
            rx + 10.0,
            box_y,
            rw - 20.0,
            box_h,
            FontKind::Ui,
            11.5,
            false,
            TEXT_SECONDARY,
            HAlign::Center,
            VAlign::Center,
            false,
            true,
            false,
            false,
        );
    }
}
