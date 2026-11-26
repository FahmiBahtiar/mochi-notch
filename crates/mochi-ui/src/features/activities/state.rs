use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use mochi_winrt::mochi_activities;
use mochi_winrt::mochi_activities::port::{ActivityOp, ActivityState};
use mochi_winrt::native::focus_host_window;
use mochi_winrt::paths::{Glyphs, HudKind, HudMessage};

use crate::d2d::D2dFactories;
use crate::features::focus::state::FocusPhase;
use crate::state::{BUBBLE_GAP, EAR, NotchApp};

#[derive(Debug, Clone)]
pub struct Activity {
    pub id: String,
    pub title: String,
    pub text: Option<String>,
    pub progress: Option<f64>,
    pub state: String,
    pub icon: String,
    pub glyph: String,
    pub badge: Option<String>,
    pub warning: bool,
    pub line: String,
    pub updated: Instant,
    pub expires: Instant,
    pub window: Option<(i64, i32)>,
}

#[derive(Debug, Clone)]
pub struct BubbleItem {
    pub glyph: String,
    pub label: Option<String>,
    pub progress: Option<f64>,
    pub warning: bool,
    pub detail: HudMessage,
    pub dim: bool,
    pub badge: Option<String>,
    pub activity_id: Option<String>,
    pub window: Option<(i64, i32)>,
}

fn bubble_items_equal(a: &Option<BubbleItem>, b: &Option<BubbleItem>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.glyph == y.glyph
                && x.label == y.label
                && x.progress == y.progress
                && x.warning == y.warning
                && x.dim == y.dim
                && x.badge == y.badge
                && x.activity_id == y.activity_id
                && x.window == y.window
                && x.detail.text == y.detail.text
                && x.detail.value == y.detail.value
                && x.detail.level == y.detail.level
                && x.detail.warning == y.detail.warning
                && x.detail.glyph == y.detail.glyph
                && x.detail.badge == y.detail.badge
        }
        _ => false,
    }
}

impl NotchApp {
    pub(crate) fn layout_activity_panel(&mut self, expanded_width: f64) {
        if self.activities.is_empty() {
            self.panel_below = false;
            self.panel_width = 320.0;
            return;
        }
        let room =
            self.display_dip_width / 2.0 - expanded_width / 2.0 - BUBBLE_GAP - 2.0 * EAR - 8.0;
        self.panel_below = room < 220.0;
        self.panel_width = if self.panel_below {
            320.0
        } else {
            320.0_f64.min(room)
        };
    }

    pub fn has_agent_prompt_bar(&self) -> bool {
        !self.activities.is_empty()
            && (self.copilot_hooks_on
                || self
                    .activities
                    .iter()
                    .any(|a| a.icon.eq_ignore_ascii_case("copilot") || a.window.is_some()))
    }

    pub fn panel_height(&self) -> f64 {
        let rows = self.activities.len().min(4);
        if rows == 0 {
            return 0.0;
        }
        let more = if self.activities.len() > rows {
            18.0
        } else {
            0.0
        };
        let prompt_bar = if self.has_agent_prompt_bar() {
            38.0
        } else {
            0.0
        };
        (rows as f64) * 30.0 + more + prompt_bar + 16.0
    }

    pub fn update_bubble(&mut self) {
        let now = chrono::Local::now().naive_local();
        let item = self.pick_bubble(now);
        let appearing = item.is_some() != self.bubble_item.is_some();
        let changed = !bubble_items_equal(&self.bubble_item, &item);
        self.bubble_item = item;
        if appearing {
            self.update_targets(None);
        } else if changed {
            self.dirty = true;
        }
    }

