use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, BeginPaint, BitBlt, EndPaint, InvalidateRect,
    PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow,
    GetClientRect, GetSystemMetrics, HICON, ICON_BIG, ICON_SMALL, IDC_ARROW, LoadCursorW,
    PostMessageW, RegisterClassExW, SM_CXSCREEN, SM_CYSCREEN, SW_RESTORE, SW_SHOW, SendMessageW,
    SetForegroundWindow, ShowWindow, WM_APP, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETICON, WM_SIZE,
    WNDCLASSEXW, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPEDWINDOW, WS_SYSMENU,
};
use windows_core::PCWSTR;

use mochi_winrt::agent_setup::{
    AgentSetup, AgentStatus, UserPath, copy_text_to_clipboard, diagnostics_summary,
};
use mochi_winrt::calendar::{CalendarEvent, CalendarInfo, fetch_calendars_and_events};
use mochi_winrt::google::GoogleAuth;
use mochi_winrt::location::locate_once;
use mochi_winrt::monitors::Monitors;
use mochi_winrt::native::{Autostart, to_wide};
use mochi_winrt::paths::AppPaths;
use mochi_winrt::strings::{Language, fmt1};
use mochi_winrt::tasks::{RemoteTask, TaskListInfo, fetch_google_task_lists, fetch_google_tasks};
use mochi_winrt::updates::{
    DownloadedUpdate, download_and_verify_update, fetch_latest_release_tag, launch_silent_installer,
};
use mochi_winrt::weather::{WeatherPlace, WeatherReport, fetch_weather, search_weather_places};

use crate::d2d::{D2dFactories, D2dSurface, FontKind, HAlign, VAlign};
use crate::render::RectF;
use crate::state::NotchApp;
use crate::theme::{
    CARD_BG, Color, DIVIDER, HOVER_FILL, INVERSE_FILL, INVERSE_TEXT, MOCHI_ICO, PILL_FILL,
    PILL_FOREGROUND, SELECTED_FILL, SUBTLE_FILL, TEXT_HINT, TEXT_PLACEHOLDER, TEXT_PRIMARY,
    TEXT_SECONDARY, TRACK, WARNING_BRUSH, WINDOW_BG,
};
use crate::window::load_ico_from_bytes;

const WM_GOOGLE_DONE: u32 = WM_APP + 10;
const WM_SETTINGS_ASYNC: u32 = WM_APP + 11;

type GoogleOAuthResult = Result<(serde_json::Value, Vec<&'static str>, bool), String>;

enum SettingsAsyncResult {
    UpdatesChecked(Option<String>),
    UpdateInstallDone(Result<DownloadedUpdate, String>),
    WeatherSearched(Vec<WeatherPlace>),
    WeatherLocated(Option<(f64, f64)>),
    WeatherFetched(Option<WeatherReport>),
    CalendarSynced(Result<(Vec<CalendarInfo>, Vec<CalendarEvent>), String>),
    TasksSynced(Result<(Vec<TaskListInfo>, String, Vec<RemoteTask>), String>),
}

#[derive(Debug, Clone, Default)]
struct CachedGoogleStatus {
    configured: bool,
    signed_in: bool,
    has_cal: bool,
    has_tasks: bool,
    email: String,
}

#[derive(Debug, Clone)]
struct CachedAgentStatus {
    copilot: AgentStatus,
    claude: AgentStatus,
    cursor: AgentStatus,
    codex: AgentStatus,
    on_path: bool,
}

impl CachedAgentStatus {
    fn load() -> Self {
        let setup = AgentSetup::for_this_user();
        let cli_dir = AgentSetup::cli_dir();
        Self {
            copilot: setup.status("copilot"),
            claude: setup.status("claude"),
            cursor: setup.status("cursor"),
            codex: setup.status("codex"),
            on_path: AgentSetup::has_cli() && UserPath::contains(&cli_dir),
        }
    }
}

fn load_google_status(app: &NotchApp, reload_client: bool) -> CachedGoogleStatus {
    app.google_auth
        .lock()
        .map(|mut g| {
            let configured = if reload_client {
                g.reload_client()
            } else {
                g.is_configured()
            };
            let signed_in = g.is_signed_in();
            let has_cal = signed_in
                && g.has_scopes(&[GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE]);
            let has_tasks = signed_in && g.has_scopes(&[GoogleAuth::TASKS_SCOPE]);
            let email = g
                .credential
                .as_ref()
                .map(|c| c.email.clone())
                .unwrap_or_default();
            CachedGoogleStatus {
                configured,
                signed_in,
                has_cal,
                has_tasks,
                email,
            }
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettingAction {
    Nav(usize),
    WelcomeDone,
    ToggleWaterReminders,
    ToggleUpdateCheck,
    ToggleAutoUpdate,
    CycleLanguage,
    ToggleAutostart,
    CycleMonitor,
    ToggleHotkey,
    ToggleJumpHotkey,
    ToggleMicHotkey,
    ToggleHideFullscreen,
    ToggleHideCapture,
    StepHoverDelay(i32),
    ToggleCalmMotion,
    ToggleAccent,
    CheckUpdatesNow,
    InstallUpdate,
    ToggleLyrics,
    GoogleConnectOrDisconnect,
    GoogleCancel,
    CalendarAllow,
    StepCalendarReminder(i32),
    ToggleCalendarCompact,
    TasksAllowOrToggle,
    ToggleWeather,
    FocusWeatherSearch,
    SearchWeather,
    LocateWeather,
    PickWeatherPlace(usize),
    ToggleRainAlerts,
    ToggleSounds,
    ToggleQuietCalls,
    ToggleHudVolume,
    ToggleHudBrightness,
    ToggleHudPower,
    ToggleHudAudioDevice,
    ToggleHudCapsLock,
    ToggleHudKeyboard,
    ToggleHudNetwork,
    ToggleHudPrivacy,
    ToggleActivities,
    StepFocusMinutes(i32),
    StepShortBreak(i32),
    StepLongBreak(i32),
    StepSessionsPerCycle(i32),
    StepFocusGoal(i32),
    TogglePauseAway,
    StepWaterGoal(i32),
    StepWaterCup(i32),
    StepWaterReminder(i32),
    ToggleCompanion,
    StepCompanionIdle(i32),
    StepCompanionSleep(i32),
    ToggleAwaySummary,
    ToggleAgent(&'static str),
    ToggleCliPath,
    OpenGoogleDataFolder,
    OpenDataFolder,
    OpenLogsFolder,
    CopyDiagnostics,
}

struct SettingsState {
    app: Rc<RefCell<NotchApp>>,
    factories: Rc<D2dFactories>,
    surface: D2dSurface,
    hwnd: HWND,
    scroll_y: f32,
    max_scroll: f32,
    show_welcome: bool,
    active_section: usize,
    section_ys: Vec<f32>,
    weather_query: String,
    weather_focused: bool,
    hover_action: Option<SettingAction>,
    hits: Vec<(RectF, SettingAction)>,
    google_rx: Option<mpsc::Receiver<GoogleOAuthResult>>,
    google_cancel: Option<Arc<AtomicBool>>,
    google_status: Option<(String, bool)>,
    async_tx: mpsc::Sender<SettingsAsyncResult>,
    async_rx: mpsc::Receiver<SettingsAsyncResult>,
    cached_google: CachedGoogleStatus,
    cached_agents: CachedAgentStatus,
    icon_big: HICON,
    icon_small: HICON,
    on_changed: Rc<dyn Fn()>,
}

thread_local! {
    static SETTINGS_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
}

pub fn open_settings_window(
    app: Rc<RefCell<NotchApp>>,
    factories: Rc<D2dFactories>,
    show_welcome: bool,
    on_changed: Rc<dyn Fn()>,
) {
    if let Some(existing) = SETTINGS_HWND.with(|c| *c.borrow()) {
        // SAFETY: Restoring and focusing existing settings window.
        unsafe {
            let _ = ShowWindow(existing, SW_RESTORE);
            let _ = SetForegroundWindow(existing);
        }
        return;
    }

    let class_name = to_wide("MochiSettingsWindow");
    // SAFETY: GetModuleHandleW(None) returns current module handle.
    let hinst = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(settings_wnd_proc),
        hInstance: hinst.into(),
        // SAFETY: Loading standard arrow cursor.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    // SAFETY: Registering window class once.
    unsafe {
        RegisterClassExW(&wc);
    }

    let title = {
        let a = app.borrow();
        to_wide(a.lang.strings().settings_title)
    };
    // SAFETY: Querying primary screen dimensions for centering.
    let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let w = 780;
    let h = 760;
    let x = (sw - w) / 2;
    let y = (sh - h) / 2;

    let surface = match D2dSurface::new(&factories, w, h, 1.0) {
        Ok(s) => s,
        Err(_) => return,
    };
    let icon_big = load_ico_from_bytes(MOCHI_ICO, 32);
    let icon_small = load_ico_from_bytes(MOCHI_ICO, 16);
    let (async_tx, async_rx) = mpsc::channel();
    let cached_google = {
        let a = app.borrow();
        load_google_status(&a, true)
    };
    let cached_agents = CachedAgentStatus::load();
    let state = Box::new(SettingsState {
        app,
        factories,
        surface,
        hwnd: HWND::default(),
        scroll_y: 0.0,
        max_scroll: 0.0,
        show_welcome,
        active_section: 0,
        section_ys: Vec::new(),
        weather_query: String::new(),
        weather_focused: false,
        hover_action: None,
        hits: Vec::new(),
        google_rx: None,
        google_cancel: None,
        google_status: None,
        async_tx,
        async_rx,
        cached_google,
        cached_agents,
        icon_big,
        icon_small,
        on_changed,
    });
    let ptr = Box::into_raw(state);

    // SAFETY: Creating overlapped settings window with `ptr` passed via lpParam.
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(hinst.into()),
            Some(ptr.cast()),
        )
    };
    let Ok(hwnd) = hwnd else {
        // SAFETY: Reclaiming `ptr` if CreateWindowExW failed.
        unsafe {
            drop(Box::from_raw(ptr));
        }
        return;
    };
    SETTINGS_HWND.with(|c| *c.borrow_mut() = Some(hwnd));
    let dark: i32 = 1;
    // SAFETY: Enabling dark title bar and setting Mochi window icons.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const dark).cast(),
            std::mem::size_of::<i32>() as u32,
        );
        if !icon_big.is_invalid() {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icon_big.0 as isize)),
            );
        }
        if !icon_small.is_invalid() {
            let _ = SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(icon_small.0 as isize)),
            );
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

