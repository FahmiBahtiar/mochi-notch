use chrono::{Local, Timelike};
use windows::Win32::Graphics::Direct2D::ID2D1Geometry;
use windows_core::Interface;
use windows_numerics::{Matrix3x2, Vector2};

use mochi_winrt::mochi_spring;
use mochi_winrt::strings::fmt1;

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::features::agenda::render::draw_agenda_page;
use crate::features::clipboard::render::draw_clipboard_page;
use crate::features::companion::render::{draw_companion_sprite, draw_robo_visor};
use crate::features::companion::state::{CompanionAnimator, CompanionMode};
use crate::features::focus::render::draw_focus_page;
use crate::features::hud::render::{draw_compact_view, draw_hud_view, draw_news_view};
use crate::features::media::render::draw_media_page;
use crate::features::notifications::render::draw_notifications_page;
use crate::features::prayer::render::draw_pray_page;
use crate::features::shelf::render::draw_shelf_page;
use crate::features::tasks::render::draw_tasks_page;
use crate::features::water::render::draw_water_page;
use crate::features::weather::render::draw_weather_page;
use crate::features::widgets::render::{draw_icon_button, draw_icon_view};
use crate::state::{
    BUBBLE_GAP, BUBBLE_HEIGHT, BUBBLE_WIDTH, EAR, EXPANDED_SHORT, EXPANDED_WIDE, EXPANDED_WIDTH,
    HoverTarget, NotchApp, NotchPage,
};
use crate::theme::{
    CAMERA_BRUSH, CLAUDE_ORANGE, COPILOT_BLUE, Color, HOVER_FILL, MICROPHONE_BRUSH, NOTCH_BLACK,
    PRESSED_FILL, SUBTLE_FILL, TEXT_PLACEHOLDER, TEXT_PRIMARY, TEXT_SECONDARY, TRACK,
    WARNING_BRUSH,
};

#[derive(Debug, Clone, Copy)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

#[derive(Default)]
pub struct HitMap {
    pub items: Vec<(RectF, HoverTarget)>,
    pub notch_rect: Option<RectF>,
    pub bubble_rect: Option<RectF>,
    pub panel_rect: Option<RectF>,
    pub progress_rect: Option<RectF>,
    pub scroll_rect: Option<RectF>,
}

impl HitMap {
    pub fn push(&mut self, rect: RectF, target: HoverTarget) {
        self.items.push((rect, target));
    }

    pub fn hit_test(&self, x: f32, y: f32) -> HoverTarget {
        for &(r, t) in self.items.iter().rev() {
            if r.contains(x, y) {
                return t;
            }
        }
        if self.bubble_rect.is_some_and(|r| r.contains(x, y)) {
            return HoverTarget::Bubble;
        }
        if self.panel_rect.is_some_and(|r| r.contains(x, y)) {
            return HoverTarget::ActivityPanel;
        }
        if self.notch_rect.is_some_and(|r| r.contains(x, y)) {
            return HoverTarget::Notch;
        }
        HoverTarget::None
    }
}