    fn pick_bubble(&self, now: NaiveDateTime) -> Option<BubbleItem> {
        let event_waits = self.media_state.is_some() && self.upcoming_event.is_some();
        let focus_waits = self.media_state.is_some() && self.focus.is_active();
        let others =
            self.activities.len() as isize + isize::from(event_waits) + isize::from(focus_waits)
                - 1;

        let top = self.activities.first();
        let mut item = if let Some(a) = top
            && a.warning
        {
            Some(Self::bubble_from_activity(a))
        } else if event_waits && let Some(ref ev) = self.upcoming_event {
            let left = ev.start - now;
            let mins = ((left.num_seconds() as f64) / 60.0).ceil().max(0.0) as i64;
            let prog = (1.0 - (left.num_seconds() as f64) / 900.0).clamp(0.0, 1.0);
            let mut detail = HudMessage::new(
                Glyphs::CALENDAR,
                format!("{} {}", ev.title, self.format_until(ev.start, now)),
            );
            detail.duration_ms = 3000;
            Some(BubbleItem {
                glyph: Glyphs::CALENDAR.to_string(),
                label: Some(mins.to_string()),
                progress: Some(prog),
                warning: false,
                detail,
                dim: false,
                badge: None,
                activity_id: None,
                window: None,
            })
        } else if focus_waits {
            let rem = self.focus.remaining();
            let s = self.lang.strings();
            let phase = match self.focus.phase {
                FocusPhase::ShortBreak => s.focus_short_break,
                FocusPhase::LongBreak => s.focus_long_break,
                FocusPhase::Focus => s.focus_focus,
            };
            let exact = format!("{:02}:{:02}", rem.as_secs() / 60, rem.as_secs() % 60);
            let paused = self.focus.is_paused();
            let glyph = if paused { "\u{E769}" } else { Glyphs::TIMER };
            let mut detail = HudMessage::new(glyph, format!("{phase} · {exact}"));
            detail.duration_ms = 3000;
            let prog = 1.0 - self.focus.progress(&self.store.data.settings);
            if paused {
                Some(BubbleItem {
                    glyph: glyph.to_string(),
                    label: None,
                    progress: Some(prog),
                    warning: false,
                    detail,
                    dim: true,
                    badge: None,
                    activity_id: None,
                    window: None,
                })
            } else {
                let mins = (rem.as_secs_f64() / 60.0).ceil() as i64;
                let label = if rem < Duration::from_secs(60) {
                    format!("{}s", rem.as_secs())
                } else if mins > 99 {
                    format!("{}h", (rem.as_secs_f64() / 3600.0).round() as i64)
                } else {
                    mins.to_string()
                };
                Some(BubbleItem {
                    glyph: Glyphs::TIMER.to_string(),
                    label: Some(label),
                    progress: Some(prog),
                    warning: false,
                    detail,
                    dim: false,
                    badge: None,
                    activity_id: None,
                    window: None,
                })
            }
        } else {
            top.map(Self::bubble_from_activity)
        }?;

        if others > 0 {
            let more = format!("+{others}");
            item.detail.value = Some(match item.detail.value {
                Some(v) => format!("{v} · {more}"),
                None => more,
            });
        }
        Some(item)
    }

    fn bubble_from_activity(a: &Activity) -> BubbleItem {
        let mut detail = HudMessage::new(a.glyph.clone(), a.line.clone());
        detail.value = a.progress.map(|p| format!("{:.0}%", p * 100.0));
        detail.level = a.progress;
        detail.warning = a.warning;
        detail.kind = HudKind::Activity;
        detail.tag = Some(a.id.clone());
        detail.badge = a.badge.clone();
        BubbleItem {
            glyph: a.glyph.clone(),
            label: None,
            progress: a.progress,
            warning: a.warning,
            detail,
            dim: false,
            badge: a.badge.clone(),
            activity_id: Some(a.id.clone()),
            window: a.window,
        }
    }