// SAFETY: Standard Win32 window procedure for SettingsWindow.
unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        GWLP_USERDATA, GetWindowLongPtrW, SetWindowLongPtrW, WM_NCCREATE,
    };
    if msg == WM_NCCREATE {
        // SAFETY: `lparam` points to CREATESTRUCTW during WM_NCCREATE.
        let cs = unsafe {
            &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW)
        };
        let ptr = cs.lpCreateParams as isize;
        // SAFETY: Storing `SettingsState` pointer in GWLP_USERDATA.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
            if let Some(st) = (ptr as *mut SettingsState).as_mut() {
                st.hwnd = hwnd;
            }
        }
        // SAFETY: Forwarding WM_NCCREATE to DefWindowProcW.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    // SAFETY: Retrieving `SettingsState` pointer from GWLP_USERDATA.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsState;
    if ptr.is_null() {
        // SAFETY: Default window proc.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: `ptr` remains valid until WM_DESTROY frees it.
    let st = unsafe { &mut *ptr };

    match msg {
        WM_SIZE => {
            // SAFETY: Invalidate client area on resize.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: BeginPaint/EndPaint pair for WM_PAINT.
            let hdc = unsafe { BeginPaint(hwnd, &raw mut ps) };
            let mut rc = RECT::default();
            // SAFETY: Querying client rect and DPI.
            let (cw, ch, scale) = unsafe {
                let _ = GetClientRect(hwnd, &raw mut rc);
                let dpi = GetDpiForWindow(hwnd);
                (
                    (rc.right - rc.left).max(1),
                    (rc.bottom - rc.top).max(1),
                    if dpi > 0 { f64::from(dpi) / 96.0 } else { 1.0 },
                )
            };
            st.surface.ensure_size(cw, ch, scale);
            if st.surface.begin_draw() {
                draw_settings(st, cw as f32 / scale as f32, ch as f32 / scale as f32);
                st.surface.end_draw();
                // SAFETY: Blitting rendered DIB from `st.surface.mem_dc()` to window DC.
                unsafe {
                    let _ = BitBlt(hdc, 0, 0, cw, ch, Some(st.surface.mem_dc()), 0, 0, SRCCOPY);
                }
            }
            // SAFETY: Ending paint.
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            let _ = (
                AC_SRC_OVER,
                AC_SRC_ALPHA,
                BLENDFUNCTION::default(),
                POINT::default(),
                SIZE::default(),
            );
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as i16) as f32 / 120.0;
            st.scroll_y = (st.scroll_y - delta * 48.0).clamp(0.0, st.max_scroll);
            // SAFETY: Requesting repaint.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let scale = if dpi > 0 {
                f32::from(dpi as u16) / 96.0
            } else {
                1.0
            };
            let x = ((lparam.0 & 0xFFFF) as i16) as f32 / scale;
            let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as f32 / scale;
            let next_hover = st
                .hits
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, y))
                .map(|(_, a)| a.clone());
            if next_hover != st.hover_action {
                st.hover_action = next_hover;
                // SAFETY: Requesting repaint.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let scale = if dpi > 0 {
                f32::from(dpi as u16) / 96.0
            } else {
                1.0
            };
            let x = ((lparam.0 & 0xFFFF) as i16) as f32 / scale;
            let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as f32 / scale;
            if let Some((_, act)) = st
                .hits
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, y))
                .cloned()
            {
                handle_setting_action(st, act);
                // SAFETY: Requesting repaint.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            } else {
                st.weather_focused = false;
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => LRESULT(0),
        WM_CHAR => {
            if st.weather_focused {
                let ch = char::from_u32((wparam.0 & 0xFFFF) as u32);
                match ch {
                    Some('\x08') => {
                        st.weather_query.pop();
                    }
                    Some('\r') => {
                        let q = st.weather_query.clone();
                        spawn_settings_weather_search(st, q);
                    }
                    Some(c) if !c.is_control() && st.weather_query.chars().count() < 80 => {
                        st.weather_query.push(c);
                    }
                    _ => {}
                }
                // SAFETY: Requesting repaint.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == 0x1B {
                // SAFETY: Closing settings window on Escape.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: Destroying settings window.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_GOOGLE_DONE => {
            st.google_cancel = None;
            if let Some(rx) = st.google_rx.take()
                && let Ok(res) = rx.try_recv()
            {
                let mut app = st.app.borrow_mut();
                let lang = app.lang;
                match res {
                    Ok((doc, wanted, enable_tasks)) => {
                        let ok = app
                            .google_auth
                            .lock()
                            .map_err(|_| "lock".to_string())
                            .and_then(|mut g| g.finish_oauth(doc, &wanted, lang));
                        match ok {
                            Ok(all_granted) => {
                                if enable_tasks {
                                    app.store.data.settings.tasks_sync_enabled = true;
                                    app.store.save_now();
                                }
                                st.google_status = if all_granted {
                                    None
                                } else {
                                    Some((lang.strings().google_some_unchecked.to_string(), true))
                                };
                                st.cached_google = load_google_status(&app, false);
                                drop(app);
                                spawn_settings_calendar_refresh(st);
                                if enable_tasks {
                                    spawn_settings_tasks_refresh(st);
                                }
                                (st.on_changed)();
                            }
                            Err(e) => {
                                st.google_status = Some((e, true));
                            }
                        }
                    }
                    Err(e) => {
                        st.google_status = Some((e, true));
                    }
                }
            }
            // SAFETY: Requesting repaint after Google OAuth completes.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_SETTINGS_ASYNC => {
            let mut notify_host = false;
            while let Ok(res) = st.async_rx.try_recv() {
                match res {
                    SettingsAsyncResult::UpdatesChecked(tag) => {
                        let mut app = st.app.borrow_mut();
                        app.apply_update_tag(tag);
                        app.update_error = None;
                        notify_host = true;
                    }
                    SettingsAsyncResult::UpdateInstallDone(res) => {
                        let mut close_settings = false;
                        {
                            let mut app = st.app.borrow_mut();
                            app.update_installing = false;
                            match res {
                                Ok(downloaded) => {
                                    app.update_error = None;
                                    app.store.save_now();
                                    match launch_silent_installer(&downloaded.installer_path) {
                                        Ok(()) => {
                                            app.update_pending_exit = true;
                                            close_settings = true;
                                            notify_host = true;
                                        }
                                        Err(e) => app.update_error = Some(e),
                                    }
                                }
                                Err(e) => app.update_error = Some(e),
                            }
                        }
                        if close_settings {
                            (st.on_changed)();
                            // SAFETY: Closing Settings window before silent installer replaces binaries.
                            unsafe {
                                let _ = DestroyWindow(hwnd);
                            }
                            return LRESULT(0);
                        }
                    }
                    SettingsAsyncResult::WeatherSearched(places) => {
                        st.app.borrow_mut().apply_weather_places(places);
                    }
                    SettingsAsyncResult::WeatherLocated(coords) => {
                        if let Some((lat, lon)) = coords {
                            {
                                let mut app = st.app.borrow_mut();
                                app.store.data.settings.weather_latitude = Some(lat);
                                app.store.data.settings.weather_longitude = Some(lon);
                                app.store.data.settings.weather_place =
                                    Some(format!("{lat:.2}, {lon:.2}"));
                                app.store.data.settings.weather_enabled = true;
                                app.store.save_now();
                            }
                            spawn_settings_weather_refresh(st);
                            notify_host = true;
                        }
                    }
                    SettingsAsyncResult::WeatherFetched(rep) => {
                        st.app.borrow_mut().apply_weather_report(rep, None);
                        notify_host = true;
                    }
                    SettingsAsyncResult::CalendarSynced(res) => {
                        let mut app = st.app.borrow_mut();
                        app.calendar_fetching = false;
                        match res {
                            Ok((cals, evs)) => {
                                app.calendars = cals;
                                app.calendar_events = evs;
                                app.calendar_last_fetched =
                                    Some(chrono::Local::now().naive_local());
                                app.calendar_error = None;
                            }
                            Err(e) => app.calendar_error = Some(e),
                        }
                        app.dirty = true;
                        notify_host = true;
                    }
                    SettingsAsyncResult::TasksSynced(res) => {
                        let mut app = st.app.borrow_mut();
                        app.tasks_syncing = false;
                        match res {
                            Ok((lists, lid, remote)) => {
                                app.apply_remote_tasks_snapshot(lists, lid, &remote);
                            }
                            Err(e) => app.tasks_error = Some(e),
                        }
                        app.dirty = true;
                        notify_host = true;
                    }
                }
            }
            if notify_host {
                (st.on_changed)();
            }
            // SAFETY: Requesting repaint after async settings work completes.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(ref cancel) = st.google_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            SETTINGS_HWND.with(|c| *c.borrow_mut() = None);
            // SAFETY: Clearing GWLP_USERDATA, destroying icons, and freeing `SettingsState`.
            unsafe {
                if !st.icon_big.is_invalid() {
                    let _ = DestroyIcon(st.icon_big);
                }
                if !st.icon_small.is_invalid() {
                    let _ = DestroyIcon(st.icon_small);
                }
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr));
            }
            mochi_winrt::native::trim_working_set();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn handle_setting_action(st: &mut SettingsState, act: SettingAction) {
    st.weather_focused = matches!(act, SettingAction::FocusWeatherSearch);
    let mut changed = true;
    {
        let mut app = st.app.borrow_mut();
        match act {
            SettingAction::Nav(idx) => {
                if let Some(&y) = st.section_ys.get(idx) {
                    st.scroll_y = y.clamp(0.0, st.max_scroll);
                    st.active_section = idx;
                }
                changed = false;
            }
            SettingAction::WelcomeDone => {
                st.show_welcome = false;
                changed = false;
            }
            SettingAction::ToggleWaterReminders => {
                app.store.data.water.reminders_enabled = !app.store.data.water.reminders_enabled;
            }
            SettingAction::ToggleUpdateCheck => {
                app.store.data.settings.update_check_enabled =
                    !app.store.data.settings.update_check_enabled;
            }
            SettingAction::ToggleAutoUpdate => {
                app.store.data.settings.auto_update_enabled =
                    !app.store.data.settings.auto_update_enabled;
                if app.store.data.settings.auto_update_enabled {
                    app.store.data.settings.update_check_enabled = true;
                }
            }
            SettingAction::CycleLanguage => {
                app.store.data.settings.language = match app.store.data.settings.language.as_deref()
                {
                    None => Some("en".to_string()),
                    Some("en") => Some("id".to_string()),
                    _ => None,
                };
            }
            SettingAction::ToggleAutostart => {
                Autostart::set(!Autostart::is_enabled());
                changed = false;
            }
            SettingAction::CycleMonitor => {
                let all = Monitors::all(app.lang);
                if !all.is_empty() {
                    let cur = app
                        .store
                        .data
                        .settings
                        .monitor
                        .as_deref()
                        .and_then(|m| all.iter().position(|d| d.device_name == m))
                        .unwrap_or(0);
                    let next = (cur + 1) % all.len();
                    app.store.data.settings.monitor = Some(all[next].device_name.clone());
                }
            }
            SettingAction::ToggleHotkey => {
                app.store.data.settings.hotkey_enabled = !app.store.data.settings.hotkey_enabled;
            }
            SettingAction::ToggleJumpHotkey => {
                app.store.data.settings.jump_hotkey_enabled =
                    !app.store.data.settings.jump_hotkey_enabled;
            }
            SettingAction::ToggleMicHotkey => {
                app.store.data.settings.mic_hotkey_enabled =
                    !app.store.data.settings.mic_hotkey_enabled;
            }
            SettingAction::ToggleHideFullscreen => {
                app.store.data.settings.hide_in_fullscreen =
                    !app.store.data.settings.hide_in_fullscreen;
            }
            SettingAction::ToggleHideCapture => {
                app.store.data.settings.hide_from_capture =
                    !app.store.data.settings.hide_from_capture;
            }
            SettingAction::StepHoverDelay(d) => {
                app.store.data.settings.hover_delay_ms =
                    (app.store.data.settings.hover_delay_ms + d).clamp(0, 1000);
            }
            SettingAction::ToggleCalmMotion => {
                app.store.data.settings.calm_motion = !app.store.data.settings.calm_motion;
            }
            SettingAction::ToggleAccent => {
                app.store.data.settings.use_artwork_accent =
                    !app.store.data.settings.use_artwork_accent;
            }
            SettingAction::CheckUpdatesNow => {
                drop(app);
                spawn_settings_update_check(st);
                return;
            }
            SettingAction::InstallUpdate => {
                drop(app);
                spawn_settings_update_install(st);
                return;
            }
            SettingAction::ToggleLyrics => {
                app.store.data.settings.lyrics_enabled = !app.store.data.settings.lyrics_enabled;
            }
            SettingAction::GoogleConnectOrDisconnect => {
                let signed_in = st.cached_google.signed_in;
                if signed_in {
                    let revoke_tok = app
                        .google_auth
                        .lock()
                        .ok()
                        .and_then(|mut g| g.sign_out_local());
                    if let Some(tok) = revoke_tok {
                        std::thread::spawn(move || GoogleAuth::revoke_refresh_token(&tok));
                    }
                    app.calendars.clear();
                    app.calendar_events.clear();
                    app.store.data.settings.tasks_sync_enabled = false;
                    st.google_status = None;
                    st.cached_google = load_google_status(&app, false);
                } else {
                    drop(app);
                    start_google_oauth(
                        st,
                        vec![GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE],
                        false,
                    );
                    return;
                }
            }
            SettingAction::GoogleCancel => {
                if let Some(ref cancel) = st.google_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
                st.google_cancel = None;
                st.google_rx = None;
                st.google_status = None;
                changed = false;
            }
            SettingAction::CalendarAllow => {
                drop(app);
                start_google_oauth(
                    st,
                    vec![GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE],
                    false,
                );
                return;
            }
            SettingAction::StepCalendarReminder(d) => {
                app.store.data.settings.calendar_reminder_minutes =
                    (app.store.data.settings.calendar_reminder_minutes + d).clamp(0, 30);
            }
            SettingAction::ToggleCalendarCompact => {
                app.store.data.settings.calendar_in_compact =
                    !app.store.data.settings.calendar_in_compact;
            }
            SettingAction::TasksAllowOrToggle => {
                let has_tasks = st.cached_google.has_tasks;
                if !has_tasks {
                    drop(app);
                    start_google_oauth(st, vec![GoogleAuth::TASKS_SCOPE], true);
                    return;
                }
                app.store.data.settings.tasks_sync_enabled =
                    !app.store.data.settings.tasks_sync_enabled;
                let should_sync = app.store.data.settings.tasks_sync_enabled;
                app.store.save_now();
                drop(app);
                if should_sync {
                    spawn_settings_tasks_refresh(st);
                }
                (st.on_changed)();
                return;
            }
            SettingAction::ToggleWeather => {
                app.store.data.settings.weather_enabled = !app.store.data.settings.weather_enabled;
                let should_fetch = app.store.data.settings.weather_enabled;
                app.store.save_now();
                drop(app);
                if should_fetch {
                    spawn_settings_weather_refresh(st);
                }
                (st.on_changed)();
                return;
            }
            SettingAction::FocusWeatherSearch => changed = false,
            SettingAction::SearchWeather => {
                let q = st.weather_query.clone();
                drop(app);
                spawn_settings_weather_search(st, q);
                return;
            }
            SettingAction::LocateWeather => {
                drop(app);
                spawn_settings_weather_locate(st);
                return;
            }
            SettingAction::PickWeatherPlace(idx) => {
                if let Some(p) = app.weather_places.get(idx).cloned() {
                    app.store.data.settings.weather_latitude = Some(p.latitude);
                    app.store.data.settings.weather_longitude = Some(p.longitude);
                    app.store.data.settings.weather_place = Some(if p.detail.is_empty() {
                        p.name
                    } else {
                        format!("{}, {}", p.name, p.detail)
                    });
                    app.store.data.settings.weather_enabled = true;
                    app.weather_places.clear();
                    app.store.save_now();
                    drop(app);
                    spawn_settings_weather_refresh(st);
                    (st.on_changed)();
                    return;
                }
            }
            SettingAction::ToggleRainAlerts => {
                app.store.data.settings.weather_rain_alerts =
                    !app.store.data.settings.weather_rain_alerts;
            }
            SettingAction::ToggleSounds => {
                app.store.data.settings.sounds_enabled = !app.store.data.settings.sounds_enabled;
            }
            SettingAction::ToggleQuietCalls => {
                app.store.data.settings.quiet_during_calls =
                    !app.store.data.settings.quiet_during_calls;
            }
            SettingAction::ToggleHudVolume => {
                app.store.data.settings.hud_volume = !app.store.data.settings.hud_volume;
            }
            SettingAction::ToggleHudBrightness => {
                app.store.data.settings.hud_brightness = !app.store.data.settings.hud_brightness;
            }
            SettingAction::ToggleHudPower => {
                app.store.data.settings.hud_power = !app.store.data.settings.hud_power;
            }
            SettingAction::ToggleHudAudioDevice => {
                app.store.data.settings.hud_audio_device =
                    !app.store.data.settings.hud_audio_device;
            }
            SettingAction::ToggleHudCapsLock => {
                app.store.data.settings.hud_caps_lock = !app.store.data.settings.hud_caps_lock;
            }
            SettingAction::ToggleHudKeyboard => {
                app.store.data.settings.hud_keyboard_layout =
                    !app.store.data.settings.hud_keyboard_layout;
            }
            SettingAction::ToggleHudNetwork => {
                app.store.data.settings.hud_network = !app.store.data.settings.hud_network;
            }
            SettingAction::ToggleHudPrivacy => {
                app.store.data.settings.hud_privacy = !app.store.data.settings.hud_privacy;
            }
            SettingAction::ToggleActivities => {
                app.store.data.settings.activities_enabled =
                    !app.store.data.settings.activities_enabled;
            }
            SettingAction::StepFocusMinutes(d) => {
                app.store.data.settings.focus_minutes =
                    (app.store.data.settings.focus_minutes + d).clamp(5, 120);
            }
            SettingAction::StepShortBreak(d) => {
                app.store.data.settings.short_break_minutes =
                    (app.store.data.settings.short_break_minutes + d).clamp(1, 30);
            }
            SettingAction::StepLongBreak(d) => {
                app.store.data.settings.long_break_minutes =
                    (app.store.data.settings.long_break_minutes + d).clamp(5, 60);
            }
            SettingAction::StepSessionsPerCycle(d) => {
                app.store.data.settings.sessions_per_cycle =
                    (app.store.data.settings.sessions_per_cycle + d).clamp(2, 8);
            }
            SettingAction::StepFocusGoal(d) => {
                app.store.data.settings.focus_goal_minutes =
                    (app.store.data.settings.focus_goal_minutes + d).clamp(15, 600);
            }
            SettingAction::TogglePauseAway => {
                app.store.data.settings.pause_focus_when_away =
                    !app.store.data.settings.pause_focus_when_away;
            }
            SettingAction::StepWaterGoal(d) => {
                app.store.data.water.goal_ml = (app.store.data.water.goal_ml + d).clamp(500, 5000);
            }
            SettingAction::StepWaterCup(d) => {
                app.store.data.water.cup_ml = (app.store.data.water.cup_ml + d).clamp(50, 1000);
            }
            SettingAction::StepWaterReminder(d) => {
                app.store.data.water.reminder_minutes =
                    (app.store.data.water.reminder_minutes + d).clamp(15, 240);
            }
            SettingAction::ToggleCompanion => {
                app.store.data.settings.companion_enabled =
                    !app.store.data.settings.companion_enabled;
            }
            SettingAction::StepCompanionIdle(d) => {
                app.store.data.settings.companion_idle_minutes =
                    (app.store.data.settings.companion_idle_minutes + d).clamp(1, 30);
            }
            SettingAction::StepCompanionSleep(d) => {
                app.store.data.settings.companion_sleep_minutes =
                    (app.store.data.settings.companion_sleep_minutes + d).clamp(2, 120);
            }
            SettingAction::ToggleAwaySummary => {
                app.store.data.settings.away_summary_enabled =
                    !app.store.data.settings.away_summary_enabled;
            }
            SettingAction::ToggleAgent(agent) => {
                let setup = AgentSetup::for_this_user();
                let cur = setup.status(agent);
                if cur != AgentStatus::OnByPlugin {
                    let next = cur != AgentStatus::On;
                    if setup.set(agent, next).is_ok()
                        && matches!(agent, "codex" | "claude" | "cursor" | "copilot")
                        && next
                        && AgentSetup::has_cli()
                    {
                        let dir = AgentSetup::cli_dir();
                        if !UserPath::contains(&dir) {
                            UserPath::set(&dir, true);
                        }
                    }
                    if agent == "copilot" {
                        app.store.data.settings.copilot_hooks_initialized = true;
                        app.store.save_now();
                        app.copilot_hooks_on = setup.status("copilot") == AgentStatus::On;
                    }
                }
                st.cached_agents = CachedAgentStatus::load();
                changed = false;
            }
            SettingAction::ToggleCliPath => {
                if AgentSetup::has_cli() {
                    let dir = AgentSetup::cli_dir();
                    UserPath::set(&dir, !UserPath::contains(&dir));
                }
                st.cached_agents = CachedAgentStatus::load();
                changed = false;
            }
            SettingAction::OpenGoogleDataFolder | SettingAction::OpenDataFolder => {
                let dir = AppPaths::data_dir();
                let _ = std::fs::create_dir_all(&dir);
                NotchApp::open_local_path(&dir.to_string_lossy());
                changed = false;
            }
            SettingAction::OpenLogsFolder => {
                let dir = AppPaths::logs_dir();
                let _ = std::fs::create_dir_all(&dir);
                NotchApp::open_local_path(&dir.to_string_lossy());
                changed = false;
            }
            SettingAction::CopyDiagnostics => {
                let g_status = if st.cached_google.signed_in {
                    "connected"
                } else {
                    "disconnected"
                };
                let txt = diagnostics_summary(&app.store.data.settings, g_status);
                copy_text_to_clipboard(&txt);
                changed = false;
            }
        }
        if changed {
            app.store.save_now();
            let settings_copy = app.store.data.settings.clone();
            app.focus.apply_settings(&settings_copy);
            app.update_companion();
            app.update_targets(Some(&st.factories));
        }
    }
    if changed {
        (st.on_changed)();
    }
}

fn start_google_oauth(st: &mut SettingsState, wanted: Vec<&'static str>, enable_tasks: bool) {
    if st.google_cancel.is_some() {
        return;
    }
    let (lang, session_res) = {
        let app = st.app.borrow();
        let lang = app.lang;
        let res = app
            .google_auth
            .lock()
            .map_err(|_| "lock".to_string())
            .and_then(|mut g| g.begin_oauth(&wanted, lang));
        (lang, res)
    };
    match session_res {
        Ok(session) => {
            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_bg = Arc::clone(&cancel);
            let (tx, rx) = mpsc::channel();
            st.google_cancel = Some(cancel);
            st.google_rx = Some(rx);
            st.google_status = Some((lang.strings().google_continue_in_browser.to_string(), false));
            let hwnd_raw = st.hwnd.0 as usize;
            std::thread::spawn(move || {
                let res = session
                    .wait_and_exchange(cancel_bg, lang)
                    .map(|doc| (doc, wanted, enable_tasks));
                let _ = tx.send(res);
                // SAFETY: Notifying Settings window that OAuth finished.
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_raw as *mut _)),
                        WM_GOOGLE_DONE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });
        }
        Err(err) => {
            let dir = AppPaths::data_dir();
            st.google_status = Some((format!("{err} ({})", dir.display()), true));
        }
    }
}

