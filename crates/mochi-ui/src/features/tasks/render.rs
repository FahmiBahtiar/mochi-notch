use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::companion::render::draw_companion_sprite;
use crate::features::companion::state::CompanionAnimator;
use crate::features::widgets::render::{draw_icon_button, draw_undo_toast};
use crate::render::{HitMap, RectF};
use crate::state::{HoverTarget, NotchApp};
use crate::theme::{
    DIVIDER, SELECTION_HIGHLIGHT, SUBTLE_FILL, SURFACE, TEXT_PLACEHOLDER, TEXT_PRIMARY,
    TEXT_SECONDARY,
};

#[expect(clippy::too_many_arguments, reason = "renders tasks page")]
pub(crate) fn draw_tasks_page(
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
    let tx = px + 16.0;
    let ty = py + 6.0;
    let tw = pw - 32.0;
    let th = ph - 20.0;
    let s = app.lang.strings();

    let list_w = (tw - 260.0).max(220.0);
    let list_h = th;

    if app.store.data.tasks.is_empty() {
        let cy = ty + (list_h - 64.0) / 2.0;
        let mut sleep_anim = CompanionAnimator::default();
        sleep_anim.sleep();
        draw_companion_sprite(
            factories,
            surf,
            &sleep_anim,
            tx + (list_w - 40.0) / 2.0,
            cy,
            40.0,
        );
        surf.draw_text(
            factories,
            s.tasks_empty,
            tx,
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
                x: tx,
                y: ty,
                w: list_w,
                h: list_h,
            });
        }
        let total_h = (app.store.data.tasks.len() as f32) * 32.0;
        let max_scroll = (total_h - list_h).max(0.0) as f64;
        app.tasks_scroll = app.tasks_scroll.clamp(0.0, max_scroll);
        let scroll = app.tasks_scroll as f32;

        let above = scroll > 0.5;
        let below = scroll < max_scroll as f32 - 0.5;
        let mask = if (above || below) && list_h > 56.0 {
            surf.create_linear_mask(
                tx,
                ty,
                tx,
                ty + list_h,
                &[
                    (0.0, if above { 0.0 } else { 1.0 }),
                    (24.0 / list_h, 1.0),
                    (1.0 - 24.0 / list_h, 1.0),
                    (1.0, if below { 0.0 } else { 1.0 }),
                ],
            )
        } else {
            None
        };
        surf.push_clip(tx, ty, list_w, list_h);
        if mask.is_some() {
            surf.push_layer(tx, ty, list_w, list_h, 1.0, None, mask.as_ref());
        }
        for (i, task) in app.store.data.tasks.iter().enumerate() {
            let ry = ty - scroll + (i as f32) * 32.0;
            if ry + 32.0 < ty || ry > ty + list_h {
                continue;
            }
            let check_glyph = if task.done { "\u{E73A}" } else { "\u{E739}" };
            draw_icon_button(
                factories,
                surf,
                tx,
                ry + 1.0,
                30.0,
                check_glyph,
                15.0,
                TEXT_PRIMARY,
                app.hover == HoverTarget::TaskToggle(i),
                app.pressed == HoverTarget::TaskToggle(i),
            );
            if interactive {
                hits.push(
                    RectF {
                        x: tx,
                        y: ry + 1.0,
                        w: 30.0,
                        h: 30.0,
                    },
                    HoverTarget::TaskToggle(i),
                );
            }

            let rem_x = tx + list_w - 26.0;
            draw_icon_button(
                factories,
                surf,
                rem_x,
                ry + 3.0,
                26.0,
                "\u{E711}",
                10.0,
                TEXT_PRIMARY.with_alpha(0.6),
                app.hover == HoverTarget::TaskRemove(i),
                app.pressed == HoverTarget::TaskRemove(i),
            );
            if interactive {
                hits.push(
                    RectF {
                        x: rem_x,
                        y: ry + 3.0,
                        w: 26.0,
                        h: 26.0,
                    },
                    HoverTarget::TaskRemove(i),
                );
            }

            let mut chip_w = 0.0;
            if !task.done && task.due.is_some() {
                let due_txt = app.format_task_due(task);
                if !due_txt.is_empty() {
                    let dw = factories
                        .measure_text(&due_txt, FontKind::Ui, 11.0, false, false, 140.0, false)
                        .0;
                    let cw = 14.0 + 14.0 + dw;
                    let cx = rem_x - 4.0 - cw;
                    let cy = ry + 7.0;
                    surf.fill_rounded_rect(cx, cy, cw, 18.0, 8.0, 8.0, SURFACE);
                    surf.draw_text(
                        factories,
                        "\u{EA8F}",
                        cx + 7.0,
                        cy,
                        12.0,
                        18.0,
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
                    surf.draw_text(
                        factories,
                        &due_txt,
                        cx + 21.0,
                        cy,
                        dw + 2.0,
                        18.0,
                        FontKind::Ui,
                        11.0,
                        false,
                        TEXT_SECONDARY,
                        HAlign::Left,
                        VAlign::Center,
                        false,
                        false,
                        false,
                        false,
                    );
                    chip_w = cw + 4.0;
                }
            }

            let lbl_w = (rem_x - chip_w - (tx + 36.0) - 6.0).max(20.0);
            surf.draw_text(
                factories,
                &task.text,
                tx + 36.0,
                ry,
                lbl_w,
                32.0,
                FontKind::Ui,
                13.0,
                false,
                if task.done {
                    TEXT_SECONDARY
                } else {
                    TEXT_PRIMARY
                },
                HAlign::Left,
                VAlign::Center,
                false,
                false,
                true,
                task.done,
            );
        }
        if mask.is_some() {
            surf.pop_layer();
        }
        surf.pop_clip();
    }

    if let Some(ref undo) = app.undo {
        draw_undo_toast(
            app,
            factories,
            surf,
            hits,
            tx,
            ty + list_h - 34.0,
            list_w,
            &undo.message,
            interactive,
        );
    }

    let div_x = tx + list_w + 12.0;
    surf.fill_rect(div_x, ty + 4.0, 1.0, (th - 8.0).max(20.0), DIVIDER);
    let rx = div_x + 14.0;
    let rw = (tx + tw - rx).max(120.0);
    let input_h = 34.0;
    let iy = ty + (th - input_h - 28.0) * 0.5;
    surf.fill_rounded_rect(rx, iy, rw, input_h, 14.0, 14.0, SUBTLE_FILL);
    if interactive {
        hits.push(
            RectF {
                x: rx,
                y: iy,
                w: rw - 34.0,
                h: input_h,
            },
            HoverTarget::TaskInput,
        );
    }
    if app.task_input.is_empty() {
        surf.draw_text(
            factories,
            s.tasks_new_placeholder,
            rx + 12.0,
            iy,
            rw - 48.0,
            input_h,
            FontKind::Ui,
            13.0,
            false,
            TEXT_PLACEHOLDER,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
    } else {
        surf.draw_text(
            factories,
            &app.task_input,
            rx + 12.0,
            iy,
            rw - 48.0,
            input_h,
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
    }
    if app.task_input_focused {
        let caret_x = rx
            + 12.0
            + factories
                .measure_text(
                    &app.task_input,
                    FontKind::Ui,
                    13.0,
                    false,
                    false,
                    rw - 48.0,
                    false,
                )
                .0
                .min(rw - 50.0);
        surf.fill_rect(caret_x, iy + 9.0, 1.5, 16.0, SELECTION_HIGHLIGHT);
    }
    let add_x = rx + rw - 32.0;
    draw_icon_button(
        factories,
        surf,
        add_x,
        iy + 2.0,
        30.0,
        "\u{E710}",
        13.0,
        TEXT_PRIMARY,
        app.hover == HoverTarget::TaskAddBtn,
        app.pressed == HoverTarget::TaskAddBtn,
    );
    if interactive {
        hits.push(
            RectF {
                x: add_x,
                y: iy + 2.0,
                w: 30.0,
                h: 30.0,
            },
            HoverTarget::TaskAddBtn,
        );
    }
    let open_count = app.store.data.tasks.iter().filter(|t| !t.done).count();
    let done_count = app.store.data.tasks.len().saturating_sub(open_count);
    let summary = format!("{open_count} open · {done_count} done");
    surf.draw_text(
        factories,
        &summary,
        rx + 6.0,
        iy + input_h + 8.0,
        rw - 12.0,
        18.0,
        FontKind::Ui,
        11.0,
        false,
        TEXT_SECONDARY,
        HAlign::Left,
        VAlign::Top,
        true,
        false,
        true,
        false,
    );
}