    pub fn handle_activity(&mut self, line: &str, factories: Option<&D2dFactories>) -> String {
        if !self.store.data.settings.activities_enabled {
            return "off: activities are turned off in Mochi's settings".to_string();
        }
        let now = Instant::now();
        while let Some(&front) = self.activity_requests.front() {
            if now.duration_since(front) > Duration::from_secs(10) {
                self.activity_requests.pop_front();
            } else {
                break;
            }
        }
        if self.activity_requests.len() >= 60 {
            return "busy: too many requests, try again in a few seconds".to_string();
        }
        self.activity_requests.push_back(now);

        let req = match mochi_activities::port::parse_activity_request(line) {
            Ok(r) => r,
            Err(err) => return format!("error: {err}"),
        };
        let id = req.id;

        if req.op == ActivityOp::End {
            if let Some(pos) = self.activities.iter().position(|a| a.id == id) {
                self.activities.remove(pos);
                self.update_bubble();
                self.update_targets(factories);
            }
            return "ok".to_string();
        }

        let existing_idx = self.activities.iter().position(|a| a.id == id);
        if existing_idx.is_none() && self.activities.len() >= 8 {
            return "full: at most 8 activities at a time".to_string();
        }
        let prev = existing_idx.map(|i| self.activities[i].clone());
        let title = req
            .title
            .filter(|s| !s.is_empty())
            .or_else(|| prev.as_ref().map(|p| p.title.clone()));
        let Some(title) = title else {
            return "error: no activity with this id (it may have ended or expired); send --title to create it".to_string();
        };

        let state = req.state.as_str().to_string();
        let s = self.lang.strings();
        let phrase_text = req.phrase.as_deref().map(|p| match p {
            "permission" => s.activity_permission,
            "waiting" => s.activity_waiting,
            "done" => s.activity_done,
            "failed" => s.activity_failed,
            _ => s.activity_working,
        });
        let text = req
            .text
            .or_else(|| phrase_text.map(str::to_string))
            .or_else(|| prev.as_ref().and_then(|p| p.text.clone()));
        let icon = req
            .icon
            .or_else(|| prev.as_ref().map(|p| p.icon.clone()))
            .unwrap_or_else(|| "activity".to_string());
        let progress = if req.state == ActivityState::Done {
            None
        } else {
            req.progress
                .or_else(|| prev.as_ref().and_then(|p| p.progress))
        };
        let ttl_secs = match req.state {
            ActivityState::Done => 12.0,
            ActivityState::Attention | ActivityState::Failed => 3600.0,
            ActivityState::Running => req.ttl_seconds.unwrap_or(600.0),
        };
        let window = match (req.window_handle, req.window_pid) {
            (Some(h), Some(p)) => Some((h, p as i32)),
            _ => prev.as_ref().and_then(|p| p.window),
        };
        let glyph = match icon.to_ascii_lowercase().as_str() {
            "code" => "\u{E943}",
            "build" => "\u{E90F}",
            "download" => "\u{E896}",
            "upload" => "\u{E898}",
            "sync" => "\u{E895}",
            "agent" => "\u{E99A}",
            "bell" => "\u{EA8F}",
            "timer" => "\u{E916}",
            "claude" => "mark:claude",
            "cursor" => "mark:cursor",
            "codex" => "mark:codex",
            "copilot" => "mark:copilot",
            _ => "\u{E945}",
        }
        .to_string();
        let badge = match state.as_str() {
            "Done" => Some("\u{E73E}".to_string()),
            "Failed" => Some("\u{E783}".to_string()),
            "Attention" => Some("\u{E171}".to_string()),
            _ => None,
        };
        let warning = matches!(state.as_str(), "Attention" | "Failed");
        let line_str = match &text {
            Some(t) if !t.is_empty() => format!("{title} · {t}"),
            _ => title.clone(),
        };

        let act = Activity {
            id: id.clone(),
            title,
            text,
            progress,
            state: state.clone(),
            icon,
            glyph: glyph.clone(),
            badge: badge.clone(),
            warning,
            line: line_str.clone(),
            updated: now,
            expires: now + Duration::from_secs_f64(ttl_secs),
            window,
        };

        let added = existing_idx.is_none();
        let state_changed = prev.as_ref().is_some_and(|p| p.state != state);
        if let Some(idx) = existing_idx {
            self.activities[idx] = act.clone();
        } else {
            self.activities.push(act.clone());
        }
        self.activities.sort_by(|a, b| {
            b.warning
                .cmp(&a.warning)
                .then_with(|| b.updated.cmp(&a.updated))
        });

        let important = state != "Running" && (added || state_changed);
        if important || added {
            let mut notice = HudMessage::new(glyph, line_str);
            notice.value = progress.map(|p| format!("{:.0}%", p * 100.0));
            notice.level = progress;
            notice.warning = warning;
            notice.duration_ms = if important { 4500 } else { 2500 };
            notice.important = important;
            notice.celebrate = state == "Done" && act.icon.eq_ignore_ascii_case("copilot");
            notice.kind = HudKind::Activity;
            notice.tag = Some(id);
            notice.badge = badge;
            self.show_hud(notice, factories);
        }
        self.update_bubble();
        self.update_targets(factories);
        "ok".to_string()
    }

    pub fn go_to_activity_window(&mut self) -> bool {
        let announced_id = if self.hud_active
            && let Some(ref cur) = self.hud_queue.current
            && cur.kind == HudKind::Activity
        {
            cur.tag.clone()
        } else {
            None
        };
        let Some(id) = announced_id else {
            return false;
        };
        let Some(win) = self
            .activities
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.window)
        else {
            return false;
        };
        if focus_host_window(win.0, win.1) {
            self.hide_hud(None);
            true
        } else {
            if let Some(a) = self.activities.iter_mut().find(|a| a.id == id) {
                a.window = None;
            }
            self.update_bubble();
            false
        }
    }

    pub fn on_jump_hotkey(&mut self, factories: Option<&D2dFactories>) {
        let now = Instant::now();
        let after = if self
            .last_jump_at
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(5))
        {
            self.last_jump_id.clone()
        } else {
            None
        };
        let mut candidates: Vec<usize> = self
            .activities
            .iter()
            .enumerate()
            .filter(|(_, a)| a.window.is_some())
            .map(|(i, _)| i)
            .collect();
        if let Some(ref after_id) = after
            && let Some(pos) = candidates
                .iter()
                .position(|&i| self.activities[i].id == *after_id)
        {
            let len = candidates.len();
            candidates.rotate_left((pos + 1) % len);
        }
        for idx in candidates {
            let id = self.activities[idx].id.clone();
            if let Some(win) = self.activities[idx].window {
                if focus_host_window(win.0, win.1) {
                    self.last_jump_id = Some(id.clone());
                    self.last_jump_at = Some(now);
                    if self.hud_queue.current.as_ref().is_some_and(|c| {
                        c.kind == HudKind::Activity && c.tag.as_deref() == Some(&id)
                    }) {
                        self.hide_hud(factories);
                    }
                    return;
                } else {
                    self.activities[idx].window = None;
                }
            }
        }
        self.last_jump_id = None;
        self.update_bubble();
        if self.is_visible() && !self.expanded {
            self.hover_deadline = None;
            self.pinned_until = Some(now + Duration::from_secs(10));
            self.set_expanded(true, factories);
        }
    }
}