fn post_settings_async(hwnd_raw: usize) {
    // SAFETY: Posting WM_SETTINGS_ASYNC to Settings window from a background worker thread.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd_raw as *mut _)),
            WM_SETTINGS_ASYNC,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn spawn_settings_update_check(st: &SettingsState) {
    {
        let mut app = st.app.borrow_mut();
        if app.update_checking {
            return;
        }
        app.update_checking = true;
        app.update_error = None;
    }
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let tag = fetch_latest_release_tag();
        let _ = tx.send(SettingsAsyncResult::UpdatesChecked(tag));
        post_settings_async(hwnd_raw);
    });
}

fn spawn_settings_update_install(st: &SettingsState) {
    {
        let mut app = st.app.borrow_mut();
        if app.update_installing {
            return;
        }
        app.update_installing = true;
        app.update_error = None;
    }
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let res = download_and_verify_update(env!("CARGO_PKG_VERSION"));
        let _ = tx.send(SettingsAsyncResult::UpdateInstallDone(res));
        post_settings_async(hwnd_raw);
    });
}

fn spawn_settings_weather_search(st: &SettingsState, query: String) {
    if query.trim().is_empty() {
        return;
    }
    let lang = {
        let mut app = st.app.borrow_mut();
        app.weather_searching = true;
        app.lang
    };
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let places = search_weather_places(&query, lang);
        let _ = tx.send(SettingsAsyncResult::WeatherSearched(places));
        post_settings_async(hwnd_raw);
    });
}

