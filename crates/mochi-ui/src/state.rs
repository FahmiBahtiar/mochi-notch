use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike};
use serde_json::Value;

use mochi_winrt::audio::AudioService;
use mochi_winrt::calendar::{CalendarEvent, CalendarInfo};
use mochi_winrt::google::GoogleAuth;
use mochi_winrt::http::open_url;
use mochi_winrt::lyrics::{Lyrics, LyricsService};
use mochi_winrt::media::{MediaService, MediaState};
use mochi_winrt::mochi_prayer::port::PrayerSchedule;
pub use mochi_winrt::mochi_presence::port::{AwayItem, AwayReport};
use mochi_winrt::mochi_spring;
use mochi_winrt::native::{get_cursor_pos, get_idle_ms};
use mochi_winrt::notifications::NotificationAccess;
use mochi_winrt::paths::{AppStore, Glyphs, HudMessage, Log};
use mochi_winrt::strings::Language;
use mochi_winrt::tasks::TaskListInfo;
use mochi_winrt::watchers::{KeyboardWatcher, PowerWatcher, is_motion_reduced};
use mochi_winrt::weather::{WeatherPlace, WeatherReport};

use crate::d2d::{D2dFactories, FontKind};
pub use crate::features::activities::state::{Activity, BubbleItem};
pub use crate::features::agenda::state::AgendaRow;
pub use crate::features::clipboard::state::{ClipboardItem, ClipboardKind};
pub use crate::features::companion::state::{GazeState, RavenAnimator, RavenMode};
pub use crate::features::focus::state::{FocusController, FocusPhase, FocusState};
pub use crate::features::hud::state::HudQueue;
pub use crate::features::notifications::state::NotificationItem;
pub use crate::features::shelf::state::{ShelfEntry, UndoKind, UndoState};
use crate::theme::{Color, DEFAULT_ACCENT, DecodedFrame};

pub const EAR: f64 = 8.0;
pub const COMPACT_IDLE: (f64, f64) = (120.0, 30.0);
pub const COMPACT_IDLE_WITH_RAVEN: (f64, f64) = (126.0, 30.0);
pub const COMPACT_IDLE_TIMER: (f64, f64) = (160.0, 30.0);
pub const COMPACT_IDLE_WEATHER: (f64, f64) = (180.0, 30.0);
pub const COMPACT_MEDIA: (f64, f64) = (260.0, 32.0);
pub const COMPACT_EVENT: (f64, f64) = (300.0, 32.0);
pub const COMPACT_EVENT_TIMER: (f64, f64) = (380.0, 32.0);
pub const EXPANDED_WIDTH: f64 = 520.0;
pub const EXPANDED_SHORT: (f64, f64) = (EXPANDED_WIDTH, 224.0);
pub const EXPANDED_WIDE: (f64, f64) = (760.0, 224.0);
pub const COMPACT_AMBIENT: (f64, f64) = (120.0, 30.0);
pub const NEWS_WIDTH: f64 = 460.0;
pub const HUD_MIN_WIDTH: f64 = 340.0;
pub const HUD_PADDING: f64 = 22.0;
pub const HUD_HEIGHT: f64 = 36.0;
pub const COMPACT_RADIUS: f64 = 15.0;
pub const EXPANDED_RADIUS: f64 = 30.0;
pub const PEEK_WIDTH: f64 = 8.0;
pub const PEEK_HEIGHT: f64 = 3.0;
pub const PRIVACY_GAP: f64 = 8.0;
pub const BUBBLE_WIDTH: f64 = 54.0;
pub const BUBBLE_HEIGHT: f64 = 32.0;
pub const BUBBLE_GAP: f64 = 6.0;
pub const TAB_INACTIVE_W: f64 = 24.0;
pub const TAB_GAP: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotchPage {
    Media = 0,
    Agenda = 1,
    Focus = 2,
    Water = 3,
    Pray = 4,
    Tasks = 5,
    Shelf = 6,
    Notifications = 7,
    Clipboard = 8,
    Weather = 9,
}

#[derive(Debug, Clone)]
pub struct Spring {
    pub value: f64,
    pub target: f64,
    pub velocity: f64,
    pub stiffness: f64,
    pub damping_ratio: f64,
    pub epsilon: f64,
}

impl Spring {
    pub fn new(initial: f64, epsilon: f64) -> Self {
        Self {
            value: initial,
            target: initial,
            velocity: 0.0,
            stiffness: 400.0,
            damping_ratio: 1.0,
            epsilon,
        }
    }

    pub fn step(&mut self, dt: f64) -> bool {
        if let Ok(res) = mochi_spring::port::step_spring(
            self.value,
            self.velocity,
            self.target,
            self.stiffness,
            self.damping_ratio,
            self.epsilon,
            dt,
        ) {
            self.value = res
                .get("Value")
                .and_then(Value::as_f64)
                .unwrap_or(self.target);
            self.velocity = res.get("Velocity").and_then(Value::as_f64).unwrap_or(0.0);
            res.get("Moving").and_then(Value::as_bool).unwrap_or(false)
        } else {
            self.snap();
            false
        }
    }

    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }
}

#[derive(Debug, Clone)]
pub struct FrameTimings {
    pub path: Option<PathBuf>,
    pub recording: bool,
    pub label: String,
    pub cycle: i32,
    pub frame: i32,
    pub start: Instant,
    pub last: Instant,
    pub moving: bool,
    pub pending: String,
}