pub fn render_notch(app: &mut NotchApp, factories: &D2dFactories, surf: &mut D2dSurface) -> HitMap {
    let mut hits = HitMap::default();
    let cx = (app.window_w / 2.0) as f32;
    let width = app.width.value.max(24.0);
    let height = app.height.value.max(14.0);
    let radius = app.radius.value.clamp(0.0, width.min(height) / 2.0);
    let w = width as f32;
    let h = height as f32;
    let notch_x = cx - w / 2.0;
    let notch_outer_x = notch_x - EAR as f32;
    let no_shadow = app.bench_variant.as_deref() == Some("no-shadow");

    let progress = app.expand.value;
    let op = mochi_spring::port::compute_frame_opacities(
        app.expanded,
        progress,
        app.hud.value,
        app.ambient.value,
        app.overlay_is_news,
    );
    let compact_opacity = op.compact_opacity as f32;
    let expanded_opacity = op.expanded_opacity as f32;

    let overlay = app.hud.value.clamp(0.0, 1.0) as f32;
    let ambient = app.ambient.value.clamp(0.0, 1.0) as f32;
    let under_overlay = compact_opacity * (1.0 - overlay);

    // 1. Bubble (drawn first so it slides out from under the notch)
    let bubble_shown = app.bubble.value.max(0.0);
    let bubble_op = (bubble_shown.clamp(0.0, 1.0) as f32) * compact_opacity;
    if bubble_op > 0.001
        && let Some(ref bitem) = app.bubble_item.clone()
    {
        let shift_x = ((width / 2.0 + BUBBLE_GAP + BUBBLE_WIDTH / 2.0) * bubble_shown) as f32;
        let bx = cx + shift_x - (BUBBLE_WIDTH as f32) / 2.0;
        let bw = BUBBLE_WIDTH as f32;
        let bh = BUBBLE_HEIGHT as f32;
        if bubble_shown > 0.7 && compact_opacity > 0.7 {
            hits.bubble_rect = Some(RectF {
                x: bx - EAR as f32,
                y: 0.0,
                w: bw + 2.0 * EAR as f32,
                h: bh,
            });
        }
        surf.push_layer(bx - 30.0, 0.0, bw + 60.0, bh + 40.0, bubble_op, None, None);
        if !no_shadow {
            surf.draw_soft_shadow(bx, 0.0, bw, bh, 20.0, 4.0, 0.4);
        }
        if let Some(bgeom) =
            factories.build_notch_geometry(BUBBLE_WIDTH, BUBBLE_HEIGHT, BUBBLE_HEIGHT / 2.0)
        {
            surf.set_transform(Matrix3x2::translation(bx - EAR as f32, 0.0));
            let g: ID2D1Geometry = bgeom.cast().unwrap();
            surf.fill_geometry(&g, NOTCH_BLACK);
            surf.set_transform(Matrix3x2::identity());
        }
        let content_op = if bitem.dim { 0.55 } else { 1.0 };
        let fg = if bitem.warning {
            WARNING_BRUSH
        } else {
            TEXT_PRIMARY
        }
        .with_alpha(content_op);
        let live_running = bitem.progress.is_none()
            && bitem.badge.is_none()
            && bitem.label.is_none()
            && bitem.activity_id.is_some()
            && !bitem.warning
            && !bitem.dim;
        let live_col = match bitem.glyph.as_str() {
            "mark:copilot" => COPILOT_BLUE,
            "mark:claude" => CLAUDE_ORANGE,
            _ => app.accent,
        };
        if live_running {
            surf.fill_radial_glow(
                bx + 16.0,
                16.0,
                18.0,
                14.0,
                live_col.with_alpha(0.28),
                Color::argb(0, 0, 0, 0),
            );
        }
        draw_icon_view(
            factories,
            surf,
            &bitem.glyph,
            None,
            bx + 9.5,
            5.0 + (22.0 - 13.0) / 2.0,
            13.0,
            fg,
        );
        let rx = bx + bw - 5.0 - 22.0;
        let ry = 5.0;
        if let Some(prog) = bitem.progress {
            surf.draw_ellipse(
                rx + 11.0,
                ry + 11.0,
                10.0,
                10.0,
                TRACK.with_alpha(content_op),
                2.0,
            );
            if let Some(arc) = factories.build_arc_geometry(prog, 22.0, 2.0) {
                surf.set_transform(Matrix3x2::translation(rx, ry));
                let ag: ID2D1Geometry = arc.cast().unwrap();
                surf.draw_geometry(&ag, fg, 2.0, Some(&factories.round_stroke));
                surf.set_transform(Matrix3x2::identity());
            }
        }
        if let Some(ref badge) = bitem.badge {
            surf.draw_text(
                factories,
                badge,
                rx,
                ry,
                22.0,
                22.0,
                FontKind::Icon,
                10.0,
                false,
                fg,
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
        } else if let Some(ref lbl) = bitem.label {
            let fsz = if lbl.len() > 2 { 8.0 } else { 10.0 };
            surf.draw_text(
                factories,
                lbl,
                rx,
                ry,
                22.0,
                22.0,
                FontKind::Ui,
                fsz,
                true,
                fg,
                HAlign::Center,
                VAlign::Center,
                true,
                false,
                false,
                false,
            );
        } else if live_running {
            surf.draw_ellipse(
                rx + 11.0,
                ry + 11.0,
                9.5,
                9.5,
                TRACK.with_alpha(content_op),
                1.8,
            );
            if let Some(arc) = factories.build_arc_geometry(0.34, 22.0, 2.0) {
                let deg = (app.agent_pulse_phase.to_degrees() % 360.0) as f32;
                surf.set_transform(
                    Matrix3x2::rotation_around(deg, Vector2::new(11.0, 11.0))
                        * Matrix3x2::translation(rx, ry),
                );
                let ag: ID2D1Geometry = arc.cast().unwrap();
                surf.draw_geometry(
                    &ag,
                    live_col.with_alpha(content_op),
                    2.0,
                    Some(&factories.round_stroke),
                );
                surf.set_transform(Matrix3x2::identity());
            }
            let pulse = 0.65 + 0.35 * (app.agent_pulse_phase as f32 * 1.4).sin();
            surf.fill_ellipse(
                rx + 11.0,
                ry + 11.0,
                2.6,
                2.6,
                live_col.with_alpha(pulse * content_op),
            );
        } else if bitem.progress.is_none() {
            surf.fill_ellipse(
                rx + 11.0,
                ry + 11.0,
                2.5,
                2.5,
                TEXT_SECONDARY.with_alpha(content_op),
            );
        }
        surf.pop_layer();
    }

    // 2. ActivityPanel (beside or below open notch)
    let panel_shown = if !app.activities.is_empty() {
        ((progress - 0.35) / 0.65).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    if panel_shown > 0.001 {
        let pw = app.panel_width as f32;
        let ph = app.panel_height() as f32;
        let (px, py) = if app.panel_below {
            (
                notch_x + w / 2.0 - pw / 2.0,
                (h + BUBBLE_GAP as f32) * panel_shown,
            )
        } else {
            (
                cx + ((width / 2.0 + BUBBLE_GAP + app.panel_width / 2.0) as f32) * panel_shown
                    - pw / 2.0,
                0.0,
            )
        };
        if panel_shown > 0.7 {
            hits.panel_rect = Some(RectF {
                x: px - EAR as f32,
                y: py,
                w: pw + 2.0 * EAR as f32,
                h: ph,
            });
        }
        surf.push_layer(
            px - 30.0,
            py - 10.0,
            pw + 60.0,
            ph + 50.0,
            panel_shown,
            None,
            None,
        );
        if !no_shadow {
            surf.draw_soft_shadow(px, py, pw, ph, 20.0, 4.0, 0.4);
        }
        if app.panel_below {
            surf.fill_rounded_rect(px, py, pw, ph, 16.0, 16.0, NOTCH_BLACK);
        } else if let Some(pgeom) =
            factories.build_notch_geometry(app.panel_width, f64::from(ph), 16.0)
        {
            surf.set_transform(Matrix3x2::translation(px - EAR as f32, py));
            let g: ID2D1Geometry = pgeom.cast().unwrap();
            surf.fill_geometry(&g, NOTCH_BLACK);
            surf.set_transform(Matrix3x2::identity());
        }
        let rows = app.activities.len().min(4);
        let sx = px + 14.0;
        let sw = pw - 24.0;
        for i in 0..rows {
            let act = &app.activities[i];
            let ry = py + 8.0 + (i as f32) * 30.0;
            let fg = if act.warning {
                WARNING_BRUSH
            } else {
                TEXT_PRIMARY
            };
            let is_running = act.state == "Running";
            if is_running && act.progress.is_none() {
                let pulse_col = match act.glyph.as_str() {
                    "mark:copilot" => COPILOT_BLUE,
                    "mark:claude" => CLAUDE_ORANGE,
                    _ => app.accent,
                };
                let pulse = 0.55 + 0.45 * (app.agent_pulse_phase as f32 * 1.4).sin();
                surf.fill_radial_glow(
                    sx + 15.0,
                    ry + 15.0,
                    13.0,
                    13.0,
                    pulse_col.with_alpha(0.28 * pulse),
                    Color::argb(0, 0, 0, 0),
                );
            }
            draw_icon_view(
                factories,
                surf,
                &act.glyph,
                act.badge.as_deref(),
                sx + (30.0 - 13.0) / 2.0,
                ry + (30.0 - 13.0) / 2.0,
                13.0,
                fg,
            );
            let mut btn_x = sx + sw - 24.0;
            let mut right_used = 26.0;
            draw_icon_button(
                factories,
                surf,
                btn_x,
                ry + 3.0,
                24.0,
                "\u{E711}",
                9.5,
                TEXT_SECONDARY,
                app.hover == HoverTarget::StripDismiss(i),
                app.pressed == HoverTarget::StripDismiss(i),
            );
            if panel_shown > 0.7 {
                hits.push(
                    RectF {
                        x: btn_x,
                        y: ry + 3.0,
                        w: 24.0,
                        h: 24.0,
                    },
                    HoverTarget::StripDismiss(i),
                );
            }
            if act.window.is_some() {
                if is_running {
                    btn_x -= 25.0;
                    right_used += 25.0;
                    draw_icon_button(
                        factories,
                        surf,
                        btn_x,
                        ry + 3.0,
                        24.0,
                        "\u{E71A}",
                        9.5,
                        WARNING_BRUSH.with_alpha(0.9),
                        app.hover == HoverTarget::StripStop(i),
                        app.pressed == HoverTarget::StripStop(i),
                    );
                    if panel_shown > 0.7 {
                        hits.push(
                            RectF {
                                x: btn_x,
                                y: ry + 3.0,
                                w: 24.0,
                                h: 24.0,
                            },
                            HoverTarget::StripStop(i),
                        );
                    }
                }
                if act.icon.eq_ignore_ascii_case("copilot") {
                    btn_x -= 25.0;
                    right_used += 25.0;
                    draw_icon_button(
                        factories,
                        surf,
                        btn_x,
                        ry + 3.0,
                        24.0,
                        "\u{E8BD}",
                        10.5,
                        COPILOT_BLUE,
                        app.hover == HoverTarget::StripChat(i),
                        app.pressed == HoverTarget::StripChat(i),
                    );
                    if panel_shown > 0.7 {
                        hits.push(
                            RectF {
                                x: btn_x,
                                y: ry + 3.0,
                                w: 24.0,
                                h: 24.0,
                            },
                            HoverTarget::StripChat(i),
                        );
                    }
                }
                btn_x -= 25.0;
                right_used += 25.0;
                draw_icon_button(
                    factories,
                    surf,
                    btn_x,
                    ry + 3.0,
                    24.0,
                    "\u{E8A7}",
                    10.5,
                    TEXT_SECONDARY,
                    app.hover == HoverTarget::StripGo(i),
                    app.pressed == HoverTarget::StripGo(i),
                );
                if panel_shown > 0.7 {
                    hits.push(
                        RectF {
                            x: btn_x,
                            y: ry + 3.0,
                            w: 24.0,
                            h: 24.0,
                        },
                        HoverTarget::StripGo(i),
                    );
                }
            }
            if let Some(prog) = act.progress {
                let ptxt = format!("{:.0}%", prog * 100.0);
                let pw_txt = factories
                    .measure_text(&ptxt, FontKind::Ui, 11.0, false, true, 60.0, false)
                    .0;
                surf.draw_text(
                    factories,
                    &ptxt,
                    sx + sw - right_used - pw_txt - 4.0,
                    ry,
                    pw_txt + 4.0,
                    30.0,
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
                right_used += pw_txt + 12.0;
            }
            let line_w = (sw - 30.0 - right_used).max(10.0);
            surf.draw_text(
                factories,
                &act.line,
                sx + 30.0,
                ry,
                line_w,
                30.0,
                FontKind::Ui,
                12.0,
                false,
                fg,
                HAlign::Left,
                VAlign::Center,
                false,
                false,
                true,
                false,
            );
        }
        let mut extra_y = py + 8.0 + (rows as f32) * 30.0;
        if app.activities.len() > rows {
            let more = fmt1(
                app.lang.strings().activity_more,
                &(app.activities.len() - rows).to_string(),
            );
            surf.draw_text(
                factories,
                &more,
                sx + 30.0,
                extra_y,
                sw - 30.0,
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
            extra_y += 18.0;
        }
        if app.has_agent_prompt_bar() {
            let bar_y = extra_y + 4.0;
            let bar_h = 28.0_f32;
            let new_x = sx + sw - 26.0;
            let send_x = new_x - 28.0;
            let input_w = (send_x - sx - 4.0).max(60.0);
            let border_col = if app.agent_prompt_focused {
                COPILOT_BLUE.with_alpha(0.75)
            } else {
                Color::argb(0x22, 0xFF, 0xFF, 0xFF)
            };
            surf.fill_rounded_rect(sx, bar_y, input_w, bar_h, 14.0, 14.0, SUBTLE_FILL);
            surf.draw_rounded_rect(sx, bar_y, input_w, bar_h, 14.0, 14.0, border_col, 1.0, None);
            draw_icon_view(
                factories,
                surf,
                "mark:copilot",
                None,
                sx + 8.0,
                bar_y + 7.0,
                14.0,
                COPILOT_BLUE,
            );
            let txt_x = sx + 27.0;
            let txt_w = (input_w - 35.0).max(20.0);
            if app.agent_prompt_input.is_empty() {
                surf.draw_text(
                    factories,
                    "Ask Copilot…",
                    txt_x,
                    bar_y,
                    txt_w,
                    bar_h,
                    FontKind::Ui,
                    11.5,
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
                    &app.agent_prompt_input,
                    txt_x,
                    bar_y,
                    txt_w,
                    bar_h,
                    FontKind::Ui,
                    11.5,
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
            if app.agent_prompt_focused {
                let caret_x = txt_x
                    + factories
                        .measure_text(
                            &app.agent_prompt_input,
                            FontKind::Ui,
                            11.5,
                            false,
                            false,
                            txt_w,
                            false,
                        )
                        .0
                        .min(txt_w - 2.0);
                surf.fill_rect(caret_x, bar_y + 7.0, 1.5, 14.0, COPILOT_BLUE);
            }
            draw_icon_button(
                factories,
                surf,
                send_x,
                bar_y + 1.0,
                26.0,
                "\u{E724}",
                11.5,
                if app.agent_prompt_input.trim().is_empty() {
                    TEXT_SECONDARY
                } else {
                    COPILOT_BLUE
                },
                app.hover == HoverTarget::AgentPromptSend,
                app.pressed == HoverTarget::AgentPromptSend,
            );
            draw_icon_button(
                factories,
                surf,
                new_x,
                bar_y + 1.0,
                26.0,
                "\u{E710}",
                11.5,
                TEXT_SECONDARY,
                app.hover == HoverTarget::AgentNewChat,
                app.pressed == HoverTarget::AgentNewChat,
            );
            if panel_shown > 0.7 {
                hits.push(
                    RectF {
                        x: sx,
                        y: bar_y,
                        w: input_w,
                        h: bar_h,
                    },
                    HoverTarget::AgentPromptInput,
                );
                hits.push(
                    RectF {
                        x: send_x,
                        y: bar_y + 1.0,
                        w: 26.0,
                        h: 26.0,
                    },
                    HoverTarget::AgentPromptSend,
                );
                hits.push(
                    RectF {
                        x: new_x,
                        y: bar_y + 1.0,
                        w: 26.0,
                        h: 26.0,
                    },
                    HoverTarget::AgentNewChat,
                );
            }
        }
        surf.pop_layer();
    }

    // 3. Main Notch Shadow + Shape
    hits.notch_rect = Some(RectF {
        x: notch_outer_x,
        y: 0.0,
        w: w + 2.0 * EAR as f32,
        h,
    });
    if !no_shadow {
        surf.draw_soft_shadow(notch_x, 0.0, w, h, 28.0, 6.0, 0.45);
    }
    let notch_geom = factories.build_notch_geometry(width, height, radius);
    if let Some(ref geom) = notch_geom {
        surf.set_transform(Matrix3x2::translation(notch_outer_x, 0.0));
        let g: ID2D1Geometry = geom.cast().unwrap();
        surf.fill_geometry(&g, NOTCH_BLACK);
        surf.set_transform(Matrix3x2::identity());
    }

    // Clip ContentHost to rounded notch geometry (or rect fallback)
    if let Some(ref geom) = notch_geom {
        let g: ID2D1Geometry = geom.cast().unwrap();
        surf.push_layer_masked(
            notch_x,
            0.0,
            w,
            h,
            1.0,
            Some(&g),
            Matrix3x2::translation(notch_outer_x, 0.0),
            None,
        );
    } else {
        surf.push_clip(notch_x, 0.0, w, h);
    }

    // 4. CompactView
    let approach_vis = (app.drag_approach as f32).clamp(0.0, 1.0) * compact_opacity;
    let compact_fade = (1.0 - approach_vis * 2.2).clamp(0.0, 1.0);
    let compact_vis = under_overlay * (1.0 - ambient) * compact_fade;
    if compact_vis > 0.001 {
        surf.push_layer(notch_x, 0.0, w, h, compact_vis, None, None);
        draw_compact_view(app, factories, surf, notch_x, w, h);
        surf.pop_layer();
    }

    // 5. AmbientView (Pure Eyes + Kaomoji + Zzz)
    let ambient_vis = under_overlay * ambient;
    if ambient_vis > 0.001 {
        surf.push_layer(notch_x, 0.0, w, h, ambient_vis, None, None);
        draw_robo_visor(app, factories, surf, notch_x, w, h);
        if app.visor_sleeping {
            let z_y = 1.0 + (3.0 - app.zzz_step as f32);
            let z_op = (1.0 - (app.zzz_step as f32) / 9.0).clamp(0.0, 1.0);
            surf.draw_text(
                factories,
                "z",
                notch_x + w - 18.0,
                z_y,
                12.0,
                14.0,
                FontKind::Ui,
                9.0,
                true,
                TEXT_SECONDARY.with_alpha(z_op),
                HAlign::Right,
                VAlign::Top,
                false,
                false,
                false,
                false,
            );
        }
        surf.pop_layer();
    }

    // 5b. Magnetic Drag Approach (when dragging a file toward the collapsed notch)
    if approach_vis > 0.01 {
        let glow_col = app.accent.lerp(TEXT_PRIMARY, 0.35);
        let spot_x = app.drag_cursor_dip.map_or(cx, |(x, _)| {
            (x as f32).clamp(notch_x + 14.0, notch_x + w - 14.0)
        });
        surf.fill_radial_glow(
            spot_x,
            h - 2.0,
            84.0,
            30.0,
            glow_col.with_alpha(0.45 * approach_vis),
            Color::argb(0, 0, 0, 0),
        );
        if let Some(ref geom) = notch_geom {
            surf.push_clip(notch_outer_x, 1.5, w + 2.0 * EAR as f32, h);
            surf.set_transform(Matrix3x2::translation(notch_outer_x, 0.0));
            let g: ID2D1Geometry = geom.cast().unwrap();
            surf.draw_geometry(&g, glow_col.with_alpha(0.78 * approach_vis), 1.6, None);
            surf.set_transform(Matrix3x2::identity());
            surf.pop_clip();
        }
        let pill_fade = ((approach_vis - 0.25) / 0.75).clamp(0.0, 1.0);
        if pill_fade > 0.01 {
            let bob = (app.drag_phase as f32 * 1.6).sin() * 1.8 * pill_fade;
            let mut peek_anim = CompanionAnimator::default();
            peek_anim.perch();
            if let Some((cx_dip, cy_dip)) = app.drag_cursor_dip {
                peek_anim.eye_shift_x =
                    ((cx_dip - f64::from(notch_x + 22.0)) / 12.0).clamp(-12.0, 12.0);
                peek_anim.eye_shift_y = ((cy_dip - f64::from(h * 0.5)) / 10.0).clamp(-6.0, 8.0);
            }
            peek_anim.eye_scale_x = 1.12;
            peek_anim.eye_scale_y = 1.15;
            surf.push_layer(notch_x, 0.0, w, h, pill_fade, None, None);
            draw_companion_sprite(
                factories,
                surf,
                &peek_anim,
                notch_x + 12.0,
                (h - 22.0) * 0.5,
                22.0,
            );
            surf.draw_text(
                factories,
                app.lang.strings().shelf_drop_to_keep,
                notch_x + 38.0,
                0.0,
                (w - 76.0).max(40.0),
                h,
                FontKind::Ui,
                13.0,
                true,
                TEXT_PRIMARY,
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                true,
                false,
            );
            surf.draw_text(
                factories,
                "\u{E896}",
                notch_x + w - 30.0,
                bob,
                18.0,
                h,
                FontKind::Icon,
                14.0,
                false,
                glow_col.lerp(TEXT_PRIMARY, 0.4),
                HAlign::Center,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
            surf.pop_layer();
        }
    }

    // 5c. Drop Celebration Rim Flash on Notch Contour
    if app.drop_burst_t < 0.55
        && let Some(ref geom) = notch_geom
    {
        let rim_t = (app.drop_burst_t as f32 / 0.55).clamp(0.0, 1.0);
        let rim_alpha = (1.0 - rim_t).powi(2) * 0.85;
        let rim_col = app.accent.lerp(TEXT_PRIMARY, 0.45).with_alpha(rim_alpha);
        surf.push_clip(notch_outer_x, 1.5, w + 2.0 * EAR as f32, h);
        surf.set_transform(Matrix3x2::translation(notch_outer_x, 0.0));
        let g: ID2D1Geometry = geom.cast().unwrap();
        surf.draw_geometry(&g, rim_col, 2.0, None);
        surf.set_transform(Matrix3x2::identity());
        surf.pop_clip();
    }

    // 6. PrivacyDots
    let dots_op = compact_opacity
        * if app.overlay_is_news {
            1.0 - overlay
        } else {
            1.0
        };
    if app.privacy_shown() && dots_op > 0.001 {
        let mut dx = notch_x + 14.0;
        let dy = 13.0;
        if !app.camera_apps.is_empty() {
            surf.fill_ellipse(
                dx + 3.0,
                dy + 3.0,
                3.0,
                3.0,
                CAMERA_BRUSH.with_alpha(dots_op),
            );
            dx += 9.0;
        }
        if !app.mic_apps.is_empty() {
            let mc = MICROPHONE_BRUSH.with_alpha(dots_op);
            if app.mic_muted {
                surf.draw_ellipse(dx + 3.0, dy + 3.0, 2.25, 2.25, mc, 1.5);
            } else {
                surf.fill_ellipse(dx + 3.0, dy + 3.0, 3.0, 3.0, mc);
            }
        }
    }

    // 7. HudView
    let hud_op = if app.overlay_is_news {
        0.0
    } else {
        compact_opacity * overlay
    };
    if hud_op > 0.001
        && let Some(ref msg) = app.hud_queue.current.clone()
    {
        surf.push_layer(notch_x, 0.0, w, h, hud_op, None, None);
        draw_hud_view(app, factories, surf, msg, notch_x, w, h);
        surf.pop_layer();
    }

    // 8. NewsView
    let news_op = if app.overlay_is_news {
        compact_opacity * overlay
    } else {
        0.0
    };
    if news_op > 0.001
        && let Some(ref report) = app.news_report.clone()
    {
        surf.push_layer(notch_x, 0.0, w, h, news_op, None, None);
        draw_news_view(app, factories, surf, report, notch_x, w, h);
        surf.pop_layer();
    }

    // 9. ExpandedView
    if expanded_opacity > 0.001 {
        let exp_w = match app.page {
            NotchPage::Agenda
            | NotchPage::Pray
            | NotchPage::Tasks
            | NotchPage::Shelf
            | NotchPage::Notifications
            | NotchPage::Clipboard
            | NotchPage::Weather => EXPANDED_WIDE.0 as f32,
            NotchPage::Media if app.lyrics.is_some() => EXPANDED_WIDE.0 as f32,
            _ => EXPANDED_WIDTH as f32,
        };
        let exp_h = EXPANDED_SHORT.1 as f32;
        let ex = cx - exp_w / 2.0;
        surf.push_layer(ex, 0.0, exp_w, exp_h, expanded_opacity, None, None);
        draw_expanded_view(
            app,
            factories,
            surf,
            &mut hits,
            ex,
            exp_w,
            exp_h,
            progress > 0.7,
        );
        surf.pop_layer();
    }

    if notch_geom.is_some() {
        surf.pop_layer();
    } else {
        surf.pop_clip();
    }
    hits
}

#[expect(
    clippy::too_many_arguments,
    reason = "renders full expanded notch view"
)]
fn draw_expanded_view(
    app: &mut NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    hits: &mut HitMap,
    ex: f32,
    ew: f32,
    eh: f32,
    interactive: bool,
) {
    // Top Bar (y = 8 .. 36): Left Cozmo+Page Pill, Center Live Clock Pill, Right Copilot+Settings
    let hx = ex + 18.0;
    let hy = 8.0;
    let hw = ew - 36.0;
    let hh = 28.0;
    let accent_glow = app.accent.lerp(TEXT_PRIMARY, 0.3);

    // Center: Live Clock Pill (HH:MM:SS + Date + Notification Bell)
    let now = Local::now();
    let today = now.date_naive();
    let time_str = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    let date_str = if ew >= 600.0 {
        app.lang.format_long_date(today)
    } else {
        app.lang.format_weekday_date(today)
    };
    let unread_notifs = app.unread_notifications_count();
    let notif_count_str = if unread_notifs > 99 {
        "99+".to_string()
    } else {
        unread_notifs.to_string()
    };
    let time_w = factories
        .measure_text(&time_str, FontKind::Ui, 12.5, true, true, 100.0, false)
        .0;
    let date_w = factories
        .measure_text(&date_str, FontKind::Ui, 11.0, false, false, 180.0, false)
        .0
        .min(if ew >= 600.0 { 170.0 } else { 92.0 });
    let notif_count_w = if unread_notifs > 0 {
        factories
            .measure_text(
                &notif_count_str,
                FontKind::Ui,
                10.5,
                true,
                true,
                40.0,
                false,
            )
            .0
    } else {
        0.0
    };
    let notif_seg_w = if unread_notifs > 0 {
        (16.0 + notif_count_w + 10.0).max(32.0)
    } else {
        24.0
    };
    let clock_h = 26.0_f32;
    let clock_y = hy + (hh - clock_h) * 0.5;
    let clock_w =
        (20.0 + time_w + 10.0 + date_w + 8.0 + notif_seg_w + 4.0).min((ew - 190.0).max(140.0));
    let clock_x = ex + (ew - clock_w) * 0.5;

    surf.fill_rounded_rect_radial_glow(
        clock_x,
        clock_y,
        clock_w,
        clock_h,
        13.0,
        clock_x + clock_w * 0.5,
        clock_y + clock_h * 0.5,
        clock_w * 0.6,
        clock_h * 0.9,
        accent_glow.with_alpha(0.16),
        Color::argb(0, 0, 0, 0),
    );
    surf.fill_rounded_rect(
        clock_x,
        clock_y,
        clock_w,
        clock_h,
        13.0,
        13.0,
        Color::argb(0x14, 0xFF, 0xFF, 0xFF),
    );
    surf.draw_rounded_rect(
        clock_x,
        clock_y,
        clock_w,
        clock_h,
        13.0,
        13.0,
        Color::argb(0x22, 0xFF, 0xFF, 0xFF),
        1.0,
        None,
    );
    let pulse = if app.reduce_motion {
        0.85
    } else {
        let phase = (now.timestamp_subsec_millis() as f32 / 1000.0) * std::f32::consts::TAU;
        0.55 + 0.45 * phase.sin()
    };
    surf.fill_ellipse(
        clock_x + 11.0,
        clock_y + clock_h * 0.5,
        2.4,
        2.4,
        accent_glow.with_alpha(pulse),
    );
    let tx_clock = clock_x + 18.0;
    surf.draw_text(
        factories,
        &time_str,
        tx_clock,
        clock_y,
        time_w + 2.0,
        clock_h,
        FontKind::Ui,
        12.5,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );
    let sep_x = tx_clock + time_w + 4.0;
    surf.fill_ellipse(
        sep_x,
        clock_y + clock_h * 0.5,
        1.2,
        1.2,
        TEXT_SECONDARY.with_alpha(0.6),
    );
    let notif_x = clock_x + clock_w - notif_seg_w - 3.0;
    let dx_clock = sep_x + 5.0;
    let dw_avail = (notif_x - 6.0 - dx_clock).max(10.0);
    surf.draw_text(
        factories,
        &date_str,
        dx_clock,
        clock_y,
        dw_avail,
        clock_h,
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

    // Notification bell segment inside Live Clock Pill
    surf.fill_rect(
        notif_x - 3.0,
        clock_y + 6.0,
        1.0,
        clock_h - 12.0,
        Color::argb(0x22, 0xFF, 0xFF, 0xFF),
    );
    let notif_hovered = app.hover == HoverTarget::HeaderNotifBtn;
    let notif_pressed = app.pressed == HoverTarget::HeaderNotifBtn;
    let notif_on_page = app.page == NotchPage::Notifications;
    let notif_bg = if notif_pressed {
        PRESSED_FILL
    } else if notif_hovered {
        HOVER_FILL
    } else if unread_notifs > 0 {
        WARNING_BRUSH.with_alpha(0.18)
    } else if notif_on_page {
        accent_glow.with_alpha(0.20)
    } else {
        Color::argb(0, 0, 0, 0)
    };
    if notif_bg.a > 0.001 {
        surf.fill_rounded_rect(
            notif_x,
            clock_y + 2.0,
            notif_seg_w,
            clock_h - 4.0,
            11.0,
            11.0,
            notif_bg,
        );
    }
    let bell_col = if unread_notifs > 0 {
        WARNING_BRUSH
    } else if notif_hovered || notif_on_page {
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    };
    let bell_x = if unread_notifs > 0 {
        notif_x + 5.0
    } else {
        notif_x + (notif_seg_w - 14.0) * 0.5
    };
    let bell_pulse = if unread_notifs > 0 && app.notif_pulse_t > 0.005 {
        1.0 + 0.18 * (app.notif_pulse_t as f32)
    } else {
        1.0
    };
    if (bell_pulse - 1.0).abs() > 0.002 {
        surf.set_transform(Matrix3x2::scale_around(
            bell_pulse,
            bell_pulse,
            Vector2::new(bell_x + 7.0, clock_y + clock_h * 0.5),
        ));
    }
    surf.draw_text(
        factories,
        "\u{EA8F}",
        bell_x,
        clock_y,
        14.0,
        clock_h,
        FontKind::Icon,
        11.5,
        false,
        bell_col,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    if (bell_pulse - 1.0).abs() > 0.002 {
        surf.set_transform(Matrix3x2::identity());
    }
    if unread_notifs > 0 {
        surf.draw_text(
            factories,
            &notif_count_str,
            bell_x + 14.0,
            clock_y,
            notif_count_w + 4.0,
            clock_h,
            FontKind::Ui,
            10.5,
            true,
            WARNING_BRUSH.lerp(TEXT_PRIMARY, 0.35),
            HAlign::Left,
            VAlign::Center,
            true,
            false,
            false,
            false,
        );
    }
    if interactive {
        hits.push(
            RectF {
                x: notif_x,
                y: clock_y,
                w: notif_seg_w + 3.0,
                h: clock_h,
            },
            HoverTarget::HeaderNotifBtn,
        );
    }

    // Right header: Optional Copilot button + Rotating Settings Gear
    let gear_size = 26.0_f32;
    let gear_x = hx + hw - gear_size;
    let gear_y = hy + (hh - gear_size) * 0.5;
    let gh = app.gear_hover_anim as f32;
    let gear_pressed = app.pressed == HoverTarget::SettingsBtn;
    let gear_bg = if gear_pressed {
        PRESSED_FILL
    } else {
        Color::argb(0x14, 0xFF, 0xFF, 0xFF).lerp(HOVER_FILL, gh)
    };
    let gear_border = Color::argb(0x1F, 0xFF, 0xFF, 0xFF).lerp(accent_glow.with_alpha(0.55), gh);
    surf.fill_ellipse(
        gear_x + gear_size * 0.5,
        gear_y + gear_size * 0.5,
        gear_size * 0.5,
        gear_size * 0.5,
        gear_bg,
    );
    surf.draw_ellipse(
        gear_x + gear_size * 0.5,
        gear_y + gear_size * 0.5,
        gear_size * 0.5 - 0.5,
        gear_size * 0.5 - 0.5,
        gear_border,
        1.0,
    );
    let gear_cx = gear_x + gear_size * 0.5;
    let gear_cy = gear_y + gear_size * 0.5;
    if gh > 0.005 {
        let g_scale = 1.0 + 0.12 * gh;
        surf.set_transform(
            Matrix3x2::rotation_around(gh * 45.0, Vector2::new(gear_cx, gear_cy))
                * Matrix3x2::scale_around(g_scale, g_scale, Vector2::new(gear_cx, gear_cy)),
        );
    }
    surf.draw_text(
        factories,
        "\u{E713}",
        gear_x,
        gear_y,
        gear_size,
        gear_size,
        FontKind::Icon,
        12.5,
        false,
        TEXT_SECONDARY.lerp(TEXT_PRIMARY, gh),
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    if gh > 0.005 {
        surf.set_transform(Matrix3x2::identity());
    }
    if interactive {
        hits.push(
            RectF {
                x: gear_x,
                y: gear_y,
                w: gear_size,
                h: gear_size,
            },
            HoverTarget::SettingsBtn,
        );
    }

    let show_copilot_btn = app.copilot_hooks_on && !app.privacy_shown();
    if show_copilot_btn {
        let cx_btn = gear_x - 32.0;
        let cy_btn = gear_y;
        let hovered = app.hover == HoverTarget::HeaderCopilotBtn;
        let pressed = app.pressed == HoverTarget::HeaderCopilotBtn;
        let bg = if pressed {
            PRESSED_FILL
        } else if hovered {
            HOVER_FILL
        } else {
            Color::argb(0x14, 0xFF, 0xFF, 0xFF)
        };
        let border = if hovered {
            COPILOT_BLUE.with_alpha(0.65)
        } else {
            Color::argb(0x1F, 0xFF, 0xFF, 0xFF)
        };
        surf.fill_ellipse(cx_btn + 13.0, cy_btn + 13.0, 13.0, 13.0, bg);
        surf.draw_ellipse(cx_btn + 13.0, cy_btn + 13.0, 12.5, 12.5, border, 1.0);
        draw_icon_view(
            factories,
            surf,
            "mark:copilot",
            None,
            cx_btn + 6.5,
            cy_btn + 6.5,
            13.0,
            COPILOT_BLUE,
        );
        if interactive {
            hits.push(
                RectF {
                    x: cx_btn,
                    y: cy_btn,
                    w: 26.0,
                    h: 26.0,
                },
                HoverTarget::HeaderCopilotBtn,
            );
        }
    }

    // Left header: Cozmo + Active Page Pill (or Privacy Pill) + optional RefreshButton
    let show_refresh = (app.page == NotchPage::Agenda
        && app.google_auth.lock().is_ok_and(|g| g.is_signed_in()))
        || (app.page == NotchPage::Tasks && app.store.data.settings.tasks_sync_enabled)
        || (app.page == NotchPage::Weather
            && app.store.data.settings.weather_latitude.is_some()
            && app.store.data.settings.weather_longitude.is_some());
    let extra_left_btns = if show_refresh { 32.0 } else { 0.0 };
    let left_max_w = (clock_x - hx - 8.0 - extra_left_btns).max(56.0);
    let pill_h_left = 26.0_f32;
    let pill_y_left = hy + (hh - pill_h_left) * 0.5;

    let used_left = if app.privacy_shown() {
        let mut content_w = 24.0_f32;
        let cam_txt = if !app.camera_apps.is_empty() {
            Some(app.camera_apps.join(", "))
        } else {
            None
        };
        let mic_txt = if !app.mic_apps.is_empty() {
            Some(app.mic_apps.join(", "))
        } else {
            None
        };
        let cam_w = cam_txt.as_ref().map_or(0.0, |t| {
            16.0 + factories
                .measure_text(t, FontKind::Ui, 11.5, false, false, 100.0, false)
                .0
                .min(100.0)
        });
        let mic_w = mic_txt.as_ref().map_or(0.0, |t| {
            16.0 + factories
                .measure_text(t, FontKind::Ui, 11.5, false, false, 100.0, false)
                .0
                .min(100.0)
        });
        if cam_w > 0.0 {
            content_w += cam_w + if mic_w > 0.0 { 10.0 } else { 0.0 };
        }
        content_w += mic_w;
        let pw_left = (content_w + 14.0).min(left_max_w);
        surf.fill_rounded_rect(
            hx,
            pill_y_left,
            pw_left,
            pill_h_left,
            13.0,
            13.0,
            Color::argb(0x14, 0xFF, 0xFF, 0xFF),
        );
        surf.draw_rounded_rect(
            hx,
            pill_y_left,
            pw_left,
            pill_h_left,
            13.0,
            13.0,
            Color::argb(0x1F, 0xFF, 0xFF, 0xFF),
            1.0,
            None,
        );
        let mut cozmo = app.header_cozmo.clone();
        if cozmo.mode == CompanionMode::Still {
            cozmo.perch();
        }
        draw_companion_sprite(factories, surf, &cozmo, hx + 7.0, pill_y_left + 4.0, 18.0);
        let mut px = hx + 29.0;
        let max_end = hx + pw_left - 8.0;
        if let Some(ref txt) = cam_txt {
            surf.draw_text(
                factories,
                "\u{E722}",
                px,
                pill_y_left,
                14.0,
                pill_h_left,
                FontKind::Icon,
                11.5,
                false,
                CAMERA_BRUSH,
                HAlign::Left,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
            px += 16.0;
            let rem = (max_end - px).max(10.0);
            let tw = factories
                .measure_text(txt, FontKind::Ui, 11.5, false, false, rem, false)
                .0
                .min(rem);
            surf.draw_text(
                factories,
                txt,
                px,
                pill_y_left,
                tw,
                pill_h_left,
                FontKind::Ui,
                11.5,
                false,
                TEXT_PRIMARY,
                HAlign::Left,
                VAlign::Center,
                false,
                false,
                true,
                false,
            );
            px += tw + 10.0;
        }
        if let Some(ref txt) = mic_txt
            && px + 16.0 < max_end
        {
            let mg = if app.mic_muted {
                "\u{EC54}"
            } else {
                "\u{E720}"
            };
            surf.draw_text(
                factories,
                mg,
                px,
                pill_y_left,
                14.0,
                pill_h_left,
                FontKind::Icon,
                11.5,
                false,
                MICROPHONE_BRUSH,
                HAlign::Left,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
            px += 16.0;
            let rem = (max_end - px).max(10.0);
            let tw = factories
                .measure_text(txt, FontKind::Ui, 11.5, false, false, rem, false)
                .0
                .min(rem);
            surf.draw_text(
                factories,
                txt,
                px,
                pill_y_left,
                tw,
                pill_h_left,
                FontKind::Ui,
                11.5,
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
        pw_left
    } else {
        let page_name = app.tab_label(app.page as usize);
        let nw = factories
            .measure_text(
                page_name,
                FontKind::Ui,
                11.5,
                true,
                false,
                (left_max_w - 38.0).max(20.0),
                false,
            )
            .0
            .min((left_max_w - 38.0).max(20.0));
        let pw_left = (30.0 + nw + 12.0).min(left_max_w);
        surf.fill_rounded_rect(
            hx,
            pill_y_left,
            pw_left,
            pill_h_left,
            13.0,
            13.0,
            Color::argb(0x14, 0xFF, 0xFF, 0xFF),
        );
        surf.draw_rounded_rect(
            hx,
            pill_y_left,
            pw_left,
            pill_h_left,
            13.0,
            13.0,
            Color::argb(0x1F, 0xFF, 0xFF, 0xFF),
            1.0,
            None,
        );
        let mut cozmo = app.header_cozmo.clone();
        if cozmo.mode == CompanionMode::Still {
            cozmo.perch();
        }
        draw_companion_sprite(factories, surf, &cozmo, hx + 7.0, pill_y_left + 4.0, 18.0);
        surf.draw_text(
            factories,
            page_name,
            hx + 29.0,
            pill_y_left,
            (pw_left - 38.0).max(10.0),
            pill_h_left,
            FontKind::Ui,
            11.5,
            true,
            TEXT_PRIMARY.with_alpha(0.92),
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
        pw_left
    };

    if show_refresh {
        let rx = hx + used_left + 6.0;
        let ry = pill_y_left;
        surf.fill_ellipse(
            rx + 13.0,
            ry + 13.0,
            13.0,
            13.0,
            Color::argb(0x14, 0xFF, 0xFF, 0xFF),
        );
        surf.draw_ellipse(
            rx + 13.0,
            ry + 13.0,
            12.5,
            12.5,
            Color::argb(0x1F, 0xFF, 0xFF, 0xFF),
            1.0,
        );
        if app.refresh_angle > 0.1 {
            let rcx = rx + 13.0;
            let rcy = ry + 13.0;
            surf.set_transform(Matrix3x2::rotation_around(
                app.refresh_angle as f32,
                Vector2::new(rcx, rcy),
            ));
        }
        draw_icon_button(
            factories,
            surf,
            rx,
            ry,
            26.0,
            "\u{E72C}",
            11.5,
            TEXT_SECONDARY,
            app.hover == HoverTarget::RefreshBtn,
            app.pressed == HoverTarget::RefreshBtn,
        );
        surf.set_transform(Matrix3x2::identity());
        if interactive {
            hits.push(
                RectF {
                    x: rx,
                    y: ry,
                    w: 26.0,
                    h: 26.0,
                },
                HoverTarget::RefreshBtn,
            );
        }
    }

    // Subtle top hairline + soft accent glow under centered Live Clock
    surf.fill_rect(hx, 39.0, hw, 1.0, Color::argb(0x12, 0xFF, 0xFF, 0xFF));
    surf.push_clip(clock_x, 38.0, clock_w, 2.5);
    surf.fill_radial_glow(
        clock_x + clock_w * 0.5,
        39.0,
        clock_w * 0.65,
        2.5,
        accent_glow.with_alpha(0.55),
        Color::argb(0, 0, 0, 0),
    );
    surf.pop_clip();

    // Page content area (y = 42 .. eh - 38) with slide-in animation
    let py = 42.0;
    let ph = (eh - 80.0).max(100.0);
    let slide_ease = 1.0 - (1.0 - app.page_slide_t as f32).powi(3);
    let slide_x = if app.page_slide_t < 1.0 {
        18.0 * (app.page_slide_dir as f32) * (1.0 - slide_ease)
    } else {
        0.0
    };
    let slide_op = if app.page_slide_t < 1.0 {
        slide_ease
    } else {
        1.0
    };
    if slide_op < 0.999 {
        surf.push_layer(ex, py, ew, ph, slide_op, None, None);
    }
    let px = ex + slide_x;
    match app.page {
        NotchPage::Media => {
            draw_media_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Agenda => {
            draw_agenda_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Focus => {
            draw_focus_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Water => {
            draw_water_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Pray => draw_pray_page(app, factories, surf, hits, px, py, ew, ph, interactive),
        NotchPage::Tasks => {
            draw_tasks_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Shelf => {
            draw_shelf_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Notifications => {
            draw_notifications_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Clipboard => {
            draw_clipboard_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
        NotchPage::Weather => {
            draw_weather_page(app, factories, surf, hits, px, py, ew, ph, interactive)
        }
    }
    if slide_op < 0.999 {
        surf.pop_layer();
    }

    // Bottom-Corner Curve Anchors (y = eh - 34 .. eh - 8)
    let tab_glyphs = [
        "\u{E8D6}", "\u{E787}", "\u{E916}", "\u{EB42}", "\u{E708}", "\u{E9D5}", "\u{E7B8}",
        "\u{EA8F}", "\u{E77F}", "\u{E753}",
    ];
    let dock_pad = 3.0_f32;
    let tab_gap = 2.0_f32;
    let bh = 26.0_f32;
    let by = eh - 34.0;
    let slot_h = 20.0_f32;
    let slot_y = by + (bh - slot_h) * 0.5;
    let slot_w: [f32; 10] = std::array::from_fn(|i| app.tab_slot_w[i].value.max(22.0) as f32);
    let left_dock_w = slot_w[0..5].iter().sum::<f32>() + 4.0 * tab_gap + 2.0 * dock_pad;
    let right_dock_w = slot_w[5..10].iter().sum::<f32>() + 4.0 * tab_gap + 2.0 * dock_pad;
    let left_dock_x = ex + 16.0;
    let right_dock_x = ex + ew - 16.0 - right_dock_w;

    draw_corner_tab_capsule(
        app,
        factories,
        surf,
        hits,
        &tab_glyphs,
        &slot_w,
        0,
        5,
        left_dock_x,
        by,
        left_dock_w,
        bh,
        slot_y,
        slot_h,
        dock_pad,
        tab_gap,
        accent_glow,
        interactive,
    );
    draw_corner_tab_capsule(
        app,
        factories,
        surf,
        hits,
        &tab_glyphs,
        &slot_w,
        5,
        5,
        right_dock_x,
        by,
        right_dock_w,
        bh,
        slot_y,
        slot_h,
        dock_pad,
        tab_gap,
        accent_glow,
        interactive,
    );

    // Bottom-Center: Hover preview pill when hovering an inactive tab, else subtle constellation dots
    let center_left = left_dock_x + left_dock_w + 8.0;
    let center_right = right_dock_x - 8.0;
    let center_avail = (center_right - center_left).max(0.0);
    let hovered_inactive = match app.hover {
        HoverTarget::Tab(i) if i < 10 && i != app.page as usize => Some(i),
        _ => None,
    };
    if let Some(hi) = hovered_inactive
        && center_avail >= 64.0
    {
        let h_op = (app.tab_hover_anim[hi] as f32).max(0.35);
        let lbl = app.tab_label(hi);
        let lw = factories
            .measure_text(lbl, FontKind::Ui, 11.0, true, false, 120.0, false)
            .0;
        let pw = (24.0 + lw + 10.0).min(center_avail);
        let px_c = ex + (ew - pw) * 0.5;
        let py_c = by + 2.0;
        let ph_c = bh - 4.0;
        surf.fill_rounded_rect(
            px_c,
            py_c,
            pw,
            ph_c,
            ph_c * 0.5,
            ph_c * 0.5,
            Color::argb(0x16, 0xFF, 0xFF, 0xFF).with_alpha(h_op),
        );
        surf.draw_rounded_rect(
            px_c,
            py_c,
            pw,
            ph_c,
            ph_c * 0.5,
            ph_c * 0.5,
            accent_glow.with_alpha(0.35 * h_op),
            1.0,
            None,
        );
        surf.draw_text(
            factories,
            tab_glyphs[hi],
            px_c + 7.0,
            py_c,
            14.0,
            ph_c,
            FontKind::Icon,
            11.0,
            false,
            accent_glow.with_alpha(h_op),
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
        surf.draw_text(
            factories,
            lbl,
            px_c + 23.0,
            py_c,
            (pw - 30.0).max(10.0),
            ph_c,
            FontKind::Ui,
            11.0,
            true,
            TEXT_PRIMARY.with_alpha(0.92 * h_op),
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
    } else if center_avail >= 64.0 {
        let active_idx = app.page as usize;
        let unread_notifs = app.unread_notifications_count();
        let total_dots_w = 9.0 * 3.0 + 10.0 + 9.0 * 5.0;
        let mut dx = ex + (ew - total_dots_w) * 0.5;
        let dy = by + bh * 0.5;
        for i in 0..10 {
            let is_act = i == active_idx;
            let w = if is_act { 10.0 } else { 3.0 };
            let col = if is_act {
                accent_glow
            } else if i == 7 && unread_notifs > 0 {
                WARNING_BRUSH
            } else {
                Color::argb(0x38, 0xFF, 0xFF, 0xFF)
            };
            surf.fill_rounded_rect(dx, dy - 1.5, w, 3.0, 1.5, 1.5, col);
            dx += w + 5.0;
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "renders bottom corner tab capsule"
)]
fn draw_corner_tab_capsule(
    app: &NotchApp,
    factories: &D2dFactories,
    surf: &mut D2dSurface,
    hits: &mut HitMap,
    tab_glyphs: &[&str; 10],
    slot_w: &[f32; 10],
    start_idx: usize,
    count: usize,
    dock_x: f32,
    by: f32,
    dock_w: f32,
    bh: f32,
    slot_y: f32,
    slot_h: f32,
    dock_pad: f32,
    tab_gap: f32,
    accent_glow: Color,
    interactive: bool,
) {
    let slots_x = dock_x + dock_pad;
    surf.fill_rounded_rect(
        dock_x,
        by,
        dock_w,
        bh,
        bh * 0.5,
        bh * 0.5,
        Color::argb(0x14, 0xFF, 0xFF, 0xFF),
    );
    surf.draw_rounded_rect(
        dock_x,
        by,
        dock_w,
        bh,
        bh * 0.5,
        bh * 0.5,
        Color::argb(0x1F, 0xFF, 0xFF, 0xFF),
        1.0,
        None,
    );

    let active_group = usize::from((app.page as usize) >= 5);
    let this_group = usize::from(start_idx >= 5);
    if active_group == this_group {
        let vel = app.tab_pill_x.velocity.abs() as f32;
        let stretch = (vel / 1600.0).min(7.0);
        let squeeze = (stretch * 0.12).min(1.0);
        let pill_x = slots_x + app.tab_pill_x.value as f32 - stretch * 0.5;
        let pill_w = (app.tab_pill_w.value as f32 + stretch).max(22.0);
        let pill_y = slot_y + squeeze * 0.5;
        let pill_h = slot_h - squeeze;
        let pill_r = pill_h * 0.5;
        surf.fill_rounded_rect_radial_glow(
            pill_x,
            pill_y,
            pill_w,
            pill_h,
            pill_r,
            pill_x + pill_w * 0.5,
            pill_y + pill_h * 0.5,
            pill_w * 0.65,
            pill_h * 0.95,
            accent_glow.with_alpha(0.30),
            Color::argb(0, 0, 0, 0),
        );
        surf.fill_rounded_rect(
            pill_x,
            pill_y,
            pill_w,
            pill_h,
            pill_r,
            pill_r,
            accent_glow.with_alpha(0.22),
        );
        surf.draw_rounded_rect(
            pill_x,
            pill_y,
            pill_w,
            pill_h,
            pill_r,
            pill_r,
            accent_glow.lerp(TEXT_PRIMARY, 0.35).with_alpha(0.55),
            1.0,
            None,
        );
    }

    let unread_notifs = app.unread_notifications_count();
    let mut cur_x = slots_x;
    for i in start_idx..(start_idx + count) {
        let sw = slot_w[i];
        let tx = cur_x;
        cur_x += sw + tab_gap;
        let active = app.page as usize == i;
        let h_anim = app.tab_hover_anim[i] as f32;

        if !active && h_anim > 0.01 {
            surf.fill_rounded_rect(
                tx,
                slot_y,
                sw,
                slot_h,
                slot_h * 0.5,
                slot_h * 0.5,
                SUBTLE_FILL.with_alpha(h_anim),
            );
            surf.fill_ellipse(
                tx + sw * 0.5,
                slot_y + slot_h - 2.0,
                1.4,
                1.4,
                accent_glow.with_alpha(0.85 * h_anim),
            );
        }

        let icon_col = if active {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY.lerp(TEXT_PRIMARY, h_anim)
        };
        let (icon_cx, icon_cy, icon_scale) = if active {
            let bt = app.tab_bounce_t as f32;
            let bounce = if bt < 1.0 {
                1.0 + 0.22 * (bt * std::f32::consts::PI).sin() * (1.0 - bt)
            } else {
                1.0
            };
            (tx + 13.0, slot_y + slot_h * 0.5, bounce)
        } else {
            let lift = -1.2 * h_anim;
            (
                tx + sw * 0.5,
                slot_y + slot_h * 0.5 + lift,
                1.0 + 0.14 * h_anim,
            )
        };
        if (icon_scale - 1.0).abs() > 0.002 {
            surf.set_transform(Matrix3x2::scale_around(
                icon_scale,
                icon_scale,
                Vector2::new(icon_cx, icon_cy),
            ));
        }
        surf.draw_text(
            factories,
            tab_glyphs[i],
            icon_cx - 9.0,
            icon_cy - 9.0,
            18.0,
            18.0,
            FontKind::Icon,
            12.0,
            false,
            icon_col,
            HAlign::Center,
            VAlign::Center,
            false,
            false,
            false,
            false,
        );
        if (icon_scale - 1.0).abs() > 0.002 {
            surf.set_transform(Matrix3x2::identity());
        }
        if i == 7 && unread_notifs > 0 && !active {
            let pulse = 1.0 + 0.35 * (app.notif_pulse_t as f32);
            surf.fill_ellipse(
                icon_cx + 5.0,
                icon_cy - 4.5,
                2.4 * pulse,
                2.4 * pulse,
                WARNING_BRUSH,
            );
        }

        let target_w = app.tab_active_width(i, Some(factories)) as f32;
        let label_prog = ((sw - 28.0) / (target_w - 28.0).max(1.0)).clamp(0.0, 1.0);
        if label_prog > 0.05 {
            let label = app.tab_label(i);
            let lx = tx + 22.0;
            let lw = (sw - 28.0).max(1.0);
            let slide = (1.0 - label_prog) * 6.0;
            surf.push_clip(lx, slot_y, lw, slot_h);
            surf.draw_text(
                factories,
                label,
                lx + slide,
                slot_y,
                lw + 8.0,
                slot_h,
                FontKind::Ui,
                11.0,
                true,
                TEXT_PRIMARY.with_alpha(label_prog),
                HAlign::Left,
                VAlign::Center,
                false,
                false,
                false,
                false,
            );
            surf.pop_clip();
        }

        if interactive {
            hits.push(
                RectF {
                    x: tx,
                    y: slot_y,
                    w: sw,
                    h: slot_h,
                },
                HoverTarget::Tab(i),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::NotchPage;

    #[test]
    fn hit_map_prefers_last_registered_target() {
        let mut map = HitMap::default();
        map.push(
            RectF {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 40.0,
            },
            HoverTarget::Notch,
        );
        map.push(
            RectF {
                x: 10.0,
                y: 10.0,
                w: 20.0,
                h: 20.0,
            },
            HoverTarget::Tab(2),
        );
        assert_eq!(map.hit_test(15.0, 15.0), HoverTarget::Tab(2));
        assert_eq!(map.hit_test(5.0, 5.0), HoverTarget::Notch);
        assert_eq!(map.hit_test(200.0, 5.0), HoverTarget::None);
    }

    #[test]
    fn offscreen_d2d_renders_compact_and_expanded_pages() {
        // SAFETY: Initializing COM STA for WIC factory in unit test.
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        let factories = D2dFactories::new().expect("D2D factories");
        let mut app = NotchApp::new(None, None, None, None);
        app.update_targets(Some(&factories));
        app.snap_all_springs();
        app.fit_window(true);

        let mut surf = D2dSurface::new(&factories, 1200, 600, 2.0).expect("D2D surface");
        assert!(surf.begin_draw());
        let hits = render_notch(&mut app, &factories, &mut surf);
        surf.end_draw();
        assert!(hits.notch_rect.is_some());

        for page in [
            NotchPage::Media,
            NotchPage::Agenda,
            NotchPage::Focus,
            NotchPage::Water,
            NotchPage::Pray,
            NotchPage::Tasks,
            NotchPage::Shelf,
            NotchPage::Notifications,
            NotchPage::Clipboard,
            NotchPage::Weather,
        ] {
            app.select_page(page, Some(&factories));
            app.set_expanded(true, Some(&factories));
            app.snap_all_springs();
            app.fit_window(true);
            let pw = (app.window_w * 2.0).ceil() as i32;
            let ph = (app.window_h * 2.0).ceil() as i32;
            surf.ensure_size(pw, ph, 2.0);
            assert!(surf.begin_draw());
            let page_hits = render_notch(&mut app, &factories, &mut surf);
            surf.end_draw();
            assert!(
                page_hits
                    .items
                    .iter()
                    .any(|(_, t)| matches!(t, HoverTarget::Tab(_)))
            );
        }
    }

    #[test]
    fn offscreen_d2d_renders_custom_agent_marks_and_mochi_icon() {
        // SAFETY: Initializing COM STA for WIC factory in unit test.
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        let factories = D2dFactories::new().expect("D2D factories");
        let mut app = NotchApp::new(None, None, None, None);
        for (id, icon) in [
            ("a1", "claude"),
            ("a2", "cursor"),
            ("a3", "codex"),
            ("a4", "copilot"),
        ] {
            let req = serde_json::json!({
                "op": "set",
                "id": id,
                "title": icon,
                "text": "running",
                "icon": icon,
            })
            .to_string();
            assert_eq!(app.handle_activity(&req, Some(&factories)), "ok");
        }
        app.set_expanded(true, Some(&factories));
        app.snap_all_springs();
        app.fit_window(true);

        let pw = (app.window_w * 2.0).ceil() as i32;
        let ph = (app.window_h * 2.0).ceil() as i32;
        let mut surf = D2dSurface::new(&factories, pw, ph, 2.0).expect("D2D surface");
        assert!(surf.begin_draw());
        let hits = render_notch(&mut app, &factories, &mut surf);
        surf.end_draw();
        assert!(hits.panel_rect.is_some());

        let h32 = crate::window::load_ico_from_bytes(crate::theme::MOCHI_ICO, 32);
        let h16 = crate::window::load_ico_from_bytes(crate::theme::MOCHI_ICO, 16);
        assert!(!h32.is_invalid());
        assert!(!h16.is_invalid());
        // SAFETY: Destroying test HICON handles.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(h32);
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(h16);
        }
    }

    #[test]
    fn offscreen_d2d_renders_pure_eyes_kaomoji_states() {
        // SAFETY: Initializing COM STA for WIC factory in unit test.
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        let factories = D2dFactories::new().expect("D2D factories");
        let mut surf = D2dSurface::new(&factories, 200, 200, 2.0).expect("D2D surface");
        let mut anim = CompanionAnimator::default();
        for action in ["idle", "yes", "jump", "alert", "no", "thinking", "sleep"] {
            match action {
                "idle" => anim.perch(),
                "yes" => anim.nod(),
                "jump" => anim.fly(),
                "alert" => anim.caw(),
                "no" => anim.shake(),
                "thinking" => {
                    anim.perch();
                    anim.state = "thinking".to_string();
                }
                "sleep" => anim.sleep(),
                _ => {}
            }
            assert!(surf.begin_draw());
            draw_companion_sprite(&factories, &mut surf, &anim, 10.0, 10.0, 64.0);
            surf.end_draw();
        }
    }

    #[test]
    fn offscreen_d2d_renders_drag_approach_drop_zone_and_burst() {
        // SAFETY: Initializing COM STA for WIC factory in unit test.
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        let factories = D2dFactories::new().expect("D2D factories");
        let mut app = NotchApp::new(None, None, None, None);
        let mut surf = D2dSurface::new(&factories, 1600, 600, 2.0).expect("D2D surface");

        // Stage 1: Magnetic approach on collapsed notch
        app.on_drag_approach((322.0, 56.0), 0.85, Some(&factories));
        app.snap_all_springs();
        app.drag_approach = 0.85;
        app.fit_window(true);
        surf.ensure_size(
            (app.window_w * 2.0).ceil() as i32,
            (app.window_h * 2.0).ceil() as i32,
            2.0,
        );
        assert!(surf.begin_draw());
        render_notch(&mut app, &factories, &mut surf);
        surf.end_draw();

        // Stage 2: Active drop zone in Shelf
        app.on_drag_enter(3, (420.0, 110.0), Some(&factories));
        app.snap_all_springs();
        app.drag_anim = 1.0;
        app.fit_window(true);
        surf.ensure_size(
            (app.window_w * 2.0).ceil() as i32,
            (app.window_h * 2.0).ceil() as i32,
            2.0,
        );
        assert!(surf.begin_draw());
        render_notch(&mut app, &factories, &mut surf);
        surf.end_draw();

        // Stage 3: Drop burst ripple
        let tmp = std::env::temp_dir().join("mochi-render-drop-test.txt");
        let _ = std::fs::write(&tmp, b"drop");
        app.on_drag_drop(
            &[tmp.to_string_lossy().into_owned()],
            Some((420.0, 110.0)),
            Some(&factories),
        );
        let _ = std::fs::remove_file(&tmp);
        app.snap_all_springs();
        app.drop_burst_t = 0.35;
        app.shelf_flash_t = 0.8;
        assert!(surf.begin_draw());
        render_notch(&mut app, &factories, &mut surf);
        surf.end_draw();
    }
}