fn spawn_settings_weather_locate(st: &SettingsState) {
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let coords = locate_once();
        let _ = tx.send(SettingsAsyncResult::WeatherLocated(coords));
        post_settings_async(hwnd_raw);
    });
}

fn spawn_settings_weather_refresh(st: &SettingsState) {
    let (coords, lang) = {
        let app = st.app.borrow();
        (
            app.store
                .data
                .settings
                .weather_latitude
                .zip(app.store.data.settings.weather_longitude),
            app.lang,
        )
    };
    let Some((lat, lon)) = coords else {
        return;
    };
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let rep = fetch_weather(lat, lon, lang);
        let _ = tx.send(SettingsAsyncResult::WeatherFetched(rep));
        post_settings_async(hwnd_raw);
    });
}

fn spawn_settings_calendar_refresh(st: &SettingsState) {
    let (auth, ids) = {
        let mut app = st.app.borrow_mut();
        if app.calendar_fetching {
            return;
        }
        let can_fetch = app.google_auth.lock().is_ok_and(|g| {
            g.is_signed_in()
                && g.has_scopes(&[GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE])
        });
        if !can_fetch {
            return;
        }
        app.calendar_fetching = true;
        (
            Arc::clone(&app.google_auth),
            app.store.data.settings.calendar_ids.clone(),
        )
    };
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let res = fetch_calendars_and_events(&auth, ids.as_deref());
        let _ = tx.send(SettingsAsyncResult::CalendarSynced(res));
        post_settings_async(hwnd_raw);
    });
}