impl FrameTimings {
    pub fn new(path: Option<PathBuf>) -> Self {
        let now = Instant::now();
        Self {
            path,
            recording: false,
            label: String::new(),
            cycle: 0,
            frame: 0,
            start: now,
            last: now,
            moving: false,
            pending: String::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    pub fn begin(&mut self, label: &str, cycle: i32) {
        if self.path.is_none() {
            return;
        }
        self.end();
        self.label = label.to_string();
        self.cycle = cycle;
        self.frame = 0;
        self.moving = false;
        let now = Instant::now();
        self.start = now;
        self.last = now;
        self.recording = true;
    }

    pub fn record_frame(&mut self, work_ms: f64) {
        if !self.recording {
            return;
        }
        let now = Instant::now();
        let interval_ms = now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.last = now;
        self.pending.push_str(&format!(
            "{};{};{};{:.2};{:.2}\r\n",
            self.label, self.cycle, self.frame, interval_ms, work_ms
        ));
        self.frame += 1;
        let elapsed = now.duration_since(self.start);
        if (!self.moving && elapsed >= Duration::from_millis(300))
            || elapsed >= Duration::from_millis(1500)
        {
            self.end();
        }
    }

    pub fn end(&mut self) {
        if !self.recording {
            return;
        }
        self.recording = false;
        if let Some(ref p) = self.path
            && !self.pending.is_empty()
        {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
            {
                use std::io::Write;
                let _ = f.write_all(self.pending.as_bytes());
            }
            self.pending.clear();
        }
    }

    pub fn note(&mut self, text: &str) {
        self.end();
        if let Some(ref p) = self.path
            && let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
        {
            use std::io::Write;
            let _ = writeln!(f, "#{text}");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverTarget {
    None,
    Notch,
    Bubble,
    ActivityPanel,
    Tab(usize),
    SettingsBtn,
    RefreshBtn,
    SwitchPlayerBtn,
    RemovePlayerBtn,
    PrevBtn,
    PlayPauseBtn,
    NextBtn,
    ProgressTrack,
    LyricLine(usize),
    AgendaEmptyBtn,
    AgendaJoin(usize),
    CalendarPrevMonth,
    CalendarNextMonth,
    CalendarToday,
    CalendarDay(i32, u32, u32),
    FocusPrimary,
    FocusSecondary,
    FocusTaskCheck,
    FocusTaskPick,
    FocusPickClose,
    FocusPickRow(usize),
    WaterDrink,
    WaterUndo,
    TaskToggle(usize),
    TaskRemove(usize),
    TaskAddBtn,
    TaskInput,
    ShelfTile(usize),
    ShelfRemove(usize),
    ShelfClear,
    UndoBtn,
    StripGo(usize),
    StripChat(usize),
    StripStop(usize),
    StripDismiss(usize),
    AgentPromptInput,
    AgentPromptSend,
    AgentNewChat,
    HeaderCopilotBtn,
    HeaderNotifBtn,
    NotifItem(usize),
    NotifDismiss(usize),
    NotifMarkAllRead,
    NotifClearAll,
    NotifOpenWinSettings,
    ClipItem(usize),
    ClipPin(usize),
    ClipRemove(usize),
    ClipClear,
    WeatherCityBtn,
    WeatherSearchInput,
    WeatherSearchBtn,
    WeatherLocateBtn,
    WeatherPickPlace(usize),
    WeatherClosePicker,
    PrayCityBtn,
    PrayAlertToggle,
    PrayPreAlertToggle,
    PrayMethodCycle,
    PrayMuteToggle(usize),
    PraySearchInput,
    PraySearchBtn,
    PrayLocateBtn,
    PrayPickPlace(usize),
    PrayClosePicker,
}

pub struct NotchApp {
    pub store: AppStore,
    pub lang: Language,
    pub page: NotchPage,
    pub expanded: bool,
    pub pending_expand: bool,
    pub hover_deadline: Option<Instant>,
    pub hover_from: (i32, i32),
    pub peeking: bool,
    pub animating: bool,
    pub pinned_until: Option<Instant>,
    pub hidden_by_user: bool,
    pub fullscreen_app: bool,
    pub session_locked: bool,
    pub capturable: bool,
    pub keyboard_mode: bool,
    pub previous_foreground: isize,
    pub drop_hinting: bool,
    pub dragging_out: bool,
    pub drag_approaching: bool,
    pub drag_approach: f64,
    pub drag_approach_target: f64,
    pub drag_anim: f64,
    pub drag_phase: f64,
    pub drag_cursor_dip: Option<(f64, f64)>,
    pub drag_file_count: usize,
    pub drag_opened_notch: bool,
    pub drag_prev_page: Option<NotchPage>,
    pub drop_burst_t: f64,
    pub drop_burst_pos: Option<(f64, f64)>,
    pub shelf_flash_t: f64,
    pub shelf_flash_count: usize,
    pub drop_celebrate_count: usize,

    // Springs
    pub width: Spring,
    pub height: Spring,
    pub radius: Spring,
    pub expand: Spring,
    pub hud: Spring,
    pub ambient: Spring,
    pub bubble: Spring,

    // Tab dock & page slide
    pub tab_slot_w: [Spring; 10],
    pub tab_pill_x: Spring,
    pub tab_pill_w: Spring,
    pub tab_hover_anim: [f64; 10],
    pub gear_hover_anim: f64,
    pub tab_bounce_t: f64,
    pub page_slide_dir: f64,
    pub page_slide_t: f64,

    // Compact text swap & marquee
    pub compact_text: String,
    pub compact_incoming: Option<String>,
    pub compact_swap_phase: f64, // 0 = idle, >0..1 = out, 1..2 = in
    pub compact_marquee_offset: f64,
    pub compact_marquee_t: f64,
    pub compact_marquee_active: bool,
    pub title_marquee_offset: f64,
    pub title_marquee_t: f64,
    pub title_marquee_active: bool,
    pub title_reveal_at: Option<Instant>,

    // Media & Lyrics
    pub media_state: Option<MediaState>,
    pub next_source: Option<String>,
    pub artwork_id: u64,
    pub artwork_frame: Option<DecodedFrame>,
    pub accent: Color,
    pub accent_from: Color,
    pub accent_to: Color,
    pub accent_t: f64,
    pub equalizer_phase: f64,
    pub equalizer_bars: [f64; 5],
    pub seeking: bool,
    pub seek_fraction: f64,
    pub lyrics: Option<Lyrics>,
    pub lyrics_track: Option<String>,
    pub lyric_index: isize,
    pub lyric_scroll_y: f64,
    pub lyric_scroll_from: f64,
    pub lyric_scroll_to: f64,
    pub lyric_scroll_t: f64,
    pub lyric_line_anim: Vec<(f64, f64)>, // (brightness 0..1, scale 1..1.04)
    pub lyric_line_heights: Vec<(f32, f32)>,
    pub lyric_heights_key: (Option<String>, u32),

    // HUD & News & Ambient
    pub hud_queue: HudQueue,
    pub hud_active: bool,
    pub hud_deadline: Option<Instant>,
    pub hud_width: f64,
    pub hud_bar_fill: f64,
    pub hud_bar_target: f64,
    pub chimed_tags: HashSet<String>,
    pub news_active: bool,
    pub news_deadline: Option<Instant>,
    pub news_report: Option<AwayReport>,
    pub news_flight_t: f64,
    pub overlay_is_news: bool,
    pub ambient_shown: bool,
    pub force_ambient: bool,
    pub visor_gaze: GazeState,
    pub visor_sleeping: bool,
    pub zzz_step: usize,
    pub zzz_accum: f64,

    // Companion animators
    pub music_raven: RavenAnimator,
    pub news_raven: RavenAnimator,
    pub hud_raven: RavenAnimator,
    pub hud_raven_land_at: Option<Instant>,
    pub clock_raven: RavenAnimator,
    pub idle_raven: RavenAnimator,
    pub drop_raven: RavenAnimator,
    pub header_cozmo: RavenAnimator,

    // Presence
    pub presence_state: u8, // 0 Active, 1 Idle, 2 Asleep
    pub is_away: bool,
    pub last_idle_secs: f64,
    pub away_items: Vec<AwayItem>,
    pub simulated_idle_secs: Option<f64>,

    // Privacy
    pub camera_apps: Vec<String>,
    pub mic_apps: Vec<String>,
    pub mic_muted: bool,

    // Activities & Bubble
    pub activities: Vec<Activity>,
    pub activity_requests: VecDeque<Instant>,
    pub bubble_item: Option<BubbleItem>,
    pub panel_width: f64,
    pub panel_below: bool,
    pub last_jump_id: Option<String>,
    pub last_jump_at: Option<Instant>,
    pub agent_pulse_phase: f64,
    pub agent_prompt_input: String,
    pub agent_prompt_focused: bool,
    pub copilot_hooks_on: bool,

    // Notification Center
    pub notifications: Vec<NotificationItem>,
    pub notifications_scroll: f64,
    pub next_notif_id: u64,
    pub seen_win_notif_ids: HashSet<u32>,
    pub win_notif_access: NotificationAccess,
    pub notif_pulse_t: f64,

    pub clipboard_items: Vec<ClipboardItem>,
    pub clipboard_scroll: f64,
    pub clipboard_selected: usize,
    pub next_clipboard_id: u64,
    pub last_clipboard_seq: u32,
    pub clipboard_copied_id: Option<u64>,
    pub clipboard_copied_until: Option<Instant>,

    // Calendar, Focus, Water, Tasks, Shelf, Weather, Updates
    pub google_auth: Arc<Mutex<GoogleAuth>>,
    pub calendars: Vec<CalendarInfo>,
    pub calendar_events: Vec<CalendarEvent>,
    pub calendar_sent: HashSet<String>,
    pub calendar_fetching: bool,
    pub calendar_connecting: bool,
    pub calendar_cancel: Option<Arc<AtomicBool>>,
    pub calendar_error: Option<String>,
    pub calendar_last_fetched: Option<NaiveDateTime>,
    pub upcoming_event: Option<CalendarEvent>,
    pub upcoming_text: Option<String>,
    pub agenda_scroll: f64,
    pub calendar_view_year: i32,
    pub calendar_view_month: u32,
    pub calendar_selected_date: NaiveDate,

    pub focus: FocusController,
    pub focus_was_active: bool,
    pub focus_picker_open: bool,
    pub focus_picker_scroll: f64,

    pub tasks_scroll: f64,
    pub task_input: String,
    pub task_input_focused: bool,
    pub tasks_syncing: bool,
    pub tasks_error: Option<String>,
    pub task_lists: Vec<TaskListInfo>,
    pub tasks_last_synced: Option<NaiveDateTime>,

    pub shelf_items: Vec<ShelfEntry>,
    pub shelf_scroll: f64,
    pub next_image_id: u64,
    pub undo: Option<UndoState>,

    pub weather_report: Option<WeatherReport>,
    pub weather_places: Vec<WeatherPlace>,
    pub weather_searching: bool,
    pub weather_last_rain_notice: Option<Instant>,
    pub weather_picker_open: bool,
    pub weather_search_input: String,
    pub weather_search_focused: bool,
    pub weather_anim_phase: f64,

    pub pray_schedule: Option<PrayerSchedule>,
    pub pray_sent_tags: HashSet<String>,
    pub pray_picker_open: bool,
    pub pray_search_input: String,
    pub pray_search_focused: bool,
    pub pray_places: Vec<WeatherPlace>,
    pub pray_anim_phase: f64,

    pub update_checking: bool,
    pub update_available: Option<String>,

    // Watchers & Services
    pub keyboard_watcher: KeyboardWatcher,
    pub power_watcher: PowerWatcher,
    pub media_service: Option<MediaService>,
    pub audio_service: Option<AudioService>,
    pub lyrics_service: Arc<LyricsService>,
    pub stop_flag: Arc<AtomicBool>,

    // UI interaction state
    pub hover: HoverTarget,
    pub pressed: HoverTarget,
    pub cursor_dip: (f64, f64),
    pub window_screen_origin: (i32, i32),
    pub window_scale: f64,
    pub window_w: f64,
    pub window_h: f64,
    pub refresh_angle: f64,
    pub reduce_motion: bool,
    pub dirty: bool,
    pub store_dirty_at: Option<Instant>,
    pub last_os_poll: Instant,
    pub last_clock_sec: u32,

    // Bench & Shots
    pub bench: Option<String>,
    pub bench_variant: Option<String>,
    pub shots_dir: Option<PathBuf>,
    pub frame_timings: FrameTimings,
    pub bench_started_at: Instant,
    pub bench_cycle: i32,
    pub bench_step: usize,
    pub bench_next_step_at: Option<Instant>,
    pub bench_done: bool,
}

impl NotchApp {
    pub fn new(
        bench: Option<String>,
        bench_variant: Option<String>,
        shots_dir: Option<PathBuf>,
        frames_path: Option<PathBuf>,
    ) -> Self {
        let store = if cfg!(test) {
            AppStore::in_memory()
        } else {
            AppStore::load()
        };
        let lang = Language::resolve(store.data.settings.language.as_deref());
        let focus = FocusController::new(&store.data.settings);
        let now_local = Local::now();
        let now_str = now_local.format("%H:%M").to_string();
        let today = now_local.date_naive();
        let reduce_motion = is_motion_reduced();

        let mut app = Self {
            store,
            lang,
            page: NotchPage::Media,
            expanded: false,
            pending_expand: false,
            hover_deadline: None,
            hover_from: (0, 0),
            peeking: false,
            animating: false,
            pinned_until: None,
            hidden_by_user: false,
            fullscreen_app: false,
            session_locked: false,
            capturable: shots_dir.is_some(),
            keyboard_mode: false,
            previous_foreground: 0,
            drop_hinting: false,
            dragging_out: false,
            drag_approaching: false,
            drag_approach: 0.0,
            drag_approach_target: 0.0,
            drag_anim: 0.0,
            drag_phase: 0.0,
            drag_cursor_dip: None,
            drag_file_count: 0,
            drag_opened_notch: false,
            drag_prev_page: None,
            drop_burst_t: 1.0,
            drop_burst_pos: None,
            shelf_flash_t: 0.0,
            shelf_flash_count: 0,
            drop_celebrate_count: 0,

            width: Spring::new(COMPACT_IDLE.0, 0.1),
            height: Spring::new(COMPACT_IDLE.1, 0.1),
            radius: Spring::new(COMPACT_RADIUS, 0.1),
            expand: Spring::new(0.0, 0.002),
            hud: Spring::new(0.0, 0.002),
            ambient: Spring::new(0.0, 0.002),
            bubble: Spring::new(0.0, 0.002),

            tab_slot_w: std::array::from_fn(|i| {
                let mut s = Spring::new(if i == 0 { 76.0 } else { TAB_INACTIVE_W }, 0.1);
                s.stiffness = 460.0;
                s.damping_ratio = 0.82;
                s
            }),
            tab_pill_x: {
                let mut s = Spring::new(0.0, 0.1);
                s.stiffness = 480.0;
                s.damping_ratio = 0.80;
                s
            },
            tab_pill_w: {
                let mut s = Spring::new(76.0, 0.1);
                s.stiffness = 480.0;
                s.damping_ratio = 0.80;
                s
            },
            tab_hover_anim: [0.0; 10],
            gear_hover_anim: 0.0,
            tab_bounce_t: 1.0,
            page_slide_dir: 0.0,
            page_slide_t: 1.0,

            compact_text: now_str,
            compact_incoming: None,
            compact_swap_phase: 0.0,
            compact_marquee_offset: 0.0,
            compact_marquee_t: 0.0,
            compact_marquee_active: false,
            title_marquee_offset: 0.0,
            title_marquee_t: 0.0,
            title_marquee_active: false,
            title_reveal_at: None,

            media_state: None,
            next_source: None,
            artwork_id: 0,
            artwork_frame: None,
            accent: DEFAULT_ACCENT,
            accent_from: DEFAULT_ACCENT,
            accent_to: DEFAULT_ACCENT,
            accent_t: 1.0,
            equalizer_phase: 0.0,
            equalizer_bars: [0.18; 5],
            seeking: false,
            seek_fraction: 0.0,
            lyrics: None,
            lyrics_track: None,
            lyric_index: -1,
            lyric_scroll_y: 0.0,
            lyric_scroll_from: 0.0,
            lyric_scroll_to: 0.0,
            lyric_scroll_t: 1.0,
            lyric_line_anim: Vec::new(),
            lyric_line_heights: Vec::new(),
            lyric_heights_key: (None, 0),

            hud_queue: HudQueue::default(),
            hud_active: false,
            hud_deadline: None,
            hud_width: 0.0,
            hud_bar_fill: 0.0,
            hud_bar_target: 0.0,
            chimed_tags: HashSet::new(),
            news_active: false,
            news_deadline: None,
            news_report: None,
            news_flight_t: 1.0,
            overlay_is_news: false,
            ambient_shown: false,
            force_ambient: false,
            visor_gaze: GazeState::default(),
            visor_sleeping: false,
            zzz_step: 0,
            zzz_accum: 0.0,

            music_raven: RavenAnimator::default(),
            news_raven: RavenAnimator::default(),
            hud_raven: RavenAnimator::default(),
            hud_raven_land_at: None,
            clock_raven: RavenAnimator::default(),
            idle_raven: RavenAnimator::default(),
            drop_raven: RavenAnimator::default(),
            header_cozmo: RavenAnimator::default(),

            presence_state: 0,
            is_away: false,
            last_idle_secs: 0.0,
            away_items: Vec::new(),
            simulated_idle_secs: None,

            camera_apps: Vec::new(),
            mic_apps: Vec::new(),
            mic_muted: false,

            activities: Vec::new(),
            activity_requests: VecDeque::new(),
            bubble_item: None,
            panel_width: 320.0,
            panel_below: false,
            last_jump_id: None,
            last_jump_at: None,
            agent_pulse_phase: 0.0,
            agent_prompt_input: String::new(),
            agent_prompt_focused: false,
            copilot_hooks_on: false,

            notifications: Vec::new(),
            notifications_scroll: 0.0,
            next_notif_id: 1,
            seen_win_notif_ids: HashSet::new(),
            win_notif_access: NotificationAccess::Unspecified,
            notif_pulse_t: 0.0,

            clipboard_items: Vec::new(),
            clipboard_scroll: 0.0,
            clipboard_selected: 0,
            next_clipboard_id: 1,
            last_clipboard_seq: 0,
            clipboard_copied_id: None,
            clipboard_copied_until: None,

            google_auth: Arc::new(Mutex::new(GoogleAuth::new())),
            calendars: Vec::new(),
            calendar_events: Vec::new(),
            calendar_sent: HashSet::new(),
            calendar_fetching: false,
            calendar_connecting: false,
            calendar_cancel: None,
            calendar_error: None,
            calendar_last_fetched: None,
            upcoming_event: None,
            upcoming_text: None,
            agenda_scroll: 0.0,
            calendar_view_year: today.year(),
            calendar_view_month: today.month(),
            calendar_selected_date: today,

            focus,
            focus_was_active: false,
            focus_picker_open: false,
            focus_picker_scroll: 0.0,

            tasks_scroll: 0.0,
            task_input: String::new(),
            task_input_focused: false,
            tasks_syncing: false,
            tasks_error: None,
            task_lists: Vec::new(),
            tasks_last_synced: None,

            shelf_items: Vec::new(),
            shelf_scroll: 0.0,
            next_image_id: 1000,
            undo: None,

            weather_report: None,
            weather_places: Vec::new(),
            weather_searching: false,
            weather_last_rain_notice: None,
            weather_picker_open: false,
            weather_search_input: String::new(),
            weather_search_focused: false,
            weather_anim_phase: 0.0,

            pray_schedule: None,
            pray_sent_tags: HashSet::new(),
            pray_picker_open: false,
            pray_search_input: String::new(),
            pray_search_focused: false,
            pray_places: Vec::new(),
            pray_anim_phase: 0.0,

            update_checking: false,
            update_available: None,

            keyboard_watcher: KeyboardWatcher::new(),
            power_watcher: PowerWatcher::new(),
            media_service: None,
            audio_service: None,
            lyrics_service: Arc::new(LyricsService::new()),
            stop_flag: Arc::new(AtomicBool::new(false)),

            hover: HoverTarget::None,
            pressed: HoverTarget::None,
            cursor_dip: (0.0, 0.0),
            window_screen_origin: (0, 0),
            window_scale: 1.0,
            window_w: 644.0,
            window_h: 120.0,
            refresh_angle: 0.0,
            reduce_motion,
            dirty: true,
            store_dirty_at: None,
            last_os_poll: Instant::now(),
            last_clock_sec: now_local.second(),

            bench,
            bench_variant,
            shots_dir,
            frame_timings: FrameTimings::new(frames_path),
            bench_started_at: Instant::now(),
            bench_cycle: 1,
            bench_step: 0,
            bench_next_step_at: None,
            bench_done: false,
        };

        if !cfg!(test) && app.bench.is_none() && app.shots_dir.is_none() {
            let setup = mochi_winrt::agent_setup::AgentSetup::for_this_user();
            if !app.store.data.settings.copilot_hooks_initialized
                && app.store.data.settings.activities_enabled
                && mochi_winrt::agent_setup::AgentSetup::has_cli()
                && setup.status("copilot") == mochi_winrt::agent_setup::AgentStatus::Off
            {
                let _ = setup.set("copilot", true);
                app.store.data.settings.copilot_hooks_initialized = true;
                app.store.save_now();
            } else if setup.status("copilot") == mochi_winrt::agent_setup::AgentStatus::On {
                setup.upgrade_copilot_if_needed();
            }
            app.copilot_hooks_on =
                setup.status("copilot") == mochi_winrt::agent_setup::AgentStatus::On;
        }
        app.load_shelf_from_store();
        app.load_clipboard_from_store();
        app.recompute_prayer_schedule();
        app.sync_tab_dock_targets(None);
        app.update_companion();
        app.update_targets(None);
        app.snap_all_springs();
        app
    }

    pub fn snap_all_springs(&mut self) {
        self.width.snap();
        self.height.snap();
        self.radius.snap();
        self.expand.snap();
        self.hud.snap();
        self.ambient.snap();
        self.bubble.snap();
        self.drag_anim = if self.drop_hinting { 1.0 } else { 0.0 };
        self.drag_approach = if self.drag_approaching {
            self.drag_approach_target
        } else {
            0.0
        };
        for s in &mut self.tab_slot_w {
            s.snap();
        }
        self.tab_pill_x.snap();
        self.tab_pill_w.snap();
        self.tab_bounce_t = 1.0;
        self.page_slide_t = 1.0;
        self.accent = self.accent_to;
        self.accent_t = 1.0;
        self.hud_bar_fill = self.hud_bar_target;
        self.lyric_scroll_y = self.lyric_scroll_to;
        self.lyric_scroll_t = 1.0;
        if let Some(inc) = self.compact_incoming.take() {
            self.compact_text = inc;
        }
        self.compact_swap_phase = 0.0;
        for (i, anim) in self.lyric_line_anim.iter_mut().enumerate() {
            if i as isize == self.lyric_index {
                *anim = (1.0, 1.04);
            } else {
                *anim = (0.0, 1.0);
            }
        }
        if self.visor_sleeping {
            self.visor_gaze.norm_x = 0.0;
            self.visor_gaze.norm_y = 0.2;
            self.visor_gaze.scale_x = 1.0;
            self.visor_gaze.scale_y = 0.12;
        }
    }

    pub fn is_visible(&self) -> bool {
        !(self.session_locked
            || self.hidden_by_user
            || (self.fullscreen_app && self.store.data.settings.hide_in_fullscreen))
    }

    pub fn privacy_shown(&self) -> bool {
        self.store.data.settings.hud_privacy
            && (!self.camera_apps.is_empty() || !self.mic_apps.is_empty())
    }

    pub fn set_expanded(&mut self, expanded: bool, factories: Option<&D2dFactories>) {
        if self.expanded == expanded {
            return;
        }
        self.expanded = expanded;
        self.peeking = false;
        if expanded {
            self.interrupt_hud();
            self.hide_news(factories);
            self.clock_raven.stop();
            self.header_cozmo.perch();
            self.sync_tab_dock_targets(factories);
            self.compact_marquee_active = false;
            self.compact_marquee_offset = 0.0;
            self.compact_swap_phase = 0.0;
            self.compact_incoming = None;
            self.title_reveal_at = Some(Instant::now() + Duration::from_millis(450));
        } else {
            self.header_cozmo.stop();
            self.tab_hover_anim = [0.0; 10];
            self.gear_hover_anim = 0.0;
            self.title_reveal_at = None;
            self.title_marquee_active = false;
            self.title_marquee_offset = 0.0;
            self.compact_marquee_active = false;
            self.compact_marquee_offset = 0.0;
            self.compact_swap_phase = 0.0;
            self.compact_incoming = None;
            self.task_input_focused = false;
            self.agent_prompt_focused = false;
            self.weather_search_focused = false;
            self.weather_picker_open = false;
            self.pray_search_focused = false;
            self.pray_picker_open = false;
            self.drop_hinting = false;
            self.drag_approaching = false;
            self.drag_approach_target = 0.0;
            self.drag_opened_notch = false;
            self.drag_prev_page = None;
            self.focus_picker_open = false;
        }
        self.update_targets(factories);
        if !expanded {
            self.show_next_hud(factories);
            self.update_companion();
        }
    }

    pub fn tab_label(&self, idx: usize) -> &'static str {
        let s = self.lang.strings();
        match idx {
            1 => s.tab_agenda,
            2 => s.tab_focus,
            3 => s.tab_water,
            4 => s.tab_pray,
            5 => s.tab_tasks,
            6 => s.tab_shelf,
            7 => s.tab_notifications,
            8 => s.tab_clipboard,
            9 => s.tab_weather,
            _ => s.tab_media,
        }
    }

    pub fn tab_active_width(&self, idx: usize, factories: Option<&D2dFactories>) -> f64 {
        let label = self.tab_label(idx);
        let lw = if let Some(f) = factories {
            f64::from(
                f.measure_text(label, FontKind::Ui, 11.0, true, false, 100.0, false)
                    .0,
            )
        } else {
            label.chars().count() as f64 * 6.2
        };
        (22.0 + lw + 10.0).clamp(54.0, 96.0)
    }

    pub fn sync_tab_dock_targets(&mut self, factories: Option<&D2dFactories>) {
        let active = self.page as usize;
        let active_group = usize::from(active >= 5);
        let mut x = 0.0;
        for i in 0..10 {
            if i == 5 {
                x = 0.0;
            }
            let w = if i == active {
                self.tab_active_width(i, factories)
            } else {
                TAB_INACTIVE_W
            };
            self.tab_slot_w[i].target = w;
            if i == active && usize::from(i >= 5) == active_group {
                self.tab_pill_x.target = x;
                self.tab_pill_w.target = w;
            }
            x += w + TAB_GAP;
        }
    }

    pub fn select_page(&mut self, page: NotchPage, factories: Option<&D2dFactories>) {
        if self.page == page {
            return;
        }
        let prev = self.page;
        self.page = page;
        self.task_input_focused = false;
        self.agent_prompt_focused = false;
        self.weather_search_focused = false;
        self.weather_picker_open = false;
        self.pray_search_focused = false;
        self.pray_picker_open = false;
        self.focus_picker_open = false;
        self.update_targets(factories);
        self.sync_tab_dock_targets(factories);
        let animate = self.expand.value > 0.01 && !self.reduce_motion;
        if animate {
            if usize::from((prev as usize) >= 5) != usize::from((page as usize) >= 5) {
                self.tab_pill_x.snap();
                self.tab_pill_w.value = TAB_INACTIVE_W;
                self.tab_pill_w.velocity = 0.0;
            }
            self.tab_bounce_t = 0.0;
            self.page_slide_dir = if page > prev { 1.0 } else { -1.0 };
            self.page_slide_t = 0.0;
        } else {
            for s in &mut self.tab_slot_w {
                s.snap();
            }
            self.tab_pill_x.snap();
            self.tab_pill_w.snap();
            self.tab_bounce_t = 1.0;
            self.page_slide_t = 1.0;
        }
        self.dirty = true;
    }

    pub fn step_page(&mut self, delta: i32, factories: Option<&D2dFactories>) {
        let pages = [
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
        ];
        let cur = self.page as isize;
        let next = (cur + delta as isize).rem_euclid(pages.len() as isize) as usize;
        self.select_page(pages[next], factories);
    }

    pub fn show_page(&mut self, name: &str, seconds: u64, factories: Option<&D2dFactories>) {
        let page = match name.to_ascii_lowercase().as_str() {
            "agenda" | "calendar" | "kalender" => NotchPage::Agenda,
            "focus" | "fokus" => NotchPage::Focus,
            "water" | "air" => NotchPage::Water,
            "pray" | "sholat" | "salat" => NotchPage::Pray,
            "tasks" | "tugas" => NotchPage::Tasks,
            "shelf" | "rak" => NotchPage::Shelf,
            "notifications" | "alerts" | "notifikasi" => NotchPage::Notifications,
            "clipboard" | "clip" | "riwayat" => NotchPage::Clipboard,
            "weather" | "cuaca" => NotchPage::Weather,
            _ => NotchPage::Media,
        };
        self.select_page(page, factories);
        self.capturable = true;
        self.pinned_until = Some(Instant::now() + Duration::from_secs(seconds.clamp(1, 300)));
        self.hover_deadline = None;
        self.set_expanded(true, factories);
    }

    pub fn update_targets(&mut self, factories: Option<&D2dFactories>) {
        let has_media = self.media_state.is_some();
        let has_lyrics = has_media && self.lyrics.is_some();
        let ambient = self.ambient_shown;
        let has_weather = self.weather_report.is_some();
        let compact_weather =
            has_weather && !has_media && !self.focus.is_active() && self.upcoming_text.is_none();
        let show_clock_raven = self.clock_raven.mode != RavenMode::Still
            && !has_media
            && !self.focus.is_active()
            && self.upcoming_text.is_none();

        let mut compact = if ambient {
            COMPACT_AMBIENT
        } else if has_media {
            self.media_compact(factories)
        } else if self.upcoming_text.is_some() {
            self.event_compact(factories)
        } else if self.focus.is_active() {
            COMPACT_IDLE_TIMER
        } else if compact_weather {
            COMPACT_IDLE_WEATHER
        } else if show_clock_raven {
            COMPACT_IDLE_WITH_RAVEN
        } else {
            COMPACT_IDLE
        };

        let dots = if !self.privacy_shown() || ambient {
            0
        } else {
            usize::from(!self.camera_apps.is_empty()) + usize::from(!self.mic_apps.is_empty())
        };
        let privacy_room = if dots == 0 {
            0.0
        } else {
            (dots as f64) * 6.0 + ((dots - 1) as f64) * 3.0 + PRIVACY_GAP
        };
        compact.0 += 2.0 * privacy_room;

        let expanded_sz = match self.page {
            NotchPage::Agenda
            | NotchPage::Pray
            | NotchPage::Tasks
            | NotchPage::Shelf
            | NotchPage::Notifications
            | NotchPage::Clipboard
            | NotchPage::Weather => EXPANDED_WIDE,
            NotchPage::Media if has_lyrics => EXPANDED_WIDE,
            _ => EXPANDED_SHORT,
        };
        self.layout_activity_panel(expanded_sz.0);

        let show_news = self.news_active && !self.expanded;
        let show_hud = !show_news && self.hud_active && !self.expanded;
        if show_news || show_hud {
            self.overlay_is_news = show_news;
        }

        let news_count = self
            .news_report
            .as_ref()
            .map_or(0, |r| r.items.len().min(4));
        let target = if self.expanded {
            expanded_sz
        } else if show_news {
            (NEWS_WIDTH, 50.0 + 22.0 * (news_count as f64))
        } else if show_hud {
            (
                compact
                    .0
                    .max(HUD_MIN_WIDTH.max(self.hud_width) + 2.0 * privacy_room),
                HUD_HEIGHT,
            )
        } else if self.drag_approaching && self.drag_approach_target > 0.01 {
            let ap = self.drag_approach_target.clamp(0.0, 1.0);
            (compact.0.max(210.0) + 24.0 * ap, compact.1 + 12.0 * ap)
        } else if self.peeking {
            (compact.0 + PEEK_WIDTH, compact.1 + PEEK_HEIGHT)
        } else {
            compact
        };

        if let Ok(motion) =
            mochi_spring::port::shape_motion(self.expanded, self.store.data.settings.calm_motion)
        {
            let stiff = motion
                .get("Stiffness")
                .and_then(Value::as_f64)
                .unwrap_or(400.0);
            let damp = motion
                .get("DampingRatio")
                .and_then(Value::as_f64)
                .unwrap_or(1.0);
            for s in [
                &mut self.width,
                &mut self.height,
                &mut self.radius,
                &mut self.expand,
                &mut self.ambient,
            ] {
                s.stiffness = stiff;
                s.damping_ratio = damp;
            }
        }

        self.width.target = target.0;
        self.height.target = target.1;
        self.radius.target = if self.expanded {
            EXPANDED_RADIUS
        } else if show_news {
            22.0
        } else if self.drag_approaching && self.drag_approach_target > 0.01 {
            COMPACT_RADIUS + 5.0 * self.drag_approach_target.clamp(0.0, 1.0)
        } else {
            COMPACT_RADIUS
        };
        self.expand.target = if self.expanded { 1.0 } else { 0.0 };
        self.hud.target = if show_hud || show_news { 1.0 } else { 0.0 };
        self.ambient.target = if ambient { 1.0 } else { 0.0 };
        self.bubble.target = if self.bubble_item.is_some() && !self.expanded {
            1.0
        } else {
            0.0
        };

        if self.expanded && self.page == NotchPage::Media && self.media_state.is_none() {
            self.idle_raven.perch();
        } else {
            self.idle_raven.stop();
        }

        self.fit_window(false);
        self.animating = true;
        self.dirty = true;
    }

    pub fn fit_window(&mut self, allow_shrink: bool) {
        const SIDE_ROOM: f64 = 40.0;
        const BOTTOM_ROOM: f64 = 48.0;
        const OVERSHOOT: f64 = 1.06;
        let bubble_room =
            if (self.bubble_item.is_some() && !self.expanded) || self.bubble.value > 0.001 {
                2.0 * (BUBBLE_GAP + BUBBLE_WIDTH + EAR)
            } else {
                0.0
            };
        let has_panel = !self.activities.is_empty() && (self.expanded || self.expand.value > 0.001);
        let (pw, ph) = if !has_panel {
            (0.0, 0.0)
        } else if self.panel_below {
            (0.0, BUBBLE_GAP + self.panel_height())
        } else {
            (2.0 * (BUBBLE_GAP + self.panel_width + EAR), 0.0)
        };

        let mut w = self.width.value.max(self.width.target) * OVERSHOOT
            + 2.0 * EAR
            + 2.0 * SIDE_ROOM
            + bubble_room.max(pw);
        let mut h = (self.height.value.max(self.height.target) * OVERSHOOT)
            .max(self.height.value.max(self.height.target) + ph)
            + BOTTOM_ROOM;

        w = w.max(644.0);
        if !self.expanded {
            h = h.max(120.0);
        }
        w = (w / 2.0).ceil() * 2.0;
        h = h.ceil();

        if !allow_shrink {
            w = w.max(self.window_w);
            h = h.max(self.window_h);
        }
        self.window_w = w;
        self.window_h = h;
    }

    pub fn step_animations(&mut self, dt: f64, factories: Option<&D2dFactories>) -> bool {
        let dt = dt.min(0.05);
        if self.reduce_motion {
            self.width.snap();
            self.height.snap();
            self.radius.snap();
            self.expand.snap();
            self.hud.snap();
            self.ambient.snap();
            self.bubble.snap();
        }
        let shape_moving = self.width.step(dt)
            | self.height.step(dt)
            | self.radius.step(dt)
            | self.expand.step(dt)
            | self.hud.step(dt)
            | self.ambient.step(dt)
            | self.bubble.step(dt);
        self.frame_timings.moving = shape_moving;
        let mut moving = shape_moving;

        if self.reduce_motion {
            for s in &mut self.tab_slot_w {
                s.snap();
            }
            self.tab_pill_x.snap();
            self.tab_pill_w.snap();
            self.tab_bounce_t = 1.0;
        } else {
            for s in &mut self.tab_slot_w {
                moving |= s.step(dt);
            }
            moving |= self.tab_pill_x.step(dt);
            moving |= self.tab_pill_w.step(dt);
            if self.tab_bounce_t < 1.0 {
                self.tab_bounce_t = (self.tab_bounce_t + dt / 0.36).min(1.0);
                moving = true;
            }
        }
        let hover_k = 1.0 - (-20.0 * dt).exp();
        for i in 0..10 {
            let target = if self.expanded && self.hover == HoverTarget::Tab(i) {
                1.0
            } else {
                0.0
            };
            if self.reduce_motion {
                self.tab_hover_anim[i] = target;
            } else if (self.tab_hover_anim[i] - target).abs() > 0.004 {
                self.tab_hover_anim[i] += (target - self.tab_hover_anim[i]) * hover_k;
                moving = true;
            } else {
                self.tab_hover_anim[i] = target;
            }
        }
        if self.notif_pulse_t > 0.0 {
            if self.reduce_motion {
                self.notif_pulse_t = 0.0;
            } else {
                self.notif_pulse_t = (self.notif_pulse_t - dt / 0.8).max(0.0);
                moving = true;
            }
        }
        let gear_target = if self.expanded && self.hover == HoverTarget::SettingsBtn {
            1.0
        } else {
            0.0
        };
        if self.reduce_motion {
            self.gear_hover_anim = gear_target;
        } else if (self.gear_hover_anim - gear_target).abs() > 0.004 {
            self.gear_hover_anim += (gear_target - self.gear_hover_anim) * hover_k;
            moving = true;
        } else {
            self.gear_hover_anim = gear_target;
        }
        if self.page_slide_t < 1.0 {
            self.page_slide_t = (self.page_slide_t + dt / 0.24).min(1.0);
            moving = true;
        }
        if self.accent_t < 1.0 {
            self.accent_t = (self.accent_t + dt / 0.5).min(1.0);
            self.accent = self.accent_from.lerp(self.accent_to, self.accent_t as f32);
            moving = true;
        }
        if (self.hud_bar_fill - self.hud_bar_target).abs() > 0.2 {
            let k = 1.0 - (-20.0 * dt).exp();
            self.hud_bar_fill += (self.hud_bar_target - self.hud_bar_fill) * k;
            moving = true;
        } else {
            self.hud_bar_fill = self.hud_bar_target;
        }
        if self.lyric_scroll_t < 1.0 {
            self.lyric_scroll_t = (self.lyric_scroll_t + dt / 0.45).min(1.0);
            let ease = 1.0 - (1.0 - self.lyric_scroll_t).powi(3);
            self.lyric_scroll_y =
                self.lyric_scroll_from + (self.lyric_scroll_to - self.lyric_scroll_from) * ease;
            moving = true;
        }
        for (i, (bright, scale)) in self.lyric_line_anim.iter_mut().enumerate() {
            let target_b = if i as isize == self.lyric_index {
                1.0
            } else {
                0.0
            };
            let target_s = if i as isize == self.lyric_index {
                1.04
            } else {
                1.0
            };
            if (*bright - target_b).abs() > 0.005 || (*scale - target_s).abs() > 0.001 {
                let k = 1.0 - (-10.0 * dt).exp();
                *bright += (target_b - *bright) * k;
                *scale += (target_s - *scale) * k;
                moving = true;
            } else {
                *bright = target_b;
                *scale = target_s;
            }
        }
        if self.compact_swap_phase > 0.0 {
            if self.compact_swap_phase < 1.0 {
                self.compact_swap_phase += dt / 0.14;
                if self.compact_swap_phase >= 1.0 {
                    if let Some(inc) = self.compact_incoming.take() {
                        self.compact_text = inc;
                    }
                    self.compact_swap_phase = 1.0;
                }
            } else {
                self.compact_swap_phase += dt / 0.22;
                if self.compact_swap_phase >= 2.0 {
                    self.compact_swap_phase = 0.0;
                }
            }
            moving = true;
        }
        if self.news_active && self.news_flight_t < 1.0 {
            self.news_flight_t = (self.news_flight_t + dt / 0.75).min(1.0);
            if self.news_flight_t >= 1.0 {
                self.news_raven.land();
            }
            moving = true;
        }

        // Drag-to-notch approach, active drop zone, and drop burst animations
        let approach_target = if self.drag_approaching && !self.expanded {
            self.drag_approach_target.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if self.reduce_motion {
            self.drag_approach = approach_target;
        } else if (self.drag_approach - approach_target).abs() > 0.004 {
            let k = 1.0 - (-18.0 * dt).exp();
            self.drag_approach += (approach_target - self.drag_approach) * k;
            moving = true;
        } else {
            self.drag_approach = approach_target;
        }

        let drag_target = if self.drop_hinting { 1.0 } else { 0.0 };
        if self.reduce_motion {
            self.drag_anim = drag_target;
        } else if (self.drag_anim - drag_target).abs() > 0.004 {
            let rate = if self.drop_hinting { 16.0 } else { 26.0 };
            let k = 1.0 - (-rate * dt).exp();
            self.drag_anim += (drag_target - self.drag_anim) * k;
            moving = true;
        } else {
            self.drag_anim = drag_target;
        }

        if !self.reduce_motion
            && (self.drag_approaching || self.drop_hinting || self.drag_anim > 0.01)
        {
            self.drag_phase = (self.drag_phase + dt * 4.2) % (std::f64::consts::TAU * 100.0);
            self.dirty = true;
        }

        if self.drop_burst_t < 1.0 {
            if self.reduce_motion {
                self.drop_burst_t = 1.0;
            } else {
                self.drop_burst_t = (self.drop_burst_t + dt / 0.92).min(1.0);
                moving = true;
            }
        }
        if self.shelf_flash_t > 0.0 {
            if self.reduce_motion {
                self.shelf_flash_t = 0.0;
            } else {
                self.shelf_flash_t = (self.shelf_flash_t - dt / 1.05).max(0.0);
                moving = true;
            }
        }

        // Equalizer (smooth 100Hz centered pill waveform)
        let eq_running = self.media_state.as_ref().is_some_and(|m| m.is_playing)
            && self.is_visible()
            && !self.expanded
            && self.music_raven.mode == RavenMode::Still
            && !self.reduce_motion;
        if eq_running {
            const ENV: [f64; 5] = [0.68, 0.94, 0.86, 1.0, 0.72];
            const F1: [f64; 5] = [1.0, 1.35, 1.15, 1.45, 0.95];
            const F2: [f64; 5] = [1.7, 2.1, 1.9, 2.3, 1.6];
            const P1: [f64; 5] = [0.0, 1.1, 2.3, 0.6, 1.9];
            const P2: [f64; 5] = [0.7, 2.0, 0.3, 1.5, 2.8];
            self.equalizer_phase =
                (self.equalizer_phase + dt * 5.4) % (std::f64::consts::TAU * 100.0);
            let k = 1.0 - (-18.0 * dt).exp();
            for i in 0..5 {
                let w1 = (self.equalizer_phase * F1[i] + P1[i]).sin() * 0.5 + 0.5;
                let w2 = (self.equalizer_phase * F2[i] + P2[i]).sin() * 0.5 + 0.5;
                let target = (0.22 + ENV[i] * (0.52 * w1 + 0.26 * w2)).clamp(0.20, 1.0);
                self.equalizer_bars[i] += (target - self.equalizer_bars[i]) * k;
            }
            self.dirty = true;
        } else if self.media_state.is_some() && !self.expanded && self.is_visible() {
            let k = 1.0 - (-14.0 * dt).exp();
            let mut settled = true;
            for bar in &mut self.equalizer_bars {
                if self.reduce_motion {
                    *bar = 0.18;
                } else if (*bar - 0.18).abs() > 0.004 {
                    *bar += (0.18 - *bar) * k;
                    settled = false;
                } else {
                    *bar = 0.18;
                }
            }
            if !settled {
                self.dirty = true;
            }
        }

        // Companion animators
        let track_cursor = self.bench.is_none()
            && self.shots_dir.is_none()
            && (self.ambient_shown
                || self.expanded
                || self.clock_raven.mode != RavenMode::Still
                || self.idle_raven.mode != RavenMode::Still);
        let (cursor_px, cursor_off) = if track_cursor {
            let px = get_cursor_pos().map(|(x, y)| (f64::from(x), f64::from(y)));
            let scale = self.window_scale.max(0.5);
            let center_x = f64::from(self.window_screen_origin.0) + (self.window_w * scale) / 2.0;
            let center_y = f64::from(self.window_screen_origin.1) + 15.0 * scale;
            let off = px.map(|(cx, cy)| ((cx - center_x) / scale, (cy - center_y) / scale));
            (px, off)
        } else {
            (None, None)
        };
        if self.ambient_shown && !self.expanded {
            if self.visor_gaze.step(
                dt,
                self.visor_sleeping,
                self.reduce_motion,
                cursor_px,
                cursor_off,
            ) {
                self.dirty = true;
            }
            if self.visor_sleeping {
                self.zzz_accum += dt;
                if self.zzz_accum >= 0.22 {
                    self.zzz_accum -= 0.22;
                    self.zzz_step = (self.zzz_step + 1) % 10;
                    self.dirty = true;
                }
            }
        }
        let drop_cursor_off = self.drag_cursor_dip.map(|(cx, cy)| {
            let ref_x = self.window_w * 0.5 - 48.0;
            let ref_y = 108.0;
            (cx - ref_x, cy - ref_y)
        });
        let header_cursor_off = if self.expanded {
            let pill_cx = self.window_w * 0.5 - self.width.target * 0.5 + 34.0;
            cursor_px.map(|(cx, cy)| {
                let scale = self.window_scale.max(0.5);
                let ox = f64::from(self.window_screen_origin.0) + pill_cx * scale;
                let oy = f64::from(self.window_screen_origin.1) + 24.0 * scale;
                ((cx - ox) / scale, (cy - oy) / scale)
            })
        } else {
            None
        };
        if self.music_raven.tick(dt, self.reduce_motion, None, None)
            | self.news_raven.tick(dt, self.reduce_motion, None, None)
            | self.hud_raven.tick(dt, self.reduce_motion, None, None)
            | self
                .clock_raven
                .tick(dt, self.reduce_motion, cursor_px, cursor_off)
            | self
                .idle_raven
                .tick(dt, self.reduce_motion, cursor_px, cursor_off)
            | self
                .drop_raven
                .tick(dt, self.reduce_motion, cursor_px, drop_cursor_off)
            | self
                .header_cozmo
                .tick(dt, self.reduce_motion, cursor_px, header_cursor_off)
        {
            self.dirty = true;
        }

        if let Some(land_at) = self.hud_raven_land_at
            && Instant::now() >= land_at
        {
            self.hud_raven_land_at = None;
            self.hud_raven.land();
            self.dirty = true;
        }

        if (self.page == NotchPage::Agenda && self.calendar_fetching)
            || (self.page == NotchPage::Tasks && self.tasks_syncing)
            || (self.page == NotchPage::Weather && self.weather_searching)
        {
            self.refresh_angle = (self.refresh_angle + dt * 360.0) % 360.0;
            self.dirty = true;
        } else {
            self.refresh_angle = 0.0;
        }

        if !self.reduce_motion && self.expanded && self.page == NotchPage::Weather {
            self.weather_anim_phase =
                (self.weather_anim_phase + dt * 2.2) % (std::f64::consts::TAU * 100.0);
            self.dirty = true;
        }
        if !self.reduce_motion && self.expanded && self.page == NotchPage::Pray {
            self.pray_anim_phase =
                (self.pray_anim_phase + dt * 1.8) % (std::f64::consts::TAU * 100.0);
            self.dirty = true;
        }

        if !self.reduce_motion
            && self.is_visible()
            && self.activities.iter().any(|a| a.state == "Running")
        {
            self.agent_pulse_phase =
                (self.agent_pulse_phase + dt * 4.5) % (std::f64::consts::TAU * 100.0);
            self.dirty = true;
        }

        if !moving && self.animating {
            self.animating = false;
            self.fit_window(true);
            self.frame_timings.moving = false;
        }
        if moving {
            self.dirty = true;
        }
        let _ = factories;
        moving
    }

    pub fn needs_continuous_frames(&self) -> bool {
        if self.frame_timings.recording || self.bench_next_step_at.is_some() {
            return true;
        }
        if !self.is_visible() {
            return false;
        }
        let tab_dock_moving = self.tab_bounce_t < 1.0
            || self
                .tab_slot_w
                .iter()
                .any(|s| (s.value - s.target).abs() > 0.15 || s.velocity.abs() > 0.5)
            || (self.tab_pill_x.value - self.tab_pill_x.target).abs() > 0.15
            || self.tab_pill_x.velocity.abs() > 0.5
            || (self.tab_pill_w.value - self.tab_pill_w.target).abs() > 0.15
            || self.tab_pill_w.velocity.abs() > 0.5
            || self.tab_hover_anim.iter().any(|&h| h > 0.005 && h < 0.995)
            || (self.gear_hover_anim > 0.005 && self.gear_hover_anim < 0.995);
        self.animating
            || tab_dock_moving
            || self.page_slide_t < 1.0
            || self.accent_t < 1.0
            || self.lyric_scroll_t < 1.0
            || self.compact_swap_phase > 0.0
            || (self.news_active && self.news_flight_t < 1.0)
            || (self.media_state.as_ref().is_some_and(|m| m.is_playing)
                && !self.expanded
                && !self.reduce_motion)
            || (self.media_state.is_some()
                && !self.expanded
                && !self.reduce_motion
                && self
                    .equalizer_bars
                    .iter()
                    .any(|&b| (b - 0.18).abs() > 0.004))
            || (self.ambient_shown && !self.expanded && !self.visor_sleeping)
            || self.clock_raven.mode != RavenMode::Still
            || self.idle_raven.mode != RavenMode::Still
            || self.drop_raven.mode != RavenMode::Still
            || (self.expanded && self.header_cozmo.mode != RavenMode::Still)
            || self.hud_raven.mode != RavenMode::Still
            || self.music_raven.mode != RavenMode::Still
            || self.news_raven.mode != RavenMode::Still
            || self.drag_approaching
            || self.drop_hinting
            || self.drag_approach > 0.005
            || self.drag_anim > 0.005
            || self.drop_burst_t < 1.0
            || self.shelf_flash_t > 0.005
            || self.notif_pulse_t > 0.005
            || (!self.reduce_motion && self.activities.iter().any(|a| a.state == "Running"))
            || (self.expanded && self.page == NotchPage::Focus && self.focus.is_paused())
            || (self.expanded && self.page == NotchPage::Weather && !self.reduce_motion)
            || (self.expanded && self.page == NotchPage::Pray && !self.reduce_motion)
            || (self.page == NotchPage::Agenda && self.calendar_fetching)
            || (self.page == NotchPage::Tasks && self.tasks_syncing)
            || (self.page == NotchPage::Weather && self.weather_searching)
    }

    pub fn set_compact_text(&mut self, text: String) {
        if self.compact_swap_phase > 0.0 && self.compact_swap_phase < 1.0 {
            self.compact_incoming = Some(text);
            return;
        }
        if text == self.compact_text {
            return;
        }
        if self.reduce_motion
            || self.expanded
            || self.expand.value > 0.05
            || self.compact_text.is_empty()
        {
            self.compact_text = text;
            self.compact_swap_phase = 0.0;
            self.compact_incoming = None;
        } else {
            self.compact_incoming = Some(text);
            self.compact_swap_phase = 0.001;
        }
        self.dirty = true;
    }

    pub fn update_clock(&mut self, factories: Option<&D2dFactories>) {
        let now = Local::now().naive_local();
        let time = format!("{:02}:{:02}", now.hour(), now.minute());
        let upcoming_ev = if self.store.data.settings.calendar_in_compact {
            self.calendar_events
                .iter()
                .filter(|e| {
                    !e.all_day && e.start > now && (e.start - now) <= chrono::Duration::minutes(15)
                })
                .min_by_key(|e| e.start)
                .cloned()
        } else {
            None
        };
        self.upcoming_event = upcoming_ev.clone();
        let upcoming = if self.media_state.is_none()
            && let Some(ref ev) = upcoming_ev
        {
            Some(format!("{} {}", ev.title, self.format_until(ev.start, now)))
        } else {
            None
        };
        if upcoming != self.upcoming_text {
            self.upcoming_text = upcoming.clone();
            self.update_targets(factories);
        }
        if self.media_state.is_none() {
            self.set_compact_text(upcoming.unwrap_or(time));
        }
        self.update_bubble();
    }

    pub fn poll_periodic(&mut self, factories: Option<&D2dFactories>) {
        let now_inst = Instant::now();
        let now_local = Local::now().naive_local();
        if self.expanded && now_local.second() != self.last_clock_sec {
            self.last_clock_sec = now_local.second();
            self.dirty = true;
        }

        // Store debounced save
        if let Some(at) = self.store_dirty_at
            && now_inst >= at
        {
            self.store_dirty_at = None;
            self.store.save_now();
        }

        // Undo toast expiry
        if let Some(ref u) = self.undo
            && now_inst >= u.expires_at
        {
            self.undo = None;
            self.dirty = true;
        }

        // Clipboard copied badge expiry
        if let Some(until) = self.clipboard_copied_until
            && now_inst >= until
        {
            self.clipboard_copied_until = None;
            self.clipboard_copied_id = None;
            self.dirty = true;
        }

        // HUD expiry
        if self.hud_active
            && let Some(dl) = self.hud_deadline
            && now_inst >= dl
        {
            self.hide_hud(factories);
        }

        // News expiry
        if self.news_active
            && let Some(dl) = self.news_deadline
            && now_inst >= dl
        {
            self.hide_news(factories);
        }

        // Pinned preview expiry
        if let Some(pin) = self.pinned_until
            && now_inst >= pin
            && self.expanded
            && self.hover == HoverTarget::None
            && !self.seeking
            && !self.task_input_focused
            && !self.agent_prompt_focused
            && !self.weather_search_focused
            && !self.pray_search_focused
        {
            self.pinned_until = None;
            self.set_expanded(false, factories);
        }

        // Hover delay
        if let Some(dl) = self.hover_deadline
            && now_inst >= dl
        {
            self.hover_deadline = None;
            if self.pending_expand {
                if self.hover != HoverTarget::Bubble {
                    if let Some(pt) = get_cursor_pos() {
                        let dx = f64::from(pt.0 - self.hover_from.0);
                        let dy = f64::from(pt.1 - self.hover_from.1);
                        if (dx * dx + dy * dy).sqrt() > 24.0 {
                            self.hover_from = pt;
                            self.hover_deadline = Some(
                                now_inst
                                    + Duration::from_millis(
                                        self.store.data.settings.hover_delay_ms.max(0) as u64,
                                    ),
                            );
                        } else {
                            self.set_expanded(true, factories);
                        }
                    } else {
                        self.set_expanded(true, factories);
                    }
                }
            } else if !self.dragging_out && !self.drop_hinting {
                self.set_expanded(false, factories);
            }
        }

        // Activity expiry
        let before_act = self.activities.len();
        self.activities.retain(|a| a.expires > now_inst);
        if self.activities.len() != before_act {
            self.update_bubble();
            self.update_targets(factories);
        }

        // Focus timer tick
        let prev_focus_secs = self.focus.remaining().as_secs();
        let (focused, focus_msg) = self.focus.tick(&self.store.data.settings, self.lang);
        if self.focus.state == FocusState::Running
            && self.focus.remaining().as_secs() != prev_focus_secs
        {
            self.dirty = true;
        }
        if let Some(dur) = focused {
            let key = Local::now().format("%Y-%m-%d").to_string();
            let secs = dur.as_secs_f64().round() as i32;
            *self.store.data.focus.days.entry(key).or_insert(0) += secs;
            self.store_dirty_at = Some(now_inst + Duration::from_millis(500));
        }
        if let Some(m) = focus_msg {
            self.show_hud(m, factories);
        }
        if self.focus.is_active() != self.focus_was_active {
            self.focus_was_active = self.focus.is_active();
            self.update_targets(factories);
        }

        // Keyboard, Power & Presence (throttled to 100ms so 100Hz animation frames stay jitter-free)
        if now_inst.duration_since(self.last_os_poll) >= Duration::from_millis(100) {
            self.last_os_poll = now_inst;
            for m in self.keyboard_watcher.poll(self.lang) {
                self.show_hud(m, factories);
            }
            if let Some(m) = self.power_watcher.poll(self.lang) {
                self.show_hud(m, factories);
            }

            // Presence
            let idle_secs = self
                .simulated_idle_secs
                .unwrap_or_else(|| (get_idle_ms() as f64) / 1000.0);
            let idle_after =
                f64::from(self.store.data.settings.companion_idle_minutes.max(1)) * 60.0;
            let sleep_after = f64::from(
                self.store
                    .data
                    .settings
                    .companion_sleep_minutes
                    .max(self.store.data.settings.companion_idle_minutes + 1),
            ) * 60.0;
            let next_pres = if idle_secs >= sleep_after {
                2
            } else if idle_secs >= idle_after {
                1
            } else {
                0
            };
            if !self.is_away && idle_secs >= 300.0 {
                self.is_away = true;
                if self.store.data.settings.pause_focus_when_away
                    && !self.media_state.as_ref().is_some_and(|m| m.is_playing)
                {
                    let (paused, focused) = self.focus.pause_after_idle(
                        Duration::from_secs_f64(idle_secs),
                        &self.store.data.settings,
                    );
                    if let Some(dur) = focused {
                        let key = Local::now().format("%Y-%m-%d").to_string();
                        *self.store.data.focus.days.entry(key).or_insert(0) +=
                            dur.as_secs_f64().round() as i32;
                    }
                    if paused {
                        self.away_items.push(AwayItem {
                            glyph: Glyphs::TIMER.to_string(),
                            text: self.lang.strings().away_focus_paused.to_string(),
                            at: now_local,
                        });
                    }
                }
            }
            if next_pres == 0 && idle_secs < self.last_idle_secs && self.last_idle_secs >= 60.0 {
                let items = std::mem::take(&mut self.away_items);
                let dur = Duration::from_secs_f64(self.last_idle_secs);
                self.is_away = false;
                if self.store.data.settings.away_summary_enabled
                    && !items.is_empty()
                    && self.is_visible()
                    && !self.expanded
                {
                    self.show_news(
                        AwayReport {
                            duration: dur,
                            items,
                        },
                        factories,
                    );
                }
            }
            self.last_idle_secs = idle_secs;
            if next_pres != self.presence_state {
                self.presence_state = next_pres;
                self.update_companion();
                self.update_targets(factories);
            }
        }

        // Lyrics sync
        self.update_lyric_line(factories);

        // Prayer schedule & notifications
        self.check_prayer_notifications(now_local, factories);

        // Bench transitions driver
        if self.bench.as_deref() == Some("transicoes")
            && let Some(next_at) = self.bench_next_step_at
            && now_inst >= next_at
            && !self.bench_done
        {
            self.step_bench_transitions(factories);
        }
    }

    pub fn start_bench(&mut self, factories: Option<&D2dFactories>) {
        let Some(ref b) = self.bench.clone() else {
            return;
        };
        match b.as_str() {
            "music" => self.simulate_bench_track(factories),
            "open" => {
                self.simulate_bench_track(factories);
                self.show_page("media", 300, factories);
            }
            "focus" => {
                self.focus.start();
                self.update_targets(factories);
            }
            "sleep" | "sleeping" => {
                self.store.data.settings.companion_enabled = true;
                self.simulated_idle_secs = Some(1800.0);
                self.presence_state = 2;
                self.update_companion();
                self.update_targets(factories);
            }
            "transicoes" | "transitions" => {
                self.simulate_bench_track(factories);
                self.bench_cycle = 1;
                self.bench_step = 0;
                self.bench_next_step_at = Some(Instant::now() + Duration::from_secs(3));
            }
            _ => {}
        }
    }

    fn simulate_bench_track(&mut self, factories: Option<&D2dFactories>) {
        let mut lrc = String::new();
        let mut line = 1;
        let mut secs = 0;
        while secs < 600 {
            lrc.push_str(&format!(
                "[{:02}:{:02}.00] Line {} of benchmark lyrics\n",
                secs / 60,
                secs % 60,
                line
            ));
            line += 1;
            secs += 4;
        }
        self.lyrics_service.seed("Mochi", "Benchmark", &lrc);
        let parsed = Lyrics::parse(&lrc);
        let state = MediaState {
            title: "Benchmark".to_string(),
            artist: "Mochi".to_string(),
            source_app: String::new(),
            is_playing: true,
            position_secs: 0.0,
            duration_secs: 600.0,
            position_updated_at: Instant::now(),
            playback_rate: 1.0,
            thumbnail_bytes: None,
            thumbnail_id: 0,
        };
        self.apply_media_snapshot(Some(state), None, factories);
        self.set_lyrics(parsed, factories);
    }

    fn step_bench_transitions(&mut self, factories: Option<&D2dFactories>) {
        if self.bench_cycle > 12 {
            self.frame_timings.note("fim");
            self.bench_done = true;
            self.bench_next_step_at = None;
            return;
        }
        let labels = [
            "abrir",
            "aba-agenda",
            "aba-tarefas",
            "aba-midia",
            "fechar",
            "hud",
            "hud-sair",
        ];
        let label = labels[self.bench_step];
        self.frame_timings.begin(label, self.bench_cycle);
        match self.bench_step {
            0 => self.show_page("media", 300, factories),
            1 => self.select_page(NotchPage::Agenda, factories),
            2 => self.select_page(NotchPage::Tasks, factories),
            3 => self.select_page(NotchPage::Media, factories),
            4 => self.set_expanded(false, factories),
            5 => {
                let mut m = HudMessage::new(Glyphs::VOLUME_2, "Volume");
                m.value = Some("50".to_string());
                m.level = Some(0.5);
                m.duration_ms = 60_000;
                self.show_hud(m, factories);
            }
            6 => self.hide_hud(factories),
            _ => {}
        }
        self.frame_timings.moving = self.animating;
        self.bench_step += 1;
        if self.bench_step >= labels.len() {
            self.bench_step = 0;
            self.bench_cycle += 1;
        }
        self.bench_next_step_at = Some(Instant::now() + Duration::from_millis(1200));
    }

    pub fn open_external_url(url: &str) {
        open_url(url);
    }

    pub fn open_local_path(path: &str) {
        mochi_winrt::http::open_local_path(path);
    }

    pub fn log_info(area: &str, msg: &str) {
        Log::info(area, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mochi_winrt::clipboard::ClipboardPayload;
    use mochi_winrt::notifications::NotificationEvent;

    #[test]
    fn spring_and_bench_transitions_converge() {
        let mut app = NotchApp::new(
            Some("transicoes".to_string()),
            Some("padrao".to_string()),
            None,
            None,
        );
        app.start_bench(None);
        assert!(app.bench_next_step_at.is_some());
        app.show_page("media", 300, None);
        for _ in 0..200 {
            app.step_animations(0.01, None);
        }
        assert!(app.expanded);
        assert!((app.width.value - EXPANDED_WIDE.0).abs() < 1.0);
        assert!((app.height.value - EXPANDED_WIDE.1).abs() < 1.0);
    }

    #[test]
    fn activity_and_hud_lifecycle() {
        let mut app = NotchApp::new(None, None, None, None);
        let reply = app.handle_activity(
            r#"{"op":"set","id":"build","title":"cargo","text":"compiling","progress":0.5}"#,
            None,
        );
        assert_eq!(reply, "ok");
        assert_eq!(app.activities.len(), 1);
        assert!(app.bubble_item.is_some());

        let end_reply = app.handle_activity(r#"{"op":"end","id":"build"}"#, None);
        assert_eq!(end_reply, "ok");
        assert!(app.activities.is_empty());
    }

    #[test]
    fn gaze_does_not_stick_left_when_cursor_is_still() {
        let mut gaze = GazeState::default();
        for _ in 0..120 {
            gaze.step(0.05, false, false, Some((0.0, 0.0)), Some((-322.0, -15.0)));
        }
        assert!(
            gaze.norm_x.abs() < 0.85,
            "gaze should not stay pinned hard left when cursor is still: {}",
            gaze.norm_x
        );
    }

    #[test]
    fn hidden_notch_stops_continuous_frames_and_hud_reserves_privacy_room() {
        let mut app = NotchApp::new(None, None, None, None);
        app.ambient_shown = true;
        assert!(app.needs_continuous_frames());
        app.hidden_by_user = true;
        assert!(!app.needs_continuous_frames());

        app.hidden_by_user = false;
        app.ambient_shown = false;
        app.camera_apps = vec!["obs64.exe".to_string()];
        app.hud_active = true;
        app.hud_width = 220.0;
        app.update_targets(None);
        assert!(
            app.width.target > 220.0,
            "HUD target width should reserve room for privacy dots: {}",
            app.width.target
        );
    }

    #[test]
    fn compact_media_width_adapts_to_lyric_length() {
        let mut app = NotchApp::new(None, None, None, None);
        let lrc = "[00:00.00]Hi\n[00:10.00]This is a much longer lyric line that should widen the compact notch";
        let media = MediaState {
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            source_app: String::new(),
            is_playing: false,
            position_secs: 1.0,
            duration_secs: 60.0,
            position_updated_at: Instant::now(),
            playback_rate: 1.0,
            thumbnail_bytes: None,
            thumbnail_id: 0,
        };
        app.apply_media_snapshot(Some(media.clone()), None, None);
        app.set_lyrics(Lyrics::parse(lrc), None);
        let short_w = app.width.target;

        let mut media_long = media;
        media_long.position_secs = 12.0;
        app.apply_media_snapshot(Some(media_long), None, None);
        let long_w = app.width.target;

        assert!(
            long_w > short_w + 100.0,
            "expected long lyric width ({long_w}) > short lyric width ({short_w})"
        );
    }

    #[test]
    fn smooth_equalizer_animates_and_settles_on_pause() {
        let mut app = NotchApp::new(None, None, None, None);
        let mut media = MediaState {
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            source_app: String::new(),
            is_playing: true,
            position_secs: 1.0,
            duration_secs: 60.0,
            position_updated_at: Instant::now(),
            playback_rate: 1.0,
            thumbnail_bytes: None,
            thumbnail_id: 0,
        };
        app.apply_media_snapshot(Some(media.clone()), None, None);
        for _ in 0..30 {
            app.step_animations(0.01, None);
        }
        assert!(app.equalizer_bars.iter().any(|&b| b > 0.25));

        media.is_playing = false;
        app.apply_media_snapshot(Some(media), None, None);
        assert!(app.needs_continuous_frames());
        for _ in 0..120 {
            app.step_animations(0.01, None);
        }
        assert!(app.equalizer_bars.iter().all(|&b| (b - 0.18).abs() < 0.01));
        assert!(!app.needs_continuous_frames());
    }

    #[test]
    fn calendar_month_navigation_and_date_filtering() {
        let mut app = NotchApp::new(None, None, None, None);
        app.calendar_view_year = 2026;
        app.calendar_view_month = 12;
        app.step_calendar_month(1);
        assert_eq!((app.calendar_view_year, app.calendar_view_month), (2027, 1));
        app.step_calendar_month(-1);
        assert_eq!(
            (app.calendar_view_year, app.calendar_view_month),
            (2026, 12)
        );

        let target_day = NaiveDate::from_ymd_opt(2026, 12, 25).unwrap();
        app.calendar_events.push(CalendarEvent {
            id: "ev1".to_string(),
            title: "Holiday Sync".to_string(),
            start: target_day.and_hms_opt(10, 0, 0).unwrap(),
            end: target_day.and_hms_opt(11, 0, 0).unwrap(),
            all_day: false,
            meeting_url: None,
            key: "ev1".to_string(),
        });
        assert!(app.calendar_has_event_on(target_day));
        app.calendar_selected_date = target_day;
        let rows = app.build_agenda_rows_for_selected();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Holiday Sync");
    }

    #[test]
    fn drag_approach_enter_leave_and_drop_lifecycle() {
        let mut app = NotchApp::new(None, None, None, None);
        assert!(!app.expanded);

        // Stage 1: Magnetic approach swells compact notch
        let base_w = app.width.target;
        app.on_drag_approach((322.0, 60.0), 0.8, None);
        assert!(app.drag_approaching);
        assert!(app.width.target > base_w + 20.0);
        assert!(app.needs_continuous_frames());

        // Stage 2: DragEnter opens Shelf and activates drop_hinting
        app.on_drag_enter(3, (322.0, 20.0), None);
        assert!(!app.drag_approaching);
        assert!(app.drop_hinting);
        assert!(app.expanded);
        assert_eq!(app.page, NotchPage::Shelf);
        assert_eq!(app.drag_file_count, 3);
        assert!(app.drag_opened_notch);

        // DragLeave collapses back if drag opened the notch
        app.on_drag_leave(None);
        assert!(!app.drop_hinting);
        assert!(!app.expanded);

        // Stage 3: Drop triggers burst ripple and keeps Shelf open
        app.on_drag_enter(1, (322.0, 20.0), None);
        let tmp = std::env::temp_dir().join("mochi-drag-test.txt");
        let _ = std::fs::write(&tmp, b"hello");
        app.on_drag_drop(
            &[tmp.to_string_lossy().into_owned()],
            Some((322.0, 100.0)),
            None,
        );
        let _ = std::fs::remove_file(&tmp);
        assert!(!app.drop_hinting);
        assert!(app.expanded);
        assert_eq!(app.page, NotchPage::Shelf);
        assert_eq!(app.drop_burst_t, 0.0);
        assert!(app.shelf_flash_t > 0.9);
        assert_eq!(app.shelf_flash_count, 1);
        for _ in 0..120 {
            app.step_animations(0.01, None);
        }
        assert!((app.drop_burst_t - 1.0).abs() < 0.01);
        assert!(app.shelf_flash_t < 0.01);
    }

    #[test]
    fn notification_center_records_and_deduplicates_windows_toasts() {
        let mut app = NotchApp::new(None, None, None, None);
        let now = Local::now().naive_local();
        let sn = mochi_winrt::notifications::SystemNotification {
            win_id: 42,
            app_name: "WhatsApp".to_string(),
            title: "Alex".to_string(),
            body: "Ping".to_string(),
            timestamp: now,
        };
        app.handle_notification_event(NotificationEvent::Added(sn.clone()), None);
        app.handle_notification_event(NotificationEvent::Added(sn), None);
        assert_eq!(app.notifications.len(), 1);
        assert_eq!(app.unread_notifications_count(), 1);
        assert!(app.hud_active);

        app.mark_all_notifications_read();
        assert_eq!(app.unread_notifications_count(), 0);

        app.handle_notification_event(NotificationEvent::Removed(42), None);
        assert!(app.notifications.is_empty());
    }

    #[test]
    fn clipboard_history_captures_dedups_pins_and_clears() {
        let mut app = NotchApp::new(None, None, None, None);
        app.capture_clipboard_payload(
            10,
            ClipboardPayload::Text("first snippet".to_string()),
            None,
        );
        app.capture_clipboard_payload(
            11,
            ClipboardPayload::Text("second\nmultiline".to_string()),
            None,
        );
        assert_eq!(app.clipboard_items.len(), 2);
        assert_eq!(app.clipboard_items[0].title, "second");

        // Duplicate text moves back to top without creating a duplicate entry
        app.capture_clipboard_payload(
            12,
            ClipboardPayload::Text("first snippet".to_string()),
            None,
        );
        assert_eq!(app.clipboard_items.len(), 2);
        assert_eq!(app.clipboard_items[0].title, "first snippet");

        // Pinning keeps item at top even when new items arrive
        app.toggle_pin_clipboard_item(0);
        assert!(app.clipboard_items[0].pinned);
        app.capture_clipboard_payload(
            13,
            ClipboardPayload::Text("third snippet".to_string()),
            None,
        );
        assert_eq!(app.clipboard_items.len(), 3);
        assert_eq!(app.clipboard_items[0].title, "first snippet");
        assert_eq!(app.clipboard_items[1].title, "third snippet");

        // Clear removes unpinned items first, keeping pinned items
        app.clear_clipboard_history();
        assert_eq!(app.clipboard_items.len(), 1);
        assert!(app.clipboard_items[0].pinned);

        // Second clear removes remaining pinned items
        app.clear_clipboard_history();
        assert!(app.clipboard_items.is_empty());
    }

    #[test]
    fn prayer_schedule_and_notifications_fire_once_per_prayer() {
        let mut app = NotchApp::new(None, None, None, None);
        app.recompute_prayer_schedule();
        let sched = app.pray_schedule.clone().expect("prayer schedule");
        assert_eq!(sched.prayers.len(), 5);

        let target = sched.prayers[3].time;
        app.check_prayer_notifications(target + chrono::Duration::seconds(5), None);
        assert!(app.hud_active);
        assert_eq!(app.notifications.len(), 1);

        // Calling again within the 90s window does not duplicate the notification
        app.check_prayer_notifications(target + chrono::Duration::seconds(20), None);
        assert_eq!(app.notifications.len(), 1);
    }
}
