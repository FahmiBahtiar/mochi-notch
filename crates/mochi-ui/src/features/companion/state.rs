use mochi_winrt::mochi_gaze::port::EyeGazeController;

use crate::state::NotchApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RavenMode {
    Still,
    Perched,
    Asleep,
    Nodding,
    Flying,
    Reacting,
    Thinking,
}

#[derive(Debug, Clone)]
pub struct GazeState {
    pub norm_x: f64,
    pub norm_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    inner: EyeGazeController,
}

impl Default for GazeState {
    fn default() -> Self {
        Self {
            norm_x: 0.0,
            norm_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            inner: EyeGazeController::new(0x9E37_79B9_7F4A_7C15),
        }
    }
}

impl GazeState {
    pub fn reset(&mut self) {
        self.inner = EyeGazeController::new(0x9E37_79B9_7F4A_7C15);
        self.norm_x = 0.0;
        self.norm_y = 0.0;
        self.scale_x = 1.0;
        self.scale_y = 1.0;
    }

    pub fn trigger_blink(&mut self, force_double: bool) {
        self.inner.trigger_blink(force_double);
    }

    pub fn step(
        &mut self,
        dt: f64,
        sleeping: bool,
        reduced_motion: bool,
        cursor_screen_px: Option<(f64, f64)>,
        cursor_offset_dip: Option<(f64, f64)>,
    ) -> bool {
        let prev = (self.norm_x, self.norm_y, self.scale_x, self.scale_y);
        self.inner.step(
            dt,
            sleeping,
            reduced_motion,
            cursor_screen_px,
            cursor_offset_dip,
            15.0,
        );
        self.norm_x = self.inner.norm_x;
        self.norm_y = self.inner.norm_y;
        self.scale_x = self.inner.scale_x;
        self.scale_y = self.inner.scale_y;
        (self.norm_x - prev.0).abs() > 0.001
            || (self.norm_y - prev.1).abs() > 0.001
            || (self.scale_x - prev.2).abs() > 0.001
            || (self.scale_y - prev.3).abs() > 0.001
    }
}

#[derive(Debug, Clone)]
pub struct RavenAnimator {
    pub mode: RavenMode,
    pub state: String,
    pub frame_index: usize,
    pub facing_left: bool,
    pub letter: bool,
    pub dim: bool,
    pub eye_shift_x: f64,
    pub eye_shift_y: f64,
    pub eye_scale_x: f64,
    pub eye_scale_y: f64,
    pub gaze: GazeState,
    accum: f64,
    interval: f64,
}

impl Default for RavenAnimator {
    fn default() -> Self {
        Self {
            mode: RavenMode::Still,
            state: "idle".to_string(),
            frame_index: 0,
            facing_left: false,
            letter: false,
            dim: false,
            eye_shift_x: 0.0,
            eye_shift_y: 0.0,
            eye_scale_x: 1.0,
            eye_scale_y: 1.0,
            gaze: GazeState::default(),
            accum: 0.0,
            interval: 0.033,
        }
    }
}

impl RavenAnimator {
    pub fn perch(&mut self) {
        if self.mode == RavenMode::Perched {
            return;
        }
        self.enter(RavenMode::Perched, "idle", 0.033);
        self.gaze.trigger_blink(true);
    }

    pub fn sleep(&mut self) {
        if self.mode == RavenMode::Asleep {
            return;
        }
        self.dim = true;
        self.enter(RavenMode::Asleep, "idle", 0.085);
    }

    pub fn nod(&mut self) {
        if self.mode == RavenMode::Nodding {
            return;
        }
        self.enter(RavenMode::Nodding, "yes", 0.033);
    }

    pub fn fly(&mut self) {
        if self.mode == RavenMode::Flying {
            return;
        }
        self.enter(RavenMode::Flying, "jump", 0.025);
    }

    pub fn land(&mut self) {
        self.perch();
    }

    pub fn caw(&mut self) {
        self.play_once("alert", 0.033);
    }

    pub fn hop(&mut self) {
        self.play_once("jump", 0.030);
    }

    pub fn shake(&mut self) {
        self.play_once("no", 0.030);
    }

    pub fn stop(&mut self) {
        self.mode = RavenMode::Still;
        self.dim = false;
        self.state = "idle".to_string();
        self.frame_index = 0;
        self.eye_shift_x = 0.0;
        self.eye_shift_y = 0.0;
        self.eye_scale_x = 1.0;
        self.eye_scale_y = 1.0;
        self.gaze.reset();
    }