fn spawn_settings_tasks_refresh(st: &SettingsState) {
    let (auth, preferred_lid) = {
        let mut app = st.app.borrow_mut();
        if app.tasks_syncing || !app.store.data.settings.tasks_sync_enabled {
            return;
        }
        let can_fetch = app
            .google_auth
            .lock()
            .is_ok_and(|g| g.is_signed_in() && g.has_scopes(&[GoogleAuth::TASKS_SCOPE]));
        if !can_fetch {
            return;
        }
        app.tasks_syncing = true;
        (
            Arc::clone(&app.google_auth),
            app.store.data.settings.tasks_list_id.clone(),
        )
    };
    let tx = st.async_tx.clone();
    let hwnd_raw = st.hwnd.0 as usize;
    std::thread::spawn(move || {
        let res = match fetch_google_task_lists(&auth) {
            Ok(lists) => {
                let lid = preferred_lid.or_else(|| lists.first().map(|l| l.id.clone()));
                if let Some(lid) = lid {
                    fetch_google_tasks(&auth, &lid).map(|remote| (lists, lid, remote))
                } else {
                    Err("No task list".to_string())
                }
            }
            Err(e) => Err(e),
        };
        let _ = tx.send(SettingsAsyncResult::TasksSynced(res));
        post_settings_async(hwnd_raw);
    });
}

