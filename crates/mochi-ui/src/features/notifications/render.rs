use chrono::Timelike;

use mochi_winrt::notifications::NotificationAccess;
use mochi_winrt::strings::fmt1;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_companion_sprite;
use crate::features::companion::state::CompanionAnimator;
use crate::features::widgets::render::{draw_icon_button, draw_icon_view, draw_pill_button};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    CAMERA_BRUSH, Color, DIVIDER, HOVER_FILL, MICROPHONE_BRUSH, SUBTLE_FILL, TEXT_PRIMARY,
    TEXT_SECONDARY, WARNING_BRUSH,
};

#[expect(clippy::too_many_arguments, reason = "renders notifications page")]
pub(crate) fn draw_notifications_page(
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
    let nx = px + 16.0;
    let ny = py + 6.0;
    let nw = pw - 32.0;
    let nh = ph - 20.0;
    let s = app.lang.strings();

    let list_w = (nw - 240.0).max(240.0);
    let list_h = nh;

    if app.notifications.items.is_empty() {
        let cy = ny + (list_h - 64.0) / 2.0;
        let mut perch = CompanionAnimator::default();
        perch.perch();
        draw_companion_sprite(
            factories,
            surf,
            &perch,
            nx + (list_w - 40.0) / 2.0,
            cy,
            40.0,
        );
        surf.draw_text(
            factories,
            s.notif_empty,
            nx,
            cy + 44.0,
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
                x: nx,
                y: ny,
                w: list_w,
                h: list_h,
            });
        }
        let row_h = 42.0_f32;
        let total_h = (app.notifications.items.len() as f32) * row_h;
        let max_scroll = (total_h - list_h).max(0.0) as f64;
        app.notifications_scroll = app.notifications_scroll.clamp(0.0, max_scroll);
        let scroll = app.notifications_scroll as f32;

        let above = scroll > 0.5;
        let below = scroll < max_scroll as f32 - 0.5;
        let mask = if (above || below) && list_h > 56.0 {
            surf.create_linear_mask(
                nx,
                ny,
                nx,
                ny + list_h,
                &[
                    (0.0, if above { 0.0 } else { 1.0 }),
                    (20.0 / list_h, 1.0),
                    (1.0 - 20.0 / list_h, 1.0),
                    (1.0, if below { 0.0 } else { 1.0 }),
                ],
            )
        } else {
            None
        };
        surf.push_clip(nx, ny, list_w, list_h);
        if mask.is_some() {
            surf.push_layer(nx, ny, list_w, list_h, 1.0, None, mask.as_ref());
        }
        for (i, item) in app.notifications.items.iter().enumerate() {
            let ry = ny - scroll + (i as f32) * row_h;
            if ry + row_h < ny || ry > ny + list_h {
                continue;
            }
            let card_h = row_h - 4.0;
            let hovered =
                app.hover == HoverTarget::NotifItem(i) || app.hover == HoverTarget::NotifDismiss(i);
            let bg = if hovered {
                HOVER_FILL
            } else if item.unread {
                SUBTLE_FILL
            } else {
                Color::argb(0x0A, 0xFF, 0xFF, 0xFF)
            };
            surf.fill_rounded_rect(nx, ry + 2.0, list_w, card_h, 10.0, 10.0, bg);
            if item.unread {
                let dot_col = if item.warning {
                    WARNING_BRUSH
                } else {
                    app.accent
                };
                surf.fill_ellipse(nx + 8.0, ry + 2.0 + card_h * 0.5, 2.5, 2.5, dot_col);
            }
            let icon_fg = if item.warning {
                WARNING_BRUSH
            } else if item.unread {
                TEXT_PRIMARY
            } else {
                TEXT_SECONDARY
            };
            draw_icon_view(
                factories,
                surf,
                &item.glyph,
                None,
                nx + 16.0,
                ry + 2.0 + (card_h - 15.0) * 0.5,
                15.0,
                icon_fg,
            );

            let dis_x = nx + list_w - 26.0;
            if hovered {
                draw_icon_button(
                    factories,
                    surf,
                    dis_x,
                    ry + 2.0 + (card_h - 24.0) * 0.5,
                    24.0,
                    "\u{E711}",
                    9.5,
                    TEXT_SECONDARY,
                    app.hover == HoverTarget::NotifDismiss(i),
                    app.pressed == HoverTarget::NotifDismiss(i),
                );
            }

            let time_s = format!("{:02}:{:02}", item.at.hour(), item.at.minute());
            let meta = if item.source.is_empty() {
                time_s
            } else {
                format!("{} · {time_s}", item.source)
            };
            let meta_w = factories
                .measure_text(&meta, FontKind::Ui, 10.5, false, true, 140.0, false)
                .0
                .min(140.0);
            let text_x = nx + 38.0;
            let right_pad = if hovered { 30.0 } else { 10.0 };
            let title_w = (nx + list_w - right_pad - meta_w - 8.0 - text_x).max(40.0);
            let has_body = !item.body.is_empty();
            let title_y = if has_body { ry + 5.0 } else { ry + 2.0 };
            let title_h = if has_body { 16.0 } else { card_h };
            surf.draw_text(
                factories,
                &item.title,
                text_x,
                title_y,
                title_w,
                title_h,
                FontKind::Ui,
                12.0,
                item.unread,
                if item.unread {
                    TEXT_PRIMARY
                } else {
                    TEXT_PRIMARY.with_alpha(0.82)
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
                &meta,
                nx + list_w - right_pad - meta_w,
                title_y,
                meta_w,
                title_h,
                FontKind::Ui,
                10.5,
                false,
                TEXT_SECONDARY,
                HAlign::Right,
                VAlign::Center,
                true,
                false,
                true,
                false,
            );
            if has_body {
                surf.draw_text(
                    factories,
                    &item.body,
                    text_x,
                    ry + 21.0,
                    (nx + list_w - right_pad - text_x).max(40.0),
                    15.0,
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
            }
            if interactive {
                hits.push(
                    RectF {
                        x: nx,
                        y: ry + 2.0,
                        w: list_w,
                        h: card_h,
                    },
                    HoverTarget::NotifItem(i),
                );
                if hovered {
                    hits.push(
                        RectF {
                            x: dis_x,
                            y: ry + 2.0 + (card_h - 24.0) * 0.5,
                            w: 24.0,
                            h: 24.0,
                        },
                        HoverTarget::NotifDismiss(i),
                    );
                }
            }
        }
        if mask.is_some() {
            surf.pop_layer();
        }
        surf.pop_clip();
    }

    let div_x = nx + list_w + 12.0;
    surf.fill_rect(div_x, ny + 4.0, 1.0, (nh - 8.0).max(20.0), DIVIDER);
    let rx = div_x + 14.0;
    let rw = (nx + nw - rx).max(120.0);

    let unread = app.unread_notifications_count();
    let summary = if unread == 0 {
        s.notif_empty.to_string()
    } else {
        fmt1(s.notif_unread, &unread.to_string())
    };
    surf.draw_text(
        factories,
        &summary,
        rx + 2.0,
        ny + 4.0,
        rw - 4.0,
        20.0,
        FontKind::Ui,
        13.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        true,
        false,
        true,
        false,
    );

    let win_ok = app.win_notif_access == NotificationAccess::Allowed;
    let status_col = if win_ok {
        CAMERA_BRUSH
    } else {
        MICROPHONE_BRUSH
    };
    surf.fill_ellipse(rx + 6.0, ny + 34.0, 3.0, 3.0, status_col);
    surf.draw_text(
        factories,
        if win_ok {
            s.notif_win_active
        } else {
            s.notif_win_enable
        },
        rx + 14.0,
        ny + 25.0,
        rw - 16.0,
        18.0,
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

    let btn_y = ny + 54.0;
    let half_w = ((rw - 8.0) * 0.5).max(60.0);
    draw_pill_button(
        factories,
        surf,
        rx,
        btn_y,
        half_w,
        28.0,
        None,
        s.notif_mark_read,
        false,
        app.hover == HoverTarget::NotifMarkAllRead,
        app.pressed == HoverTarget::NotifMarkAllRead,
    );
    draw_pill_button(
        factories,
        surf,
        rx + half_w + 8.0,
        btn_y,
        half_w,
        28.0,
        None,
        s.notif_clear,
        false,
        app.hover == HoverTarget::NotifClearAll,
        app.pressed == HoverTarget::NotifClearAll,
    );
    if interactive {
        hits.push(
            RectF {
                x: rx,
                y: btn_y,
                w: half_w,
                h: 28.0,
            },
            HoverTarget::NotifMarkAllRead,
        );
        hits.push(
            RectF {
                x: rx + half_w + 8.0,
                y: btn_y,
                w: half_w,
                h: 28.0,
            },
            HoverTarget::NotifClearAll,
        );
    }

    if !win_ok {
        let wy = btn_y + 36.0;
        draw_pill_button(
            factories,
            surf,
            rx,
            wy,
            rw,
            28.0,
            Some("\u{E713}"),
            s.notif_win_enable,
            true,
            app.hover == HoverTarget::NotifOpenWinSettings,
            app.pressed == HoverTarget::NotifOpenWinSettings,
        );
        if interactive {
            hits.push(
                RectF {
                    x: rx,
                    y: wy,
                    w: rw,
                    h: 28.0,
                },
                HoverTarget::NotifOpenWinSettings,
            );
        }
    }
}