    fn enter(&mut self, mode: RavenMode, state: &str, interval: f64) {
        self.mode = mode;
        self.state = state.to_string();
        self.frame_index = 0;
        self.facing_left = false;
        self.dim = mode == RavenMode::Asleep;
        if mode != RavenMode::Perched && mode != RavenMode::Asleep {
            self.eye_shift_x = 0.0;
            self.eye_shift_y = 0.0;
            self.eye_scale_x = 1.0;
            self.eye_scale_y = 1.0;
        }
        self.accum = 0.0;
        self.interval = interval;
    }

    fn play_once(&mut self, state: &str, interval: f64) {
        self.mode = RavenMode::Reacting;
        self.state = state.to_string();
        self.frame_index = 0;
        self.dim = false;
        self.facing_left = false;
        self.eye_shift_x = 0.0;
        self.eye_shift_y = 0.0;
        self.eye_scale_x = 1.0;
        self.eye_scale_y = 1.0;
        self.accum = 0.0;
        self.interval = interval;
    }

    pub fn tick(
        &mut self,
        dt: f64,
        reduced_motion: bool,
        cursor_screen_px: Option<(f64, f64)>,
        cursor_offset_dip: Option<(f64, f64)>,
    ) -> bool {
        if self.mode == RavenMode::Still {
            return false;
        }
        self.accum += dt;
        if self.accum < self.interval {
            return false;
        }
        let step_dt = self.accum.clamp(0.001, 0.1);
        self.accum = 0.0;

        if reduced_motion && self.mode != RavenMode::Reacting {
            let prev_frame = self.frame_index;
            self.frame_index = 0;
            let gaze_changed = if self.mode == RavenMode::Asleep {
                let c = self.gaze.step(step_dt, true, true, None, None);
                self.sync_eyes(8.5);
                c
            } else if self.mode == RavenMode::Perched {
                let c = self
                    .gaze
                    .step(step_dt, false, true, cursor_screen_px, cursor_offset_dip);
                self.sync_eyes(8.5);
                c
            } else {
                false
            };
            return prev_frame != self.frame_index || gaze_changed;
        }

        let prev_frame = self.frame_index;
        let prev_mode = self.mode;
        let mut gaze_changed = false;
        let mut anim_changed = false;
        const LOOP_FRAMES: usize = 36;
        const REACT_FRAMES: usize = 42;
        match self.mode {
            RavenMode::Perched => {
                gaze_changed =
                    self.gaze
                        .step(step_dt, false, false, cursor_screen_px, cursor_offset_dip);
                self.sync_eyes(8.5);
            }
            RavenMode::Asleep => {
                gaze_changed = self.gaze.step(step_dt, true, false, None, None);
                self.sync_eyes(8.5);
            }
            RavenMode::Nodding | RavenMode::Flying | RavenMode::Thinking => {
                self.frame_index = (self.frame_index + 1) % LOOP_FRAMES;
                anim_changed = prev_frame != self.frame_index;
            }
            RavenMode::Reacting => {
                self.frame_index += 1;
                anim_changed = true;
                if self.frame_index >= REACT_FRAMES {
                    self.perch();
                }
            }
            RavenMode::Still => {}
        }
        anim_changed || prev_mode != self.mode || gaze_changed
    }

    fn sync_eyes(&mut self, max_y: f64) {
        self.eye_shift_x = self.gaze.norm_x * 14.0;
        self.eye_shift_y = self.gaze.norm_y * max_y;
        self.eye_scale_x = self.gaze.scale_x;
        self.eye_scale_y = self.gaze.scale_y;
    }
}

impl NotchApp {
    pub fn update_companion(&mut self) {
        let idle = self.store.data.settings.companion_enabled && self.presence_state != 0;
        let playing = self.media_state.as_ref().is_some_and(|m| m.is_playing);
        let nodding = idle && playing && !self.focus.is_active();
        if nodding {
            self.music_raven.nod();
        } else {
            self.music_raven.stop();
        }

        let show_clock_raven = !idle
            && self.media_state.is_none()
            && self.store.data.settings.companion_enabled
            && !self.focus.is_active()
            && self.upcoming_text.is_none();
        if show_clock_raven {
            self.clock_raven.perch();
        } else {
            self.clock_raven.stop();
        }

        let ambient = (idle && !playing && self.is_visible()) || self.force_ambient;
        if ambient {
            self.ambient_shown = true;
            self.visor_sleeping = self.presence_state == 2;
        } else if self.ambient_shown {
            self.force_ambient = false;
            self.ambient_shown = false;
            self.visor_sleeping = false;
            self.visor_gaze.reset();
        }
    }
}