fn draw_settings(st: &mut SettingsState, w: f32, h: f32) {
    st.hits.clear();
    st.section_ys.clear();
    st.surface.fill_rect(0.0, 0.0, w, h, WINDOW_BG);

    let app_rc = Rc::clone(&st.app);
    let app = app_rc.borrow();
    let s = app.lang.strings();
    let set = &app.store.data.settings;
    let water = &app.store.data.water;

    // Sidebar (190px wide)
    st.surface.draw_text(
        &st.factories,
        "Mochi",
        34.0,
        22.0,
        140.0,
        28.0,
        FontKind::Display,
        20.0,
        true,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    let ver = fmt1(s.settings_version, env!("CARGO_PKG_VERSION"));
    st.surface.draw_text(
        &st.factories,
        &ver,
        22.0,
        54.0,
        156.0,
        32.0,
        FontKind::Ui,
        11.0,
        false,
        TEXT_HINT,
        HAlign::Left,
        VAlign::Top,
        false,
        true,
        false,
        false,
    );

    let nav_titles = [
        s.settings_general,
        s.settings_updates,
        s.settings_media,
        "Google",
        s.settings_weather,
        s.settings_notices,
        s.settings_activities,
        s.settings_focus,
        s.settings_water,
        "Mochi",
        s.settings_agents,
    ];
    for (i, title) in nav_titles.iter().enumerate() {
        let ny = 104.0 + (i as f32) * 34.0;
        let active = st.active_section == i;
        let hovered = st.hover_action.as_ref() == Some(&SettingAction::Nav(i));
        if active {
            st.surface
                .fill_rounded_rect(18.0, ny, 164.0, 30.0, 8.0, 8.0, SELECTED_FILL);
        } else if hovered {
            st.surface
                .fill_rounded_rect(18.0, ny, 164.0, 30.0, 8.0, 8.0, SUBTLE_FILL);
        }
        st.surface.draw_text(
            &st.factories,
            title,
            30.0,
            ny,
            140.0,
            30.0,
            FontKind::Ui,
            13.0,
            false,
            if active { TEXT_PRIMARY } else { TEXT_SECONDARY },
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
        st.hits.push((
            RectF {
                x: 18.0,
                y: ny,
                w: 164.0,
                h: 30.0,
            },
            SettingAction::Nav(i),
        ));
    }

    // Content area (x = 202 .. w - 28)
    let cx = 202.0;
    let cw = (w - cx - 28.0).max(200.0);
    st.surface.push_clip(cx, 0.0, cw, h);
    let mut y = 22.0 - st.scroll_y;

    if st.show_welcome {
        let card_y = y;
        let card_h = 260.0;
        st.surface
            .fill_rounded_rect(cx, card_y, cw, card_h, 12.0, 12.0, CARD_BG);
        st.surface.draw_text(
            &st.factories,
            s.welcome_title,
            cx + 18.0,
            card_y + 14.0,
            cw - 36.0,
            24.0,
            FontKind::Display,
            18.0,
            false,
            TEXT_PRIMARY,
            HAlign::Left,
            VAlign::Top,
            false,
            false,
            false,
            false,
        );
        for (ti, tip) in [
            s.welcome_tip1,
            s.welcome_tip2,
            s.welcome_tip3,
            s.welcome_tip4,
        ]
        .iter()
        .enumerate()
        {
            st.surface.draw_text(
                &st.factories,
                tip,
                cx + 18.0,
                card_y + 44.0 + (ti as f32) * 20.0,
                cw - 36.0,
                18.0,
                FontKind::Ui,
                13.0,
                false,
                TEXT_HINT,
                HAlign::Left,
                VAlign::Top,
                false,
                false,
                true,
                false,
            );
        }
        draw_switch_row(
            st,
            cx + 18.0,
            card_y + 130.0,
            cw - 36.0,
            s.welcome_water,
            Some(s.welcome_water_hint),
            water.reminders_enabled,
            SettingAction::ToggleWaterReminders,
        );
        st.surface
            .fill_rect(cx + 18.0, card_y + 180.0, cw - 36.0, 1.0, DIVIDER);
        draw_switch_row(
            st,
            cx + 18.0,
            card_y + 182.0,
            cw - 36.0,
            s.settings_update_check,
            Some(s.welcome_updates_hint),
            set.update_check_enabled,
            SettingAction::ToggleUpdateCheck,
        );
        draw_pill_row(
            st,
            cx + 18.0,
            card_y + 226.0,
            cw - 36.0,
            s.welcome_google,
            s.welcome_done,
            SettingAction::WelcomeDone,
        );
        y += card_h + 12.0;
    }

    // Helper closures for sections
    let begin_section = |title: &str, st_ref: &mut SettingsState, y_pos: &mut f32| {
        st_ref
            .section_ys
            .push((*y_pos + st_ref.scroll_y - 12.0).max(0.0));
        st_ref.surface.draw_text(
            &st_ref.factories,
            title,
            cx + 4.0,
            *y_pos + 14.0,
            cw - 8.0,
            18.0,
            FontKind::Ui,
            12.0,
            true,
            TEXT_HINT,
            HAlign::Left,
            VAlign::Top,
            false,
            false,
            false,
            false,
        );
        *y_pos += 38.0;
    };

    // 0. General
    begin_section(s.settings_general, st, &mut y);
    let lang_lbl = match set.language.as_deref() {
        Some("en") => "English".to_string(),
        Some("id") | Some("id-ID") => "Bahasa Indonesia".to_string(),
        _ => fmt1(
            s.settings_language_auto,
            if Language::detect_windows() == Language::Id {
                "Bahasa Indonesia"
            } else {
                "English"
            },
        ),
    };
    let mon_lbl = Monitors::resolve(set.monitor.as_deref(), app.lang).label;
    let gen_h = 10.0 * 52.0 + 12.0;
    st.surface
        .fill_rounded_rect(cx, y, cw, gen_h, 12.0, 12.0, CARD_BG);
    let mut ry = y + 6.0;
    draw_pill_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_language,
        &lang_lbl,
        SettingAction::CycleLanguage,
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_start_with_windows,
        None,
        Autostart::is_enabled(),
        SettingAction::ToggleAutostart,
    );
    ry += 52.0;
    draw_pill_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_display,
        &mon_lbl,
        SettingAction::CycleMonitor,
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_hotkey,
        Some(s.settings_hotkey_hint),
        set.hotkey_enabled,
        SettingAction::ToggleHotkey,
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_jump_hotkey,
        Some(s.settings_jump_hotkey_hint),
        set.jump_hotkey_enabled,
        SettingAction::ToggleJumpHotkey,
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_mic_hotkey,
        Some(s.settings_mic_hotkey_hint),
        set.mic_hotkey_enabled,
        SettingAction::ToggleMicHotkey,
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_hide_in_fullscreen,
        None,
        set.hide_in_fullscreen,
        SettingAction::ToggleHideFullscreen,
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_hide_from_capture,
        Some(s.settings_hide_from_capture_hint),
        set.hide_from_capture,
        SettingAction::ToggleHideCapture,
    );
    ry += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_hover_delay,
        &format!("{} ms", set.hover_delay_ms),
        SettingAction::StepHoverDelay(-20),
        SettingAction::StepHoverDelay(20),
    );
    ry += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        ry,
        cw - 36.0,
        s.settings_calm_motion,
        Some(s.settings_calm_motion_hint),
        set.calm_motion,
        SettingAction::ToggleCalmMotion,
    );
    y += gen_h + 8.0;

    // 1. Updates
    begin_section(s.settings_updates, st, &mut y);
    let upd_h = 168.0
        + if app.update_available.is_some() {
            52.0
        } else {
            0.0
        }
        + if app.update_error.is_some() {
            28.0
        } else {
            0.0
        };
    st.surface
        .fill_rounded_rect(cx, y, cw, upd_h, 12.0, 12.0, CARD_BG);
    draw_switch_row(
        st,
        cx + 18.0,
        y + 6.0,
        cw - 36.0,
        s.settings_update_check,
        Some(s.settings_update_check_hint),
        set.update_check_enabled,
        SettingAction::ToggleUpdateCheck,
    );
    draw_switch_row(
        st,
        cx + 18.0,
        y + 58.0,
        cw - 36.0,
        s.settings_auto_update,
        Some(s.settings_auto_update_hint),
        set.auto_update_enabled,
        SettingAction::ToggleAutoUpdate,
    );
    draw_pill_row(
        st,
        cx + 18.0,
        y + 110.0,
        cw - 36.0,
        &fmt1(s.settings_this_version, env!("CARGO_PKG_VERSION")),
        if app.update_checking {
            s.settings_checking
        } else {
            s.settings_check_now
        },
        SettingAction::CheckUpdatesNow,
    );
    let mut err_y = y + 162.0;
    if let Some(ref avail) = app.update_available {
        draw_pill_row(
            st,
            cx + 18.0,
            y + 162.0,
            cw - 36.0,
            &fmt1(s.settings_update_available, avail),
            if app.update_installing {
                s.settings_updating
            } else {
                s.settings_update_now
            },
            SettingAction::InstallUpdate,
        );
        err_y += 52.0;
    }
    if let Some(ref err) = app.update_error {
        st.surface.draw_text(
            &st.factories,
            err,
            cx + 18.0,
            err_y,
            cw - 36.0,
            24.0,
            FontKind::Ui,
            12.0,
            false,
            WARNING_BRUSH,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
    }
    y += upd_h + 8.0;

    // 2. Media
    begin_section(s.settings_media, st, &mut y);
    st.surface
        .fill_rounded_rect(cx, y, cw, 116.0, 12.0, 12.0, CARD_BG);
    draw_switch_row(
        st,
        cx + 18.0,
        y + 6.0,
        cw - 36.0,
        s.settings_lyrics,
        Some(s.settings_lyrics_hint),
        set.lyrics_enabled,
        SettingAction::ToggleLyrics,
    );
    draw_switch_row(
        st,
        cx + 18.0,
        y + 58.0,
        cw - 36.0,
        s.settings_accent,
        Some(s.settings_accent_hint),
        set.use_artwork_accent,
        SettingAction::ToggleAccent,
    );
    y += 124.0;

    // 3. Google
    begin_section("Google", st, &mut y);
    let configured = st.cached_google.configured;
    let signed_in = st.cached_google.signed_in;
    let has_cal = st.cached_google.has_cal;
    let email = st.cached_google.email.clone();
    let connecting = st.google_cancel.is_some();
    let (g_btn, g_act) = if connecting {
        (s.common_cancel, SettingAction::GoogleCancel)
    } else if signed_in {
        (
            s.google_disconnect,
            SettingAction::GoogleConnectOrDisconnect,
        )
    } else {
        (s.google_connect, SettingAction::GoogleConnectOrDisconnect)
    };
    let status_line: Option<(String, bool)> = if connecting {
        Some((s.google_continue_in_browser.to_string(), false))
    } else if let Some((ref msg, is_err)) = st.google_status {
        Some((msg.clone(), is_err))
    } else if !configured {
        Some((
            format!(
                "{} ({})",
                s.google_not_configured,
                AppPaths::data_dir().display()
            ),
            false,
        ))
    } else if signed_in && !email.is_empty() {
        Some((fmt1(s.google_connected_as, &email), false))
    } else {
        Some((s.google_intro.to_string(), false))
    };
    let extra_h = if status_line.is_some() { 52.0 } else { 0.0 };
    let g_h = 272.0 + extra_h;
    st.surface
        .fill_rounded_rect(cx, y, cw, g_h, 12.0, 12.0, CARD_BG);
    draw_pill_row(
        st,
        cx + 18.0,
        y + 6.0,
        cw - 36.0,
        s.google_account,
        g_btn,
        g_act,
    );
    let cal_btn = if has_cal { "✓" } else { s.settings_allow };
    draw_pill_row(
        st,
        cx + 18.0,
        y + 58.0,
        cw - 36.0,
        s.settings_calendar,
        cal_btn,
        SettingAction::CalendarAllow,
    );
    draw_stepper_row(
        st,
        cx + 18.0,
        y + 110.0,
        cw - 36.0,
        s.settings_remind_before,
        &format!("{} min", set.calendar_reminder_minutes),
        SettingAction::StepCalendarReminder(-1),
        SettingAction::StepCalendarReminder(1),
    );
    draw_switch_row(
        st,
        cx + 18.0,
        y + 162.0,
        cw - 36.0,
        s.settings_next_event_compact,
        Some(s.settings_next_event_compact_hint),
        set.calendar_in_compact,
        SettingAction::ToggleCalendarCompact,
    );
    draw_switch_row(
        st,
        cx + 18.0,
        y + 214.0,
        cw - 36.0,
        s.settings_tasks,
        None,
        set.tasks_sync_enabled,
        SettingAction::TasksAllowOrToggle,
    );
    if let Some((msg, is_err)) = status_line {
        let sy = y + 266.0;
        let btn_w = if !configured { 144.0 } else { 0.0 };
        st.surface.draw_text(
            &st.factories,
            &msg,
            cx + 18.0,
            sy,
            (cw - 36.0 - btn_w - 8.0).max(80.0),
            44.0,
            FontKind::Ui,
            12.0,
            false,
            if is_err { WARNING_BRUSH } else { TEXT_HINT },
            HAlign::Left,
            VAlign::Center,
            false,
            true,
            false,
            false,
        );
        if !configured {
            draw_pill_btn_at(
                st,
                cx + cw - 18.0 - 136.0,
                sy + 8.0,
                136.0,
                28.0,
                s.settings_open_data_folder,
                SettingAction::OpenGoogleDataFolder,
            );
        }
    }
    y += g_h + 8.0;

    // 4. Weather
    begin_section(s.settings_weather, st, &mut y);
    let place_lbl = set.weather_place.as_deref().unwrap_or("—");
    let res_h = (app.weather_places.len().min(4) as f32) * 40.0;
    let w_h = 220.0 + res_h;
    st.surface
        .fill_rounded_rect(cx, y, cw, w_h, 12.0, 12.0, CARD_BG);
    draw_switch_row(
        st,
        cx + 18.0,
        y + 6.0,
        cw - 36.0,
        s.settings_show_weather,
        Some(s.settings_show_weather_hint),
        set.weather_enabled,
        SettingAction::ToggleWeather,
    );
    draw_pill_row(
        st,
        cx + 18.0,
        y + 58.0,
        cw - 36.0,
        place_lbl,
        s.settings_use_my_location,
        SettingAction::LocateWeather,
    );
    // Search box
    let sbx = cx + 18.0;
    let sby = y + 114.0;
    let sbw = cw - 130.0;
    st.surface
        .fill_rounded_rect(sbx, sby, sbw, 32.0, 10.0, 10.0, SUBTLE_FILL);
    st.hits.push((
        RectF {
            x: sbx,
            y: sby,
            w: sbw,
            h: 32.0,
        },
        SettingAction::FocusWeatherSearch,
    ));
    let q_txt = if st.weather_query.is_empty() {
        s.settings_city_search
    } else {
        &st.weather_query
    };
    st.surface.draw_text(
        &st.factories,
        q_txt,
        sbx + 12.0,
        sby,
        sbw - 24.0,
        32.0,
        FontKind::Ui,
        13.0,
        false,
        if st.weather_query.is_empty() {
            TEXT_PLACEHOLDER
        } else {
            TEXT_PRIMARY
        },
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    draw_pill_btn_at(
        st,
        sbx + sbw + 8.0,
        sby + 2.0,
        86.0,
        28.0,
        if app.weather_searching {
            s.settings_searching
        } else {
            s.settings_search
        },
        SettingAction::SearchWeather,
    );
    for (pi, place) in app.weather_places.iter().take(4).enumerate() {
        let py = sby + 38.0 + (pi as f32) * 40.0;
        st.surface.draw_text(
            &st.factories,
            &format!("{} — {}", place.name, place.detail),
            sbx + 8.0,
            py,
            cw - 52.0,
            36.0,
            FontKind::Ui,
            12.0,
            false,
            TEXT_PRIMARY,
            HAlign::Left,
            VAlign::Center,
            false,
            false,
            true,
            false,
        );
        st.hits.push((
            RectF {
                x: sbx,
                y: py,
                w: cw - 36.0,
                h: 36.0,
            },
            SettingAction::PickWeatherPlace(pi),
        ));
    }
    draw_switch_row(
        st,
        cx + 18.0,
        y + 162.0 + res_h,
        cw - 36.0,
        s.settings_rain_alerts,
        Some(s.settings_rain_alerts_hint),
        set.weather_rain_alerts,
        SettingAction::ToggleRainAlerts,
    );
    y += w_h + 8.0;

    // 5. Notices
    begin_section(s.settings_notices, st, &mut y);
    let not_h = 10.0 * 52.0 + 12.0;
    st.surface
        .fill_rounded_rect(cx, y, cw, not_h, 12.0, 12.0, CARD_BG);
    let mut ny = y + 6.0;
    for (lbl, val, act) in [
        (
            s.settings_sounds,
            set.sounds_enabled,
            SettingAction::ToggleSounds,
        ),
        (
            s.settings_quiet_during_calls,
            set.quiet_during_calls,
            SettingAction::ToggleQuietCalls,
        ),
        (
            s.settings_hud_volume,
            set.hud_volume,
            SettingAction::ToggleHudVolume,
        ),
        (
            s.settings_hud_brightness,
            set.hud_brightness,
            SettingAction::ToggleHudBrightness,
        ),
        (
            s.settings_hud_power,
            set.hud_power,
            SettingAction::ToggleHudPower,
        ),
        (
            s.settings_hud_audio_device,
            set.hud_audio_device,
            SettingAction::ToggleHudAudioDevice,
        ),
        (
            s.settings_hud_caps_lock,
            set.hud_caps_lock,
            SettingAction::ToggleHudCapsLock,
        ),
        (
            s.settings_hud_keyboard_layout,
            set.hud_keyboard_layout,
            SettingAction::ToggleHudKeyboard,
        ),
        (
            s.settings_hud_network,
            set.hud_network,
            SettingAction::ToggleHudNetwork,
        ),
        (
            s.settings_hud_privacy,
            set.hud_privacy,
            SettingAction::ToggleHudPrivacy,
        ),
    ] {
        draw_switch_row(st, cx + 18.0, ny, cw - 36.0, lbl, None, val, act);
        ny += 52.0;
    }
    y += not_h + 8.0;

    // 6. Activities
    begin_section(s.settings_activities, st, &mut y);
    st.surface
        .fill_rounded_rect(cx, y, cw, 64.0, 12.0, 12.0, CARD_BG);
    draw_switch_row(
        st,
        cx + 18.0,
        y + 6.0,
        cw - 36.0,
        s.settings_activities_enabled,
        Some(s.settings_activities_hint),
        set.activities_enabled,
        SettingAction::ToggleActivities,
    );
    y += 72.0;

    // 7. Focus
    begin_section(s.settings_focus, st, &mut y);
    let foc_h = 6.0 * 52.0 + 12.0;
    st.surface
        .fill_rounded_rect(cx, y, cw, foc_h, 12.0, 12.0, CARD_BG);
    let mut fy = y + 6.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        fy,
        cw - 36.0,
        s.settings_focus_length,
        &format!("{} min", set.focus_minutes),
        SettingAction::StepFocusMinutes(-5),
        SettingAction::StepFocusMinutes(5),
    );
    fy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        fy,
        cw - 36.0,
        s.settings_short_break,
        &format!("{} min", set.short_break_minutes),
        SettingAction::StepShortBreak(-1),
        SettingAction::StepShortBreak(1),
    );
    fy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        fy,
        cw - 36.0,
        s.settings_long_break,
        &format!("{} min", set.long_break_minutes),
        SettingAction::StepLongBreak(-5),
        SettingAction::StepLongBreak(5),
    );
    fy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        fy,
        cw - 36.0,
        s.settings_sessions_per_cycle,
        &set.sessions_per_cycle.to_string(),
        SettingAction::StepSessionsPerCycle(-1),
        SettingAction::StepSessionsPerCycle(1),
    );
    fy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        fy,
        cw - 36.0,
        s.settings_focus_goal,
        &format!("{} min", set.focus_goal_minutes),
        SettingAction::StepFocusGoal(-15),
        SettingAction::StepFocusGoal(15),
    );
    fy += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        fy,
        cw - 36.0,
        s.settings_pause_when_away,
        Some(s.settings_pause_when_away_hint),
        set.pause_focus_when_away,
        SettingAction::TogglePauseAway,
    );
    y += foc_h + 8.0;

    // 8. Water
    begin_section(s.settings_water, st, &mut y);
    let wat_h = 4.0 * 52.0 + 12.0;
    st.surface
        .fill_rounded_rect(cx, y, cw, wat_h, 12.0, 12.0, CARD_BG);
    let mut wy = y + 6.0;
    draw_switch_row(
        st,
        cx + 18.0,
        wy,
        cw - 36.0,
        s.settings_water_reminders,
        None,
        water.reminders_enabled,
        SettingAction::ToggleWaterReminders,
    );
    wy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        wy,
        cw - 36.0,
        s.settings_water_goal,
        &format!("{} ml", water.goal_ml),
        SettingAction::StepWaterGoal(-250),
        SettingAction::StepWaterGoal(250),
    );
    wy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        wy,
        cw - 36.0,
        s.settings_cup_size,
        &format!("{} ml", water.cup_ml),
        SettingAction::StepWaterCup(-50),
        SettingAction::StepWaterCup(50),
    );
    wy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        wy,
        cw - 36.0,
        s.settings_water_every,
        &format!("{} min", water.reminder_minutes),
        SettingAction::StepWaterReminder(-15),
        SettingAction::StepWaterReminder(15),
    );
    y += wat_h + 8.0;

    // 9. Mochi companion
    begin_section("Mochi", st, &mut y);
    let comp_h = 4.0 * 52.0 + 12.0;
    st.surface
        .fill_rounded_rect(cx, y, cw, comp_h, 12.0, 12.0, CARD_BG);
    let mut cy = y + 6.0;
    draw_switch_row(
        st,
        cx + 18.0,
        cy,
        cw - 36.0,
        s.settings_companion,
        Some(s.settings_companion_hint),
        set.companion_enabled,
        SettingAction::ToggleCompanion,
    );
    cy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        cy,
        cw - 36.0,
        s.settings_companion_after,
        &format!("{} min", set.companion_idle_minutes),
        SettingAction::StepCompanionIdle(-1),
        SettingAction::StepCompanionIdle(1),
    );
    cy += 52.0;
    draw_stepper_row(
        st,
        cx + 18.0,
        cy,
        cw - 36.0,
        s.settings_companion_sleep,
        &format!("{} min", set.companion_sleep_minutes),
        SettingAction::StepCompanionSleep(-2),
        SettingAction::StepCompanionSleep(2),
    );
    cy += 52.0;
    draw_switch_row(
        st,
        cx + 18.0,
        cy,
        cw - 36.0,
        s.settings_away_summary,
        None,
        set.away_summary_enabled,
        SettingAction::ToggleAwaySummary,
    );
    y += comp_h + 8.0;

    // 10. AI agents
    begin_section(s.settings_agents, st, &mut y);
    let on_path = st.cached_agents.on_path;
    let ag_h = 5.0 * 52.0 + 12.0;
    st.surface
        .fill_rounded_rect(cx, y, cw, ag_h, 12.0, 12.0, CARD_BG);
    let mut ay = y + 6.0;
    for (label, id, status) in [
        ("VS Code (Copilot)", "copilot", st.cached_agents.copilot),
        ("Claude Code", "claude", st.cached_agents.claude),
        ("Cursor", "cursor", st.cached_agents.cursor),
        ("Codex", "codex", st.cached_agents.codex),
    ] {
        let on = matches!(status, AgentStatus::On | AgentStatus::OnByPlugin);
        let hint = match status {
            AgentStatus::NotFound => Some(s.agents_not_found),
            AgentStatus::OnByPlugin => Some(s.agents_by_plugin),
            _ => None,
        };
        draw_switch_row(
            st,
            cx + 18.0,
            ay,
            cw - 36.0,
            label,
            hint,
            on,
            SettingAction::ToggleAgent(id),
        );
        ay += 52.0;
    }
    draw_switch_row(
        st,
        cx + 18.0,
        ay,
        cw - 36.0,
        s.agents_path,
        Some(s.agents_path_hint),
        on_path,
        SettingAction::ToggleCliPath,
    );
    y += ag_h + 18.0;

    // Footer: Data folder, Log folder, Copy diagnostics
    draw_pill_row(
        st,
        cx + 4.0,
        y,
        cw - 4.0,
        s.settings_data_local,
        s.settings_open_data_folder,
        SettingAction::OpenDataFolder,
    );
    y += 48.0;
    st.surface.draw_text(
        &st.factories,
        s.settings_log_local,
        cx + 4.0,
        y,
        (cw - 260.0).max(80.0),
        36.0,
        FontKind::Ui,
        12.0,
        false,
        TEXT_HINT,
        HAlign::Left,
        VAlign::Center,
        false,
        true,
        false,
        false,
    );
    let diag_w = (st
        .factories
        .measure_text(
            s.settings_copy_diagnostics,
            FontKind::Ui,
            12.0,
            false,
            false,
            180.0,
            false,
        )
        .0
        + 24.0)
        .clamp(80.0, 180.0);
    let log_w = (st
        .factories
        .measure_text(
            s.settings_open_log,
            FontKind::Ui,
            12.0,
            false,
            false,
            140.0,
            false,
        )
        .0
        + 24.0)
        .clamp(70.0, 140.0);
    draw_pill_btn_at(
        st,
        cx + cw - diag_w,
        y + 4.0,
        diag_w,
        28.0,
        s.settings_copy_diagnostics,
        SettingAction::CopyDiagnostics,
    );
    draw_pill_btn_at(
        st,
        cx + cw - diag_w - 8.0 - log_w,
        y + 4.0,
        log_w,
        28.0,
        s.settings_open_log,
        SettingAction::OpenLogsFolder,
    );
    y += 56.0;

    st.max_scroll = (y + st.scroll_y - h).max(0.0);
    st.hits.retain_mut(|(r, _)| {
        let top = r.y.max(0.0);
        let bot = (r.y + r.h).min(h);
        if bot <= top {
            false
        } else {
            r.y = top;
            r.h = bot - top;
            true
        }
    });
    st.surface.pop_clip();
}

#[expect(clippy::too_many_arguments, reason = "renders settings switch row")]
fn draw_switch_row(
    st: &mut SettingsState,
    x: f32,
    y: f32,
    w: f32,
    label: &str,
    hint: Option<&str>,
    on: bool,
    act: SettingAction,
) {
    let lw = (w - 60.0).max(40.0);
    if let Some(h) = hint {
        st.surface.draw_text(
            &st.factories,
            label,
            x,
            y + 6.0,
            lw,
            20.0,
            FontKind::Ui,
            14.0,
            false,
            TEXT_PRIMARY,
            HAlign::Left,
            VAlign::Top,
            false,
            false,
            true,
            false,
        );
        st.surface.draw_text(
            &st.factories,
            h,
            x,
            y + 26.0,
            lw,
            20.0,
            FontKind::Ui,
            12.0,
            false,
            TEXT_HINT,
            HAlign::Left,
            VAlign::Top,
            false,
            false,
            true,
            false,
        );
    } else {
        st.surface.draw_text(
            &st.factories,
            label,
            x,
            y,
            lw,
            50.0,
            FontKind::Ui,
            14.0,
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
    let sx = x + w - 42.0;
    let sy = y + 14.0;
    let bg = if on { INVERSE_FILL } else { TRACK };
    st.surface
        .fill_rounded_rect(sx, sy, 42.0, 22.0, 11.0, 11.0, bg);
    let thumb_x = if on { sx + 23.0 } else { sx + 3.0 };
    let thumb_col = if on {
        INVERSE_TEXT
    } else {
        Color::argb(0xB3, 0xFF, 0xFF, 0xFF)
    };
    st.surface
        .fill_ellipse(thumb_x + 8.0, sy + 11.0, 8.0, 8.0, thumb_col);
    st.hits.push((
        RectF {
            x: sx,
            y: sy,
            w: 42.0,
            h: 22.0,
        },
        act,
    ));
}

fn draw_pill_row(
    st: &mut SettingsState,
    x: f32,
    y: f32,
    w: f32,
    label: &str,
    btn_label: &str,
    act: SettingAction,
) {
    let bw = (st
        .factories
        .measure_text(btn_label, FontKind::Ui, 12.0, false, false, 200.0, false)
        .0
        + 24.0)
        .clamp(64.0, 200.0);
    st.surface.draw_text(
        &st.factories,
        label,
        x,
        y,
        (w - bw - 12.0).max(40.0),
        50.0,
        FontKind::Ui,
        14.0,
        false,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    draw_pill_btn_at(st, x + w - bw, y + 11.0, bw, 28.0, btn_label, act);
}

fn draw_pill_btn_at(
    st: &mut SettingsState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    act: SettingAction,
) {
    let hovered = st.hover_action.as_ref() == Some(&act);
    st.surface.fill_rounded_rect(
        x,
        y,
        w,
        h,
        h / 2.0,
        h / 2.0,
        if hovered { HOVER_FILL } else { PILL_FILL },
    );
    st.surface.draw_text(
        &st.factories,
        label,
        x,
        y,
        w,
        h,
        FontKind::Ui,
        12.0,
        false,
        PILL_FOREGROUND,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    st.hits.push((RectF { x, y, w, h }, act));
}

#[expect(clippy::too_many_arguments, reason = "renders stepper row")]
fn draw_stepper_row(
    st: &mut SettingsState,
    x: f32,
    y: f32,
    w: f32,
    label: &str,
    val_str: &str,
    dec: SettingAction,
    inc: SettingAction,
) {
    st.surface.draw_text(
        &st.factories,
        label,
        x,
        y,
        (w - 140.0).max(40.0),
        50.0,
        FontKind::Ui,
        14.0,
        false,
        TEXT_PRIMARY,
        HAlign::Left,
        VAlign::Center,
        false,
        false,
        true,
        false,
    );
    let rx = x + w - 124.0;
    let ry = y + 11.0;
    st.surface
        .fill_rounded_rect(rx, ry, 28.0, 28.0, 14.0, 14.0, PILL_FILL);
    st.surface.draw_text(
        &st.factories,
        "−",
        rx,
        ry,
        28.0,
        28.0,
        FontKind::Ui,
        14.0,
        false,
        TEXT_PRIMARY,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    st.hits.push((
        RectF {
            x: rx,
            y: ry,
            w: 28.0,
            h: 28.0,
        },
        dec,
    ));

    st.surface.draw_text(
        &st.factories,
        val_str,
        rx + 28.0,
        ry,
        68.0,
        28.0,
        FontKind::Ui,
        13.0,
        false,
        TEXT_PRIMARY,
        HAlign::Center,
        VAlign::Center,
        true,
        false,
        false,
        false,
    );

    st.surface
        .fill_rounded_rect(rx + 96.0, ry, 28.0, 28.0, 14.0, 14.0, PILL_FILL);
    st.surface.draw_text(
        &st.factories,
        "+",
        rx + 96.0,
        ry,
        28.0,
        28.0,
        FontKind::Ui,
        14.0,
        false,
        TEXT_PRIMARY,
        HAlign::Center,
        VAlign::Center,
        false,
        false,
        false,
        false,
    );
    st.hits.push((
        RectF {
            x: rx + 96.0,
            y: ry,
            w: 28.0,
            h: 28.0,
        },
        inc,
    ));
}
