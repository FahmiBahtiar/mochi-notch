use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use serde::Deserialize;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, POINTL, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl,
    OleInitialize, OleUninitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, SetFocus, TME_LEAVE,
    TRACKMOUSEEVENT, TrackMouseEvent, UnregisterHotKey, VK_LBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, HDROP, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, FindWindowW, GWL_EXSTYLE,
    GWLP_USERDATA, GetForegroundWindow, GetMessageW, GetWindowLongPtrW, HICON, HTCLIENT,
    HTTRANSPARENT, IDC_ARROW, IsWindowVisible, KillTimer, LR_DEFAULTCOLOR, LoadCursorW,
    MA_NOACTIVATE, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, MSG, PM_REMOVE, PeekMessageW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOREDRAW, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_TOPALIGN, TrackPopupMenu, TranslateMessage, WA_INACTIVE, WM_ACTIVATE, WM_APP, WM_CHAR,
    WM_CLIPBOARDUPDATE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DISPLAYCHANGE, WM_DROPFILES,
    WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCCREATE, WM_NCHITTEST, WM_RBUTTONUP, WM_TIMER,
    WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows_core::{PCWSTR, Ref, implement};

use mochi_winrt::agent_setup::{AgentSetup, AgentStatus};
use mochi_winrt::audio::{AudioEvent, AudioService};
use mochi_winrt::calendar::{CalendarEvent, CalendarInfo, fetch_calendars_and_events};
use mochi_winrt::clipboard::{ClipboardPayload, read_clipboard_payload};
use mochi_winrt::google::GoogleAuth;
use mochi_winrt::location::locate_once;
use mochi_winrt::lyrics::Lyrics;
use mochi_winrt::media::MediaService;
use mochi_winrt::monitors::{DisplayInfo, Monitors};
use mochi_winrt::native::{
    Autostart, VsCodeChatAction, focus_host_window, get_cursor_pos, keep_topmost,
    send_terminal_interrupt, set_capture_exclusion, to_wide, trigger_vscode_chat_action,
    trim_working_set,
};
use mochi_winrt::notifications::{NotificationEvent, start_notification_watcher};
use mochi_winrt::paths::{Glyphs, HudKind, HudMessage};
use mochi_winrt::pipe::ActivityPipe;
use mochi_winrt::tasks::{RemoteTask, TaskListInfo, fetch_google_task_lists, fetch_google_tasks};
use mochi_winrt::updates::fetch_latest_release_tag;
use mochi_winrt::watchers::{
    create_single_instance_mutex, detect_fullscreen, start_brightness_watcher,
    start_camera_watcher, start_network_watcher,
};
use mochi_winrt::weather::{HourlyWeatherPoint, WeatherReport, fetch_weather};

use crate::d2d::{D2dFactories, D2dSurface};
use crate::render::{HitMap, render_notch};
use crate::settings_win::open_settings_window;
use crate::state::{AwayItem, AwayReport, EXPANDED_WIDE, HoverTarget, NotchApp, NotchPage};
use crate::theme::MOCHI_ICO;

const WM_TRAY: u32 = WM_APP + 1;
const WM_WAKE: u32 = WM_APP + 2;
const WM_SYNC_CLICK_THROUGH: u32 = WM_APP + 3;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_MAIN: usize = 1;
const TIMER_HOVER_WATCH: usize = 2;
const HOTKEY_TOGGLE: i32 = 0x5961;
const HOTKEY_JUMP: i32 = 0x594A;
const HOTKEY_MIC: i32 = 0x594D;

const CMD_OPEN: usize = 1001;
const CMD_HIDE: usize = 1002;
const CMD_SETTINGS: usize = 1003;
const CMD_UPDATES: usize = 1004;
const CMD_DEMO: usize = 1005;
const CMD_QUIT: usize = 1006;

pub struct LaunchOptions {
    pub bench: Option<String>,
    pub bench_variant: Option<String>,
    pub shots_dir: Option<PathBuf>,
    pub frames_path: Option<PathBuf>,
    pub show_page: Option<(String, u64)>,
    pub open_settings: bool,
    pub demo: bool,
    pub ambient: bool,
    pub autostart: bool,
}

enum BgEvent {
    MediaChanged,
    Audio(AudioEvent),
    Camera(Vec<String>),
    Hud(HudMessage),
    Network(HudMessage, bool),
    LyricsLoaded(String, Option<Lyrics>),
    ActivityLine(String, mpsc::Sender<String>),
    CalendarLoaded(Vec<CalendarInfo>, Vec<CalendarEvent>),
    GoogleAuthDone(Result<serde_json::Value, String>),
    TasksLoaded(Vec<TaskListInfo>, String, Vec<RemoteTask>),
    WeatherLoaded(Option<WeatherReport>),
    UpdateTag(Option<String>),
    SystemNotification(NotificationEvent),
}

struct WindowHost {
    hwnd: HWND,
    app: Rc<RefCell<NotchApp>>,
    factories: Rc<D2dFactories>,
    surface: D2dSurface,
    hits: HitMap,
    bg_rx: mpsc::Receiver<BgEvent>,
    bg_tx: mpsc::Sender<BgEvent>,
    tray_icon: HICON,
    tray_added: bool,
    hotkey_reg: bool,
    jump_reg: bool,
    mic_reg: bool,
    drop_registered: bool,
    clipboard_listening: bool,
    excluded_capture: Option<bool>,
    tracking_mouse: bool,
    click_through: bool,
    sync_pending: bool,
    hover_watch_ms: u32,
    rendering: bool,
    was_visible: bool,
    cached_disp: DisplayInfo,
    cached_scale: f64,
    last_scale: f64,
    last_pos: (i32, i32, i32, i32),
    last_lyrics_req: Option<String>,
    bench_done_at: Option<Instant>,
    startup_trimmed: bool,
    was_animating: bool,
    last_trim_at: Instant,
    _pipe: Option<ActivityPipe>,
}

pub fn run_desktop(opts: LaunchOptions) -> i32 {
    // SAFETY: Setting per-monitor DPI awareness v2 and initializing OLE STA before creating any window.
    let ole_ok = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        OleInitialize(None).is_ok()
    };

    let _mutex = if opts.shots_dir.is_none() && opts.bench.is_none() {
        match create_single_instance_mutex() {
            Some(m) => Some(m),
            None => return 0,
        }
    } else {
        None
    };

    let Ok(factories) = D2dFactories::new() else {
        return 1;
    };
    let factories = Rc::new(factories);

    let mut app = NotchApp::new(
        opts.bench.clone(),
        opts.bench_variant.clone(),
        opts.shots_dir.clone(),
        opts.frames_path.clone(),
    );

    if opts.bench.as_deref() == Some("vitrine")
        && let Some(ref dir) = opts.shots_dir
    {
        run_vitrine_shots(&mut app, &factories, dir);
        return 0;
    }

    let disp = Monitors::resolve(app.store.data.settings.monitor.as_deref(), app.lang);
    let scale = Monitors::scale_at(disp.x + disp.width / 2, disp.y);
    let pw = (app.window_w * scale).round() as i32;
    let ph = (app.window_h * scale).round() as i32;
    let Ok(surface) = D2dSurface::new(&factories, pw, ph, scale) else {
        return 1;
    };

    let (bg_tx, bg_rx) = mpsc::channel::<BgEvent>();
    let app_rc = Rc::new(RefCell::new(app));

    let class_name = to_wide("MochiNotchWindow");
    // SAFETY: GetModuleHandleW(None) returns current module handle.
    let hinst = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(notch_wnd_proc),
        hInstance: hinst.into(),
        // SAFETY: Loading standard arrow cursor.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    // SAFETY: Registering notch window class.
    unsafe {
        RegisterClassExW(&wc);
    }

    let title = to_wide("Mochi");
    let ex_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    let x = disp.x + (disp.width - pw) / 2;
    let y = disp.y;

    // SAFETY: Creating layered topmost tool window.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            x,
            y,
            pw,
            ph,
            None,
            None,
            Some(hinst.into()),
            None,
        )
    };
    let Ok(hwnd) = hwnd else {
        return 1;
    };

    let tray_icon = load_ico_from_bytes(MOCHI_ICO, 32);
    let mut host = Box::new(WindowHost {
        hwnd,
        app: Rc::clone(&app_rc),
        factories: Rc::clone(&factories),
        surface,
        hits: HitMap::default(),
        bg_rx,
        bg_tx: bg_tx.clone(),
        tray_icon,
        tray_added: false,
        hotkey_reg: false,
        jump_reg: false,
        mic_reg: false,
        drop_registered: false,
        clipboard_listening: false,
        excluded_capture: None,
        tracking_mouse: false,
        click_through: false,
        sync_pending: false,
        hover_watch_ms: 0,
        rendering: false,
        was_visible: true,
        cached_disp: disp,
        cached_scale: scale,
        last_scale: scale,
        last_pos: (x, y, pw, ph),
        last_lyrics_req: None,
        bench_done_at: None,
        startup_trimmed: false,
        was_animating: false,
        last_trim_at: Instant::now(),
        _pipe: None,
    });

    // Start background services unless running isolated bench
    if opts.bench.is_none() {
        start_services(&mut host);
    } else {
        host.app.borrow_mut().start_bench(Some(&host.factories));
    }

    if let Some((ref page, secs)) = opts.show_page {
        host.app
            .borrow_mut()
            .show_page(page, secs, Some(&host.factories));
    }
    if opts.autostart {
        Autostart::upgrade();
    }
    if opts.demo {
        let mut a = host.app.borrow_mut();
        a.set_expanded(false, Some(&host.factories));
        a.simulated_idle_secs = Some(180.0);
        a.presence_state = 1;
        a.update_companion();
        a.update_targets(Some(&host.factories));
    }
    if opts.ambient {
        let mut a = host.app.borrow_mut();
        a.force_ambient = true;
        a.ambient_shown = true;
        a.presence_state = 1;
        a.update_companion();
        a.update_targets(Some(&host.factories));
    }
    if opts.open_settings
        || (host.app.borrow().store.is_new
            && opts.bench.is_none()
            && opts.show_page.is_none()
            && !opts.demo
            && !opts.ambient)
    {
        let is_new = host.app.borrow().store.is_new;
        let app_c = Rc::clone(&host.app);
        let fac_c = Rc::clone(&host.factories);
        let hwnd_raw = hwnd.0 as usize;
        open_settings_window(
            app_c,
            fac_c,
            is_new,
            Rc::new(move || {
                // SAFETY: Posting wake message to notch window.
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_raw as *mut _)),
                        WM_WAKE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }),
        );
    }

    update_hotkeys(&mut host);
    apply_capture_exclusion(&mut host);
    if opts.bench.as_deref() != Some("transicoes") {
        add_tray_icon(&mut host);
    }

    let host_ptr = Box::into_raw(host);
    // SAFETY: Storing `WindowHost` pointer in GWLP_USERDATA, registering OLE drop target, and starting 250ms timer.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, host_ptr as isize);
        if ole_ok {
            let drop_target: IDropTarget = MochiDropTarget { hwnd }.into();
            if RegisterDragDrop(hwnd, &drop_target).is_ok() {
                (*host_ptr).drop_registered = true;
            }
        }
        if AddClipboardFormatListener(hwnd).is_ok() {
            (*host_ptr).clipboard_listening = true;
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetTimer(Some(hwnd), TIMER_MAIN, 250, None);
    }
    if opts.bench.is_none()
        && opts.shots_dir.is_none()
        && let Some((seq, payload)) = read_clipboard_payload()
    {
        // SAFETY: `host_ptr` is valid on the UI thread.
        let host_ref = unsafe { &mut *host_ptr };
        if let Ok(mut app) = host_ref.app.try_borrow_mut() {
            app.capture_clipboard_payload(seq, payload, Some(&host_ref.factories));
        }
    }
    // SAFETY: Updating hotkeys/fallback drop acceptance after `drop_registered` is known.
    unsafe {
        update_hotkeys(&mut *host_ptr);
    }

    // Initial render
    // SAFETY: `host_ptr` is valid on this UI thread.
    unsafe {
        render_and_present(&mut *host_ptr);
        sync_click_through(&mut *host_ptr);
    }

    // Main message + 100Hz DwmFlush vsync loop
    let mut msg = MSG::default();
    let mut last_frame = Instant::now();
    loop {
        // SAFETY: `host_ptr` remains valid until WM_QUIT after WM_DESTROY.
        let continuous = unsafe { (*host_ptr).app.borrow().needs_continuous_frames() };

        if continuous {
            // SAFETY: Draining all queued Win32 messages without blocking.
            while unsafe { PeekMessageW(&raw mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                if msg.message == windows::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    // SAFETY: Cleaning up host pointer and OLE on exit.
                    unsafe {
                        drop(Box::from_raw(host_ptr));
                        if ole_ok {
                            OleUninitialize();
                        }
                    }
                    return msg.wParam.0 as i32;
                }
                // SAFETY: Standard Win32 message dispatch.
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            // Lock cleanly to DWM vblank (10.0 ms on 100 Hz monitor)
            // SAFETY: DwmFlush waits for next DWM composition vblank.
            unsafe {
                let _ = DwmFlush();
            }
            let now = Instant::now();
            let dt = now.duration_since(last_frame).as_secs_f64();
            last_frame = now;

            // SAFETY: No Win32 dispatch is active here; short-lived exclusive access to `host_ptr`.
            let host_ref = unsafe { &mut *host_ptr };
            drain_bg_events(host_ref);
            sync_drag_approach(host_ref);
            if let Ok(mut app) = host_ref.app.try_borrow_mut() {
                app.poll_periodic(Some(&host_ref.factories));
            }
            let work_start = Instant::now();
            let (should_render, animating_now, expanded_now) =
                if let Ok(mut app) = host_ref.app.try_borrow_mut() {
                    app.step_animations(dt, Some(&host_ref.factories));
                    (
                        app.dirty || app.frame_timings.recording,
                        app.animating,
                        app.expanded,
                    )
                } else {
                    (false, false, false)
                };
            if should_render {
                render_and_present(host_ref);
            }
            if host_ref.was_animating
                && !animating_now
                && !expanded_now
                && host_ref.last_trim_at.elapsed() >= Duration::from_secs(10)
            {
                host_ref.last_trim_at = Instant::now();
                trim_working_set();
            }
            host_ref.was_animating = animating_now;
            let work_ms = work_start.elapsed().as_secs_f64() * 1000.0;
            let done = if let Ok(mut app) = host_ref.app.try_borrow_mut() {
                app.frame_timings.record_frame(work_ms);
                app.bench_done
            } else {
                false
            };
            if done {
                let exit_now = match host_ref.bench_done_at {
                    Some(t) => t.elapsed() >= Duration::from_secs(2),
                    None => {
                        host_ref.bench_done_at = Some(Instant::now());
                        false
                    }
                };
                if exit_now {
                    let hwnd_close = host_ref.hwnd;
                    // SAFETY: Closing window after bench completion grace period.
                    unsafe {
                        let _ = DestroyWindow(hwnd_close);
                    }
                }
            }
        } else {
            // SAFETY: Blocking wait when idle (0% CPU).
            let ret = unsafe { GetMessageW(&raw mut msg, None, 0, 0) };
            if ret.0 <= 0 {
                // SAFETY: Cleaning up host pointer and OLE on exit.
                unsafe {
                    drop(Box::from_raw(host_ptr));
                    if ole_ok {
                        OleUninitialize();
                    }
                }
                return msg.wParam.0 as i32;
            }
            // SAFETY: Standard Win32 message dispatch.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            last_frame = Instant::now();
            // SAFETY: No Win32 dispatch is active here; short-lived exclusive access to `host_ptr`.
            let host_ref = unsafe { &mut *host_ptr };
            drain_bg_events(host_ref);
            let dirty = host_ref.app.try_borrow().is_ok_and(|a| a.dirty);
            if dirty {
                render_and_present(host_ref);
            }
        }
    }
}

fn start_services(host: &mut WindowHost) {
    let hwnd_raw = host.hwnd.0 as usize;
    let wake = move || {
        // SAFETY: Posting wake message to UI thread.
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd_raw as *mut _)),
                WM_WAKE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    };

    let tx_m = host.bg_tx.clone();
    let media = MediaService::start(Arc::new(move || {
        let _ = tx_m.send(BgEvent::MediaChanged);
        wake();
    }));

    let lang = host.app.borrow().lang;
    let tx_a = host.bg_tx.clone();
    let audio = AudioService::start(
        lang,
        Arc::new(move |ev| {
            let _ = tx_a.send(BgEvent::Audio(ev));
            wake();
        }),
    );

    let stop = Arc::clone(&host.app.borrow().stop_flag);
    let tx_b = host.bg_tx.clone();
    start_brightness_watcher(
        Arc::clone(&stop),
        lang,
        Arc::new(move |m| {
            let _ = tx_b.send(BgEvent::Hud(m));
            wake();
        }),
    );

    let tx_n = host.bg_tx.clone();
    start_network_watcher(
        Arc::clone(&stop),
        lang,
        Arc::new(move |m, came_back| {
            let _ = tx_n.send(BgEvent::Network(m, came_back));
            wake();
        }),
    );

    let tx_c = host.bg_tx.clone();
    start_camera_watcher(
        Arc::clone(&stop),
        Arc::new(move |apps| {
            let _ = tx_c.send(BgEvent::Camera(apps));
            wake();
        }),
    );

    let tx_notif = host.bg_tx.clone();
    start_notification_watcher(
        Arc::clone(&stop),
        Arc::new(move |ev| {
            let _ = tx_notif.send(BgEvent::SystemNotification(ev));
            wake();
        }),
    );

    let tx_p = host.bg_tx.clone();
    let pipe = ActivityPipe::start(Arc::new(move |line| {
        let (rtx, rrx) = mpsc::channel();
        if tx_p.send(BgEvent::ActivityLine(line, rtx)).is_err() {
            return "error: closing".to_string();
        }
        wake();
        rrx.recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|_| "error: timeout".to_string())
    }));

    // Background online sync (Calendar, Tasks, Weather, Updates) without blocking the 100Hz UI thread
    let (auth_arc, cal_ids, tasks_on, tasks_list_id, weather_cfg, upd_enabled) = {
        let a = host.app.borrow();
        let s = &a.store.data.settings;
        (
            Arc::clone(&a.google_auth),
            s.calendar_ids.clone(),
            s.tasks_sync_enabled,
            s.tasks_list_id.clone(),
            if s.weather_enabled {
                s.weather_latitude.zip(s.weather_longitude)
            } else {
                None
            },
            s.update_check_enabled,
        )
    };
    let tx_online = host.bg_tx.clone();
    let stop_online = Arc::clone(&stop);
    let _ = std::thread::Builder::new()
        .name("mochi-online".to_string())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            let mut checked_updates = false;
            while !stop_online.load(Ordering::Relaxed) {
                if let Ok(mut guard) = auth_arc.lock()
                    && guard.is_signed_in()
                {
                    if guard
                        .has_scopes(&[GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE])
                        && let Ok((cals, evs)) =
                            fetch_calendars_and_events(&mut guard, cal_ids.as_deref())
                    {
                        let _ = tx_online.send(BgEvent::CalendarLoaded(cals, evs));
                        wake();
                    }
                    if tasks_on
                        && guard.has_scopes(&[GoogleAuth::TASKS_SCOPE])
                        && let Ok(lists) = fetch_google_task_lists(&mut guard)
                    {
                        let lid = tasks_list_id
                            .clone()
                            .or_else(|| lists.first().map(|l| l.id.clone()));
                        if let Some(lid) = lid
                            && let Ok(remote) = fetch_google_tasks(&mut guard, &lid)
                        {
                            let _ = tx_online.send(BgEvent::TasksLoaded(lists, lid, remote));
                            wake();
                        }
                    }
                }
                if let Some((lat, lon)) = weather_cfg {
                    let rep = fetch_weather(lat, lon, lang);
                    let _ = tx_online.send(BgEvent::WeatherLoaded(rep));
                    wake();
                }
                if upd_enabled && !checked_updates {
                    checked_updates = true;
                    let tag = fetch_latest_release_tag();
                    let _ = tx_online.send(BgEvent::UpdateTag(tag));
                    wake();
                }
                for _ in 0..300 {
                    if stop_online.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        });

    let mut app = host.app.borrow_mut();
    app.media_service = Some(media);
    app.audio_service = Some(audio);
    host._pipe = Some(pipe);
}

fn drain_bg_events(host: &mut WindowHost) {
    while let Ok(ev) = host.bg_rx.try_recv() {
        match ev {
            BgEvent::MediaChanged => {
                let snap = host
                    .app
                    .borrow()
                    .media_service
                    .as_ref()
                    .map(MediaService::snapshot);
                if let Some(s) = snap {
                    let track_key = s
                        .current
                        .as_ref()
                        .map(|m| format!("{}\n{}\n{}", m.source_app, m.artist, m.title));
                    let lyrics_enabled = host.app.borrow().store.data.settings.lyrics_enabled;
                    host.app.borrow_mut().apply_media_snapshot(
                        s.current.clone(),
                        s.next_source,
                        Some(&host.factories),
                    );
                    if lyrics_enabled
                        && track_key != host.last_lyrics_req
                        && let Some(ref m) = s.current
                        && let Some(ref tk) = track_key
                    {
                        host.last_lyrics_req = Some(tk.clone());
                        let svc = Arc::clone(&host.app.borrow().lyrics_service);
                        let tx = host.bg_tx.clone();
                        let hwnd_raw = host.hwnd.0 as usize;
                        let title = m.title.clone();
                        let artist = m.artist.clone();
                        let src = m.source_app.clone();
                        let dur = m.duration_secs;
                        let tk_clone = tk.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(700));
                            let lyr = svc.find(&title, &artist, &src, dur);
                            let _ = tx.send(BgEvent::LyricsLoaded(tk_clone, lyr));
                            // SAFETY: Waking UI thread.
                            unsafe {
                                let _ = PostMessageW(
                                    Some(HWND(hwnd_raw as *mut _)),
                                    WM_WAKE,
                                    WPARAM(0),
                                    LPARAM(0),
                                );
                            }
                        });
                    }
                }
            }
            BgEvent::LyricsLoaded(tk, lyr) => {
                let mut app = host.app.borrow_mut();
                if app.lyrics_track.as_deref() == Some(&tk) {
                    app.set_lyrics(lyr, Some(&host.factories));
                }
            }
            BgEvent::Audio(aev) => {
                host.app
                    .borrow_mut()
                    .handle_audio_event(aev, Some(&host.factories));
            }
            BgEvent::Camera(apps) => {
                host.app
                    .borrow_mut()
                    .handle_camera_apps(apps, Some(&host.factories));
            }
            BgEvent::Hud(m) => {
                host.app.borrow_mut().show_hud(m, Some(&host.factories));
            }
            BgEvent::Network(m, _came_back) => {
                host.app.borrow_mut().show_hud(m, Some(&host.factories));
            }
            BgEvent::ActivityLine(line, reply_tx) => {
                let reply = host
                    .app
                    .borrow_mut()
                    .handle_activity(&line, Some(&host.factories));
                let _ = reply_tx.send(reply);
            }
            BgEvent::CalendarLoaded(cals, evs) => {
                let mut app = host.app.borrow_mut();
                app.calendars = cals;
                app.calendar_events = evs;
                app.calendar_last_fetched = Some(Local::now().naive_local());
                app.calendar_error = None;
                app.update_clock(Some(&host.factories));
            }
            BgEvent::GoogleAuthDone(res) => {
                let mut app = host.app.borrow_mut();
                app.calendar_connecting = false;
                app.calendar_cancel = None;
                let lang = app.lang;
                match res {
                    Ok(doc) => {
                        let ok = app
                            .google_auth
                            .lock()
                            .map_err(|_| "lock".to_string())
                            .and_then(|mut g| {
                                g.finish_oauth(
                                    doc,
                                    &[GoogleAuth::CALENDAR_LIST_SCOPE, GoogleAuth::EVENTS_SCOPE],
                                    lang,
                                )
                            });
                        match ok {
                            Ok(_) => {
                                app.calendar_error = None;
                                app.refresh_google_calendar_sync();
                            }
                            Err(e) => app.calendar_error = Some(e),
                        }
                    }
                    Err(e) => app.calendar_error = Some(e),
                }
                app.dirty = true;
            }
            BgEvent::TasksLoaded(lists, lid, remote) => {
                let mut app = host.app.borrow_mut();
                app.task_lists = lists;
                app.store.data.settings.tasks_list_id = Some(lid);
                let local_json: Vec<serde_json::Value> = app
                    .store
                    .data
                    .tasks
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "Text": t.text,
                            "Done": t.done,
                            "DueIso": t.due,
                            "DueAllDay": t.due_all_day,
                            "GoogleId": t.google_id,
                            "Dirty": t.dirty,
                        })
                    })
                    .collect();
                let remote_json: Vec<serde_json::Value> = remote
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "Id": r.id,
                            "Title": r.title,
                            "Done": r.done,
                            "DueDateIso": r.due_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                        })
                    })
                    .collect();
                if let Ok(merged) = mochi_winrt::mochi_tasks::port::apply_remote_tasks(
                    Some(local_json),
                    Some(remote_json),
                    Some(app.store.data.task_deletes.clone()),
                ) {
                    app.store.data.tasks = merged
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| mochi_winrt::paths::TaskData {
                            id: format!("sync-{i}"),
                            text: v
                                .get("Text")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            done: v
                                .get("Done")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                            due: v
                                .get("DueIso")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string),
                            due_all_day: v
                                .get("DueAllDay")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                            reminded: false,
                            google_id: v
                                .get("GoogleId")
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string),
                            dirty: v
                                .get("Dirty")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        })
                        .collect();
                    app.tasks_last_synced = Some(Local::now().naive_local());
                    app.tasks_error = None;
                    app.dirty = true;
                }
            }
            BgEvent::WeatherLoaded(rep) => {
                let mut app = host.app.borrow_mut();
                app.weather_report = rep;
                app.update_targets(Some(&host.factories));
            }
            BgEvent::UpdateTag(tag) => {
                if let Some(t) = tag {
                    let mut app = host.app.borrow_mut();
                    app.store.data.updates.latest = Some(t.clone());
                    app.store.data.updates.last_check =
                        Some(Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
                    if mochi_winrt::mochi_updates::port::is_newer_version(
                        Some(t.clone()),
                        env!("CARGO_PKG_VERSION").to_string(),
                    )
                    .unwrap_or(false)
                    {
                        app.update_available = Some(t);
                    }
                }
            }
            BgEvent::SystemNotification(ev) => {
                host.app
                    .borrow_mut()
                    .handle_notification_event(ev, Some(&host.factories));
            }
        }
    }
}

fn refresh_display_cache(host: &mut WindowHost) {
    let Ok(app) = host.app.try_borrow() else {
        return;
    };
    let disp = Monitors::resolve(app.store.data.settings.monitor.as_deref(), app.lang);
    let scale = Monitors::scale_at(disp.x + disp.width / 2, disp.y);
    host.cached_disp = disp;
    host.cached_scale = scale;
}

fn render_and_present(host: &mut WindowHost) {
    if host.rendering {
        return;
    }
    let Ok((win_w, win_h, visible, settled_compact)) = host.app.try_borrow().map(|app| {
        (
            app.window_w,
            app.window_h,
            app.is_visible(),
            !app.animating && !app.expanded,
        )
    }) else {
        return;
    };
    if !visible {
        if host.was_visible {
            host.was_visible = false;
            // SAFETY: Hiding notch window when fullscreen/locked/hidden.
            unsafe {
                let _ = ShowWindow(host.hwnd, SW_HIDE);
            }
            trim_working_set();
        }
        let _ = apply_click_through_from_cursor(host);
        return;
    }
    if !host.was_visible {
        host.was_visible = true;
        // SAFETY: Showing notch window without activating.
        unsafe {
            let _ = ShowWindow(host.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    let disp = &host.cached_disp;
    let scale = host.cached_scale;
    let pw = (win_w * scale).round() as i32;
    let ph = (win_h * scale).round() as i32;
    let px = disp.x + (disp.width - pw) / 2;
    let py = disp.y;

    host.rendering = true;
    host.last_pos = (px, py, pw, ph);
    host.last_scale = scale;
    host.surface.ensure_size(pw, ph, scale);
    if settled_compact {
        host.surface.shrink_to_fit(pw, ph);
    }
    if host.surface.begin_draw() {
        if let Ok(mut app) = host.app.try_borrow_mut() {
            app.window_screen_origin = (px, py);
            app.window_scale = scale;
            host.hits = render_notch(&mut app, &host.factories, &mut host.surface);
            app.dirty = false;
        }
        host.surface.end_draw();
        host.surface.present_layered(host.hwnd, px, py);
    }
    host.rendering = false;
    let _ = apply_click_through_from_cursor(host);
}

struct ClickThroughEval {
    entered: bool,
    left: bool,
    target: HoverTarget,
    dip: Option<(f32, f32)>,
    in_drag_approach: bool,
    drag_proximity: f64,
    clear_drag_approach: bool,
}

fn cursor_dip(host: &WindowHost) -> Option<(f32, f32)> {
    let (sx, sy) = get_cursor_pos()?;
    let mut pt = POINT { x: sx, y: sy };
    // SAFETY: Converting screen cursor coordinates to client coordinates.
    unsafe {
        let _ = ScreenToClient(host.hwnd, &raw mut pt);
    }
    let scale = host.last_scale.max(0.5) as f32;
    Some((pt.x as f32 / scale, pt.y as f32 / scale))
}

fn set_click_through(host: &mut WindowHost, transparent: bool) {
    if host.click_through == transparent {
        return;
    }
    host.click_through = transparent;
    let hwnd = host.hwnd;
    // SAFETY: `hwnd` is a valid layered window owned by this UI thread; `WM_NCCALCSIZE`/`WM_WINDOWPOS*` bypass `&mut WindowHost`.
    unsafe {
        let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = if transparent {
            cur | (WS_EX_TRANSPARENT.0 as isize)
        } else {
            cur & !(WS_EX_TRANSPARENT.0 as isize)
        };
        if next != cur {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOZORDER
                    | SWP_NOACTIVATE
                    | SWP_FRAMECHANGED
                    | SWP_NOREDRAW,
            );
        }
    }
}

fn update_hover_watch_timer(host: &mut WindowHost, near_notch: bool) {
    let desired = if host.click_through && host.was_visible {
        if near_notch { 16 } else { 80 }
    } else {
        0
    };
    if host.hover_watch_ms == desired {
        return;
    }
    host.hover_watch_ms = desired;
    // SAFETY: `host.hwnd` is valid on the UI thread.
    unsafe {
        if desired == 0 {
            let _ = KillTimer(Some(host.hwnd), TIMER_HOVER_WATCH);
        } else {
            let _ = SetTimer(Some(host.hwnd), TIMER_HOVER_WATCH, desired, None);
        }
    }
}

fn schedule_click_through_sync(host: &mut WindowHost) {
    if !host.click_through && !host.sync_pending {
        host.sync_pending = true;
        // SAFETY: Posting deferred sync avoids re-entrant `SetWindowPos` inside `WM_NCHITTEST`.
        unsafe {
            let _ = PostMessageW(Some(host.hwnd), WM_SYNC_CLICK_THROUGH, WPARAM(0), LPARAM(0));
        }
    }
}

fn apply_click_through_from_cursor(host: &mut WindowHost) -> ClickThroughEval {
    if !host.was_visible {
        let prev = host.click_through;
        set_click_through(host, true);
        update_hover_watch_timer(host, false);
        return ClickThroughEval {
            entered: false,
            left: !prev,
            target: HoverTarget::None,
            dip: None,
            in_drag_approach: false,
            drag_proximity: 0.0,
            clear_drag_approach: false,
        };
    }

    let Some((x, y)) = cursor_dip(host) else {
        let prev = host.click_through;
        set_click_through(host, true);
        update_hover_watch_timer(host, false);
        return ClickThroughEval {
            entered: false,
            left: !prev,
            target: HoverTarget::None,
            dip: None,
            in_drag_approach: false,
            drag_proximity: 0.0,
            clear_drag_approach: false,
        };
    };

    let Ok((seeking, drop_hinting, win_w, win_h, notch_w, notch_h, was_approaching)) =
        host.app.try_borrow().map(|app| {
            (
                app.seeking,
                app.drop_hinting,
                app.window_w as f32,
                app.window_h as f32,
                app.width.value.max(120.0) as f32,
                app.height.value.max(30.0) as f32,
                app.drag_approaching,
            )
        })
    else {
        return ClickThroughEval {
            entered: false,
            left: false,
            target: HoverTarget::None,
            dip: Some((x, y)),
            in_drag_approach: false,
            drag_proximity: 0.0,
            clear_drag_approach: false,
        };
    };

    let cx = win_w * 0.5;
    let ole_drag = is_ole_drag_active();
    let catch_half_w = notch_w * 0.5 + 36.0;
    let catch_h = notch_h + 44.0;
    let in_drag_catch = ole_drag && (x - cx).abs() <= catch_half_w && (0.0..=catch_h).contains(&y);
    let in_drag_approach =
        ole_drag && !in_drag_catch && (x - cx).abs() <= 280.0 && (0.0..=120.0).contains(&y);
    let drag_proximity = if in_drag_approach {
        let dx = ((x - cx).abs() - catch_half_w).max(0.0);
        let dy = (y - catch_h).max(0.0);
        let dist = (dx * dx + dy * dy).sqrt();
        (1.0 - f64::from(dist) / 140.0).clamp(0.18, 1.0)
    } else {
        0.0
    };
    let clear_drag_approach = was_approaching && !in_drag_approach && !in_drag_catch;

    let target = host.hits.hit_test(x, y);
    let in_drop_target =
        drop_hinting && ((x - cx).abs() <= 380.0 && (0.0..=EXPANDED_WIDE.1 as f32).contains(&y));
    let should_capture = seeking || in_drop_target || in_drag_catch || target != HoverTarget::None;
    let near_notch =
        ole_drag || ((x - cx).abs() <= win_w * 0.5 + 80.0 && (-8.0..=win_h + 80.0).contains(&y));

    let prev = host.click_through;
    set_click_through(host, !should_capture);
    update_hover_watch_timer(host, near_notch);

    ClickThroughEval {
        entered: prev && !host.click_through,
        left: !prev && host.click_through,
        target,
        dip: Some((x, y)),
        in_drag_approach,
        drag_proximity,
        clear_drag_approach,
    }
}

fn sync_click_through(host: &mut WindowHost) {
    let eval = apply_click_through_from_cursor(host);
    let mut need_render = false;
    if let Ok(mut app) = host.app.try_borrow_mut() {
        if eval.in_drag_approach
            && let Some((x, y)) = eval.dip
        {
            app.on_drag_approach(
                (f64::from(x), f64::from(y)),
                eval.drag_proximity,
                Some(&host.factories),
            );
        } else if eval.clear_drag_approach {
            app.clear_drag_approach(Some(&host.factories));
        }

        if eval.entered
            && let Some((x, y)) = eval.dip
        {
            app.cursor_dip = (f64::from(x), f64::from(y));
            let prev = app.hover;
            app.hover = eval.target;
            if eval.target != HoverTarget::None {
                if app.expanded {
                    app.hover_deadline = None;
                    app.pending_expand = false;
                } else if eval.target == HoverTarget::Bubble {
                    if app.pending_expand {
                        app.hover_deadline = None;
                        app.pending_expand = false;
                    }
                } else if prev == HoverTarget::None || prev == HoverTarget::Bubble {
                    app.pinned_until = None;
                    app.pending_expand = true;
                    app.hover_from = get_cursor_pos().unwrap_or((0, 0));
                    app.hover_deadline = Some(
                        Instant::now()
                            + Duration::from_millis(
                                app.store.data.settings.hover_delay_ms.max(0) as u64
                            ),
                    );
                }
            }
            if app.hover != prev {
                app.dirty = true;
            }
        } else if eval.left && app.hover != HoverTarget::None {
            app.hover = HoverTarget::None;
            app.peeking = false;
            if app.expanded
                && !app.seeking
                && app.task_input.is_empty()
                && !app.agent_prompt_focused
                && app.agent_prompt_input.is_empty()
            {
                app.pending_expand = false;
                app.hover_deadline = Some(Instant::now() + Duration::from_millis(350));
            } else if !app.expanded {
                app.hover_deadline = None;
            }
            app.dirty = true;
        }
        need_render = app.dirty;
    }
    if need_render {
        render_and_present(host);
    }
}

// SAFETY: Standard Win32 window procedure for the notch overlay window.
unsafe extern "system" fn notch_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if matches!(
        msg,
        WM_NCCREATE | WM_NCCALCSIZE | WM_WINDOWPOSCHANGING | WM_WINDOWPOSCHANGED
    ) {
        // SAFETY: Default handling for non-client/window-pos messages avoids re-entrant `&mut WindowHost` aliasing during `SetWindowPos`.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: Retrieving `WindowHost` pointer from GWLP_USERDATA.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowHost;
    if ptr.is_null() {
        // SAFETY: Default window proc.
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // SAFETY: `ptr` is valid on the UI thread until WM_DESTROY.
    let host = unsafe { &mut *ptr };

    match msg {
        WM_NCHITTEST => {
            let mut pt = POINT {
                x: ((lparam.0 & 0xFFFF) as i16) as i32,
                y: (((lparam.0 >> 16) & 0xFFFF) as i16) as i32,
            };
            // SAFETY: Converting screen coordinates to client coordinates for hit testing.
            unsafe {
                let _ = ScreenToClient(hwnd, &raw mut pt);
            }
            let x = pt.x as f32 / host.last_scale as f32;
            let y = pt.y as f32 / host.last_scale as f32;
            let Ok((seeking, drop_hinting, win_w, notch_w, notch_h, was_approaching)) =
                host.app.try_borrow().map(|app| {
                    (
                        app.seeking,
                        app.drop_hinting,
                        app.window_w as f32,
                        app.width.value.max(120.0) as f32,
                        app.height.value.max(30.0) as f32,
                        app.drag_approaching,
                    )
                })
            else {
                return if host.hits.hit_test(x, y) == HoverTarget::None {
                    schedule_click_through_sync(host);
                    LRESULT(HTTRANSPARENT as isize)
                } else {
                    LRESULT(HTCLIENT as isize)
                };
            };
            if seeking {
                return LRESULT(HTCLIENT as isize);
            }
            if drop_hinting {
                let cx = win_w * 0.5;
                let in_target =
                    (x - cx).abs() <= 380.0 && (0.0..=EXPANDED_WIDE.1 as f32).contains(&y);
                return if in_target || host.hits.hit_test(x, y) != HoverTarget::None {
                    LRESULT(HTCLIENT as isize)
                } else {
                    schedule_click_through_sync(host);
                    LRESULT(HTTRANSPARENT as isize)
                };
            }
            if is_ole_drag_active() {
                let cx = win_w * 0.5;
                let catch_half_w = notch_w * 0.5 + 36.0;
                let catch_h = notch_h + 44.0;
                if (x - cx).abs() <= catch_half_w && (0.0..=catch_h).contains(&y) {
                    return LRESULT(HTCLIENT as isize);
                }
                if (x - cx).abs() <= 280.0 && (0.0..=120.0).contains(&y) {
                    let dx = ((x - cx).abs() - catch_half_w).max(0.0);
                    let dy = (y - catch_h).max(0.0);
                    let dist = (dx * dx + dy * dy).sqrt();
                    let proximity = (1.0 - f64::from(dist) / 140.0).clamp(0.18, 1.0);
                    if let Ok(mut app) = host.app.try_borrow_mut() {
                        app.on_drag_approach(
                            (f64::from(x), f64::from(y)),
                            proximity,
                            Some(&host.factories),
                        );
                    }
                    schedule_click_through_sync(host);
                    return LRESULT(HTTRANSPARENT as isize);
                }
                if was_approaching && let Ok(mut app) = host.app.try_borrow_mut() {
                    app.clear_drag_approach(Some(&host.factories));
                }
                schedule_click_through_sync(host);
                return LRESULT(HTTRANSPARENT as isize);
            }
            if was_approaching && let Ok(mut app) = host.app.try_borrow_mut() {
                app.clear_drag_approach(Some(&host.factories));
            }
            if host.hits.hit_test(x, y) == HoverTarget::None {
                schedule_click_through_sync(host);
                LRESULT(HTTRANSPARENT as isize)
            } else {
                LRESULT(HTCLIENT as isize)
            }
        }
        WM_SYNC_CLICK_THROUGH => {
            host.sync_pending = false;
            sync_click_through(host);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                let dirty = if let Ok(mut app) = host.app.try_borrow_mut() {
                    if app.keyboard_mode || app.task_input_focused || app.agent_prompt_focused {
                        app.task_input_focused = false;
                        app.agent_prompt_focused = false;
                        exit_keyboard_mode(hwnd, &mut app, false);
                        app.dirty = true;
                    }
                    app.dirty
                } else {
                    false
                };
                if dirty {
                    render_and_present(host);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => {
            if host.app.try_borrow().is_ok_and(|a| a.keyboard_mode) {
                // SAFETY: Default activation when typing in Tasks input.
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            } else {
                LRESULT(MA_NOACTIVATE as isize)
            }
        }
        WM_WAKE => {
            drain_bg_events(host);
            refresh_display_cache(host);
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.copilot_hooks_on =
                    AgentSetup::for_this_user().status("copilot") == AgentStatus::On;
            }
            update_hotkeys(host);
            apply_capture_exclusion(host);
            render_and_present(host);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            refresh_display_cache(host);
            render_and_present(host);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_HOVER_WATCH {
                sync_click_through(host);
                return LRESULT(0);
            }
            refresh_display_cache(host);
            sync_drag_approach(host);
            let Ok(mut app) = host.app.try_borrow_mut() else {
                return LRESULT(0);
            };
            let mon = app.store.data.settings.monitor.clone();
            let lang = app.lang;
            let fs = detect_fullscreen(hwnd, mon.as_deref(), lang);
            if fs != app.fullscreen_app {
                app.fullscreen_app = fs;
                app.dirty = true;
                if fs && app.store.data.settings.hide_in_fullscreen {
                    app.set_expanded(false, Some(&host.factories));
                    app.interrupt_hud();
                }
            }
            if !fs && app.is_visible() {
                keep_topmost(hwnd);
            }
            app.poll_periodic(Some(&host.factories));
            app.update_clock(Some(&host.factories));
            if app.focus.state == crate::state::FocusState::Running
                || (app.expanded
                    && app.page == NotchPage::Media
                    && app.media_state.as_ref().is_some_and(|m| m.is_playing))
            {
                app.dirty = true;
            }
            let dirty = app.dirty;
            drop(app);
            if dirty {
                render_and_present(host);
            }
            sync_click_through(host);
            if !host.startup_trimmed && host.last_trim_at.elapsed() >= Duration::from_secs(5) {
                host.startup_trimmed = true;
                host.last_trim_at = Instant::now();
                trim_working_set();
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            let id = wparam.0 as i32;
            match id {
                HOTKEY_JUMP => {
                    host.app.borrow_mut().on_jump_hotkey(Some(&host.factories));
                }
                HOTKEY_MIC => {
                    if let Some(ref audio) = host.app.borrow().audio_service {
                        audio.toggle_mic_mute();
                    }
                }
                HOTKEY_TOGGLE => {
                    let mut app = host.app.borrow_mut();
                    if app.hidden_by_user {
                        app.hidden_by_user = false;
                    }
                    if app.is_visible() {
                        let next = !app.expanded;
                        app.hover_deadline = None;
                        app.pinned_until = if next {
                            Some(Instant::now() + Duration::from_secs(10))
                        } else {
                            None
                        };
                        app.set_expanded(next, Some(&host.factories));
                    }
                }
                _ => {}
            }
            render_and_present(host);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if !host.tracking_mouse {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: `tme` has `cbSize` initialized.
                unsafe {
                    let _ = TrackMouseEvent(&raw mut tme);
                }
                host.tracking_mouse = true;
            }
            let x = ((lparam.0 & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let Ok(mut app) = host.app.try_borrow_mut() else {
                return LRESULT(0);
            };
            app.cursor_dip = (f64::from(x), f64::from(y));
            if app.seeking
                && let Some(pr) = host.hits.progress_rect
            {
                app.seek_fraction = f64::from(((x - pr.x) / pr.w.max(1.0)).clamp(0.0, 1.0));
                app.dirty = true;
            } else {
                let target = host.hits.hit_test(x, y);
                let prev = app.hover;
                app.hover = target;
                if target != HoverTarget::None {
                    if app.expanded {
                        app.hover_deadline = None;
                        app.pending_expand = false;
                    } else if target == HoverTarget::Bubble {
                        if app.pending_expand {
                            app.hover_deadline = None;
                            app.pending_expand = false;
                        }
                    } else if prev == HoverTarget::None || prev == HoverTarget::Bubble {
                        app.pinned_until = None;
                        app.pending_expand = true;
                        app.hover_from = get_cursor_pos().unwrap_or((0, 0));
                        app.hover_deadline = Some(
                            Instant::now()
                                + Duration::from_millis(
                                    app.store.data.settings.hover_delay_ms.max(0) as u64,
                                ),
                        );
                    }
                } else if prev != HoverTarget::None {
                    app.peeking = false;
                    if app.expanded
                        && !app.seeking
                        && app.task_input.is_empty()
                        && !app.agent_prompt_focused
                        && app.agent_prompt_input.is_empty()
                    {
                        app.pending_expand = false;
                        app.hover_deadline = Some(Instant::now() + Duration::from_millis(350));
                    } else if !app.expanded {
                        app.hover_deadline = None;
                    }
                }
                if target != prev {
                    app.dirty = true;
                }
            }
            let dirty = app.dirty;
            drop(app);
            if dirty {
                render_and_present(host);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            host.tracking_mouse = false;
            let Ok(mut app) = host.app.try_borrow_mut() else {
                return LRESULT(0);
            };
            app.hover = HoverTarget::None;
            app.peeking = false;
            if app.expanded
                && !app.seeking
                && app.task_input.is_empty()
                && !app.agent_prompt_focused
                && app.agent_prompt_input.is_empty()
            {
                app.pending_expand = false;
                app.hover_deadline = Some(Instant::now() + Duration::from_millis(350));
            } else if !app.expanded {
                app.hover_deadline = None;
            }
            app.dirty = true;
            drop(app);
            render_and_present(host);
            sync_click_through(host);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = ((lparam.0 & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let target = host.hits.hit_test(x, y);
            let Ok(mut app) = host.app.try_borrow_mut() else {
                return LRESULT(0);
            };
            app.pressed = target;
            if target == HoverTarget::ProgressTrack
                && let Some(pr) = host.hits.progress_rect
            {
                app.seeking = true;
                app.seek_fraction = f64::from(((x - pr.x) / pr.w.max(1.0)).clamp(0.0, 1.0));
            }
            app.dirty = true;
            drop(app);
            render_and_present(host);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let x = ((lparam.0 & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let target = host.hits.hit_test(x, y);
            let was_seeking = host.app.try_borrow().is_ok_and(|a| a.seeking);
            if was_seeking {
                if let Ok(mut app) = host.app.try_borrow_mut() {
                    app.seeking = false;
                    let frac = app.seek_fraction;
                    if let Some(ref m) = app.media_state
                        && let Some(ref svc) = app.media_service
                    {
                        svc.seek(m.duration_secs * frac);
                    }
                    app.pressed = HoverTarget::None;
                    app.dirty = true;
                }
            } else {
                handle_click(host, target);
            }
            render_and_present(host);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as i16) as f64 / 120.0;
            let Ok(mut app) = host.app.try_borrow_mut() else {
                return LRESULT(0);
            };
            let cy = app.cursor_dip.1;
            let eh = app.height.target;
            if app.expanded && (cy <= 40.0 || cy >= eh - 38.0) {
                let step = if delta > 0.0 { -1 } else { 1 };
                app.step_page(step, Some(&host.factories));
                drop(app);
                render_and_present(host);
                return LRESULT(0);
            }
            match app.page {
                NotchPage::Agenda => {
                    app.agenda_scroll = (app.agenda_scroll - delta * 36.0).max(0.0)
                }
                NotchPage::Tasks => app.tasks_scroll = (app.tasks_scroll - delta * 36.0).max(0.0),
                NotchPage::Shelf => app.shelf_scroll = (app.shelf_scroll - delta * 48.0).max(0.0),
                NotchPage::Notifications => {
                    app.notifications_scroll = (app.notifications_scroll - delta * 42.0).max(0.0);
                }
                NotchPage::Clipboard => {
                    app.clipboard_scroll = (app.clipboard_scroll - delta * 40.0).max(0.0);
                }
                NotchPage::Focus if app.focus_picker_open => {
                    app.focus_picker_scroll = (app.focus_picker_scroll - delta * 28.0).max(0.0);
                }
                _ => {}
            }
            app.dirty = true;
            drop(app);
            render_and_present(host);
            LRESULT(0)
        }
        WM_CHAR => {
            let Ok(mut app) = host.app.try_borrow_mut() else {
                return LRESULT(0);
            };
            if app.task_input_focused {
                let ch = char::from_u32((wparam.0 & 0xFFFF) as u32);
                match ch {
                    Some('\x08') => {
                        app.task_input.pop();
                    }
                    Some('\r') => {
                        let txt = std::mem::take(&mut app.task_input);
                        app.add_task(&txt);
                    }
                    Some(c) if !c.is_control() && app.task_input.chars().count() < 200 => {
                        app.task_input.push(c);
                    }
                    _ => {}
                }
                app.dirty = true;
                drop(app);
                render_and_present(host);
            } else if app.agent_prompt_focused {
                let ch = char::from_u32((wparam.0 & 0xFFFF) as u32);
                match ch {
                    Some('\x08') => {
                        app.agent_prompt_input.pop();
                    }
                    Some('\r') => {
                        let prompt = std::mem::take(&mut app.agent_prompt_input);
                        app.agent_prompt_focused = false;
                        exit_keyboard_mode(hwnd, &mut app, false);
                        if !prompt.trim().is_empty() {
                            let win = app
                                .activities
                                .iter()
                                .find(|a| a.icon.eq_ignore_ascii_case("copilot"))
                                .and_then(|a| a.window);
                            trigger_vscode_chat_action(win, VsCodeChatAction::SendPrompt(prompt));
                        }
                    }
                    Some(c) if !c.is_control() && app.agent_prompt_input.chars().count() < 400 => {
                        app.agent_prompt_input.push(c);
                    }
                    _ => {}
                }
                app.dirty = true;
                drop(app);
                render_and_present(host);
            } else if app.weather_search_focused {
                let ch = char::from_u32((wparam.0 & 0xFFFF) as u32);
                match ch {
                    Some('\x08') => {
                        app.weather_search_input.pop();
                    }
                    Some('\r') => {
                        let q = app.weather_search_input.clone();
                        if !q.trim().is_empty() {
                            app.search_weather(&q);
                        }
                    }
                    Some(c) if !c.is_control() && app.weather_search_input.chars().count() < 80 => {
                        app.weather_search_input.push(c);
                    }
                    _ => {}
                }
                app.dirty = true;
                drop(app);
                render_and_present(host);
            } else if app.pray_search_focused {
                let ch = char::from_u32((wparam.0 & 0xFFFF) as u32);
                match ch {
                    Some('\x08') => {
                        app.pray_search_input.pop();
                    }
                    Some('\r') => {
                        let q = app.pray_search_input.clone();
                        if !q.trim().is_empty() {
                            app.search_pray_places(&q);
                        }
                    }
                    Some(c) if !c.is_control() && app.pray_search_input.chars().count() < 80 => {
                        app.pray_search_input.push(c);
                    }
                    _ => {}
                }
                app.dirty = true;
                drop(app);
                render_and_present(host);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == 0x1B {
                let mut close_bench = false;
                if let Ok(mut app) = host.app.try_borrow_mut() {
                    if app.task_input_focused {
                        app.task_input.clear();
                        app.task_input_focused = false;
                        exit_keyboard_mode(hwnd, &mut app, true);
                        app.dirty = true;
                    } else if app.agent_prompt_focused {
                        app.agent_prompt_input.clear();
                        app.agent_prompt_focused = false;
                        exit_keyboard_mode(hwnd, &mut app, true);
                        app.dirty = true;
                    } else if app.weather_search_focused {
                        app.weather_search_focused = false;
                        exit_keyboard_mode(hwnd, &mut app, true);
                        app.dirty = true;
                    } else if app.pray_search_focused {
                        app.pray_search_focused = false;
                        exit_keyboard_mode(hwnd, &mut app, true);
                        app.dirty = true;
                    } else if app.bench.is_some() {
                        close_bench = true;
                    }
                }
                if close_bench {
                    // SAFETY: Closing bench window on Escape.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    return LRESULT(0);
                }
            }
            if host.app.try_borrow().is_ok_and(|a| a.dirty) {
                render_and_present(host);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let x = ((lparam.0 & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            let y = (((lparam.0 >> 16) & 0xFFFF) as i16) as f32 / host.last_scale as f32;
            if host.hits.hit_test(x, y) != HoverTarget::None {
                show_tray_menu(host);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: Destroying notch window on close request.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DROPFILES => {
            let hdrop = HDROP(wparam.0 as *mut _);
            // SAFETY: Querying dropped file count.
            let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
            let mut paths = Vec::with_capacity(count as usize);
            for i in 0..count {
                // SAFETY: Querying length of path `i`.
                let len = unsafe { DragQueryFileW(hdrop, i, None) };
                let mut buf = vec![0u16; len as usize + 1];
                // SAFETY: Reading path `i` into `buf`.
                unsafe {
                    DragQueryFileW(hdrop, i, Some(&mut buf));
                }
                paths.push(String::from_utf16_lossy(&buf[..len as usize]));
            }
            // SAFETY: Releasing HDROP.
            unsafe {
                DragFinish(hdrop);
            }
            let entries = NotchApp::prepare_shelf_entries(&paths, Some(&host.factories));
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.on_drag_drop_entries(entries, None, Some(&host.factories));
            }
            render_and_present(host);
            LRESULT(0)
        }
        WM_TRAY => {
            let ev = (lparam.0 & 0xFFFF) as u32;
            if ev == WM_RBUTTONUP {
                show_tray_menu(host);
            } else if ev == windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK {
                open_settings_from_host(host);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd = wparam.0 & 0xFFFF;
            match cmd {
                CMD_OPEN => {
                    let mut app = host.app.borrow_mut();
                    app.hidden_by_user = false;
                    app.pinned_until = Some(Instant::now() + Duration::from_secs(10));
                    app.set_expanded(true, Some(&host.factories));
                }
                CMD_HIDE => {
                    let mut app = host.app.borrow_mut();
                    app.hidden_by_user = !app.hidden_by_user;
                    app.pinned_until = None;
                    if app.hidden_by_user {
                        app.set_expanded(false, Some(&host.factories));
                        app.interrupt_hud();
                    }
                }
                CMD_SETTINGS => open_settings_from_host(host),
                CMD_UPDATES => {
                    let tag = host.app.borrow_mut().check_updates_sync();
                    let mut app = host.app.borrow_mut();
                    let s = app.lang.strings();
                    let mut m = if let Some(t) = tag {
                        if app.update_available.is_some() {
                            let mut msg = HudMessage::new(Glyphs::BELL, s.hud_new_version);
                            msg.value = Some(t);
                            msg.duration_ms = 5000;
                            msg
                        } else {
                            let mut msg = HudMessage::new("\u{E73E}", s.hud_up_to_date);
                            msg.value = Some(env!("CARGO_PKG_VERSION").to_string());
                            msg.duration_ms = 3000;
                            msg
                        }
                    } else {
                        let mut msg = HudMessage::new(Glyphs::BELL, s.hud_check_failed);
                        msg.warning = true;
                        msg.duration_ms = 3500;
                        msg
                    };
                    m.kind = HudKind::Reminder;
                    app.show_hud(m, Some(&host.factories));
                }
                CMD_DEMO => {
                    let mut app = host.app.borrow_mut();
                    if app.simulated_idle_secs.is_some() || app.force_ambient {
                        app.simulated_idle_secs = None;
                        app.force_ambient = false;
                        app.presence_state = 0;
                    } else {
                        app.set_expanded(false, Some(&host.factories));
                        let idle_secs =
                            (f64::from(app.store.data.settings.companion_idle_minutes.max(1))
                                * 60.0)
                                .max(180.0);
                        app.simulated_idle_secs = Some(idle_secs);
                        app.presence_state = 1;
                    }
                    app.update_companion();
                    app.update_targets(Some(&host.factories));
                }
                CMD_QUIT => {
                    // SAFETY: Destroying main window to quit.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }
            render_and_present(host);
            LRESULT(0)
        }
        WM_CLIPBOARDUPDATE => {
            if let Some((seq, payload)) = read_clipboard_payload()
                && let Ok(mut app) = host.app.try_borrow_mut()
            {
                app.capture_clipboard_payload(seq, payload, Some(&host.factories));
            }
            render_and_present(host);
            LRESULT(0)
        }
        WM_DESTROY => {
            host.app.borrow().stop_flag.store(true, Ordering::Relaxed);
            if host.app.borrow().bench.is_none() {
                host.app.borrow().store.save_now();
            }
            remove_tray_icon(host);
            // SAFETY: Revoking OLE drop target, clipboard listener, clearing GWLP_USERDATA, killing timer, unregistering hotkeys, and posting quit.
            unsafe {
                if host.clipboard_listening {
                    let _ = RemoveClipboardFormatListener(hwnd);
                    host.clipboard_listening = false;
                }
                if host.drop_registered {
                    let _ = RevokeDragDrop(hwnd);
                    host.drop_registered = false;
                }
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let _ = KillTimer(Some(hwnd), TIMER_MAIN);
                let _ = KillTimer(Some(hwnd), TIMER_HOVER_WATCH);
                if host.hotkey_reg {
                    let _ = UnregisterHotKey(Some(hwnd), HOTKEY_TOGGLE);
                }
                if host.jump_reg {
                    let _ = UnregisterHotKey(Some(hwnd), HOTKEY_JUMP);
                }
                if host.mic_reg {
                    let _ = UnregisterHotKey(Some(hwnd), HOTKEY_MIC);
                }
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn handle_click(host: &mut WindowHost, target: HoverTarget) {
    let mut open_settings = false;
    {
        let mut app = host.app.borrow_mut();
        app.pressed = HoverTarget::None;
        if target != HoverTarget::TaskInput && app.task_input_focused {
            app.task_input_focused = false;
            exit_keyboard_mode(host.hwnd, &mut app, true);
        }
        if target != HoverTarget::AgentPromptInput && app.agent_prompt_focused {
            app.agent_prompt_focused = false;
            let restore = !matches!(
                target,
                HoverTarget::AgentPromptSend | HoverTarget::AgentNewChat
            );
            exit_keyboard_mode(host.hwnd, &mut app, restore);
        }
        if target != HoverTarget::WeatherSearchInput && app.weather_search_focused {
            app.weather_search_focused = false;
            exit_keyboard_mode(host.hwnd, &mut app, true);
        }
        if target != HoverTarget::PraySearchInput && app.pray_search_focused {
            app.pray_search_focused = false;
            exit_keyboard_mode(host.hwnd, &mut app, true);
        }
        match target {
            HoverTarget::Notch => {
                if !app.expanded {
                    app.hover_deadline = None;
                    if !app.go_to_activity_window() {
                        app.set_expanded(true, Some(&host.factories));
                    }
                }
            }
            HoverTarget::Bubble => {
                if let Some(ref b) = app.bubble_item.clone()
                    && let Some(win) = b.window
                    && let Some(ref id) = b.activity_id
                    && !focus_host_window(win.0, win.1)
                    && let Some(a) = app.activities.iter_mut().find(|x| x.id == *id)
                {
                    a.window = None;
                    app.update_bubble();
                }
            }
            HoverTarget::Tab(i) => {
                let page = match i {
                    1 => NotchPage::Agenda,
                    2 => NotchPage::Focus,
                    3 => NotchPage::Water,
                    4 => NotchPage::Pray,
                    5 => NotchPage::Tasks,
                    6 => NotchPage::Shelf,
                    7 => NotchPage::Notifications,
                    8 => NotchPage::Clipboard,
                    9 => NotchPage::Weather,
                    _ => NotchPage::Media,
                };
                app.select_page(page, Some(&host.factories));
            }
            HoverTarget::SettingsBtn => open_settings = true,
            HoverTarget::RefreshBtn => {
                if app.page == NotchPage::Agenda {
                    app.refresh_google_calendar_sync();
                } else if app.page == NotchPage::Tasks {
                    app.refresh_google_tasks_sync();
                } else if app.page == NotchPage::Weather {
                    app.refresh_weather_sync();
                }
            }
            HoverTarget::SwitchPlayerBtn => {
                if let Some(ref svc) = app.media_service {
                    svc.switch_to_next();
                }
            }
            HoverTarget::RemovePlayerBtn => {
                if let Some(ref svc) = app.media_service {
                    svc.remove_current();
                }
            }
            HoverTarget::PrevBtn => {
                if let Some(ref svc) = app.media_service {
                    svc.previous();
                }
            }
            HoverTarget::PlayPauseBtn => {
                if let Some(ref svc) = app.media_service {
                    svc.toggle_play_pause();
                }
            }
            HoverTarget::NextBtn => {
                if let Some(ref svc) = app.media_service {
                    svc.next();
                }
            }
            HoverTarget::LyricLine(i) => {
                if let Some(ref lyr) = app.lyrics
                    && let Some(line) = lyr.lines.get(i)
                    && let Some(ref svc) = app.media_service
                {
                    svc.seek(line.time_seconds);
                }
            }
            HoverTarget::AgendaEmptyBtn => {
                if app.calendar_connecting {
                    if let Some(ref cancel) = app.calendar_cancel {
                        cancel.store(true, Ordering::Relaxed);
                    }
                    app.calendar_connecting = false;
                    app.calendar_cancel = None;
                } else {
                    let signed_in = app.google_auth.lock().is_ok_and(|g| g.is_signed_in());
                    if !signed_in {
                        let lang = app.lang;
                        let configured =
                            app.google_auth.lock().is_ok_and(|mut g| g.reload_client());
                        if !configured {
                            open_settings = true;
                        } else {
                            let session_res = app
                                .google_auth
                                .lock()
                                .map_err(|_| "lock".to_string())
                                .and_then(|mut g| {
                                    g.begin_oauth(
                                        &[
                                            GoogleAuth::CALENDAR_LIST_SCOPE,
                                            GoogleAuth::EVENTS_SCOPE,
                                        ],
                                        lang,
                                    )
                                });
                            match session_res {
                                Ok(session) => {
                                    let cancel = Arc::new(AtomicBool::new(false));
                                    app.calendar_connecting = true;
                                    app.calendar_cancel = Some(Arc::clone(&cancel));
                                    app.calendar_error = None;
                                    let tx = host.bg_tx.clone();
                                    let hwnd_raw = host.hwnd.0 as usize;
                                    std::thread::spawn(move || {
                                        let res = session.wait_and_exchange(cancel, lang);
                                        let _ = tx.send(BgEvent::GoogleAuthDone(res));
                                        // SAFETY: Waking UI thread after OAuth completes.
                                        unsafe {
                                            let _ = PostMessageW(
                                                Some(HWND(hwnd_raw as *mut _)),
                                                WM_WAKE,
                                                WPARAM(0),
                                                LPARAM(0),
                                            );
                                        }
                                    });
                                }
                                Err(e) => app.calendar_error = Some(e),
                            }
                        }
                    }
                }
            }
            HoverTarget::AgendaJoin(i) => {
                let rows = app.build_agenda_rows_for_selected();
                if let Some(row) = rows.get(i)
                    && let Some(ref url) = row.meeting_url
                {
                    NotchApp::open_external_url(url);
                }
            }
            HoverTarget::CalendarPrevMonth => app.step_calendar_month(-1),
            HoverTarget::CalendarNextMonth => app.step_calendar_month(1),
            HoverTarget::CalendarToday => app.select_calendar_today(),
            HoverTarget::CalendarDay(y, m, d) => {
                if let Some(date) = chrono::NaiveDate::from_ymd_opt(y, m, d) {
                    app.calendar_view_year = y;
                    app.calendar_view_month = m;
                    app.calendar_selected_date = date;
                    app.agenda_scroll = 0.0;
                }
            }
            HoverTarget::FocusPrimary => {
                let st = app.focus.state;
                match st {
                    crate::state::FocusState::Ready => app.focus.start(),
                    crate::state::FocusState::Running => {
                        if let Some(dur) = app.focus.pause() {
                            let key = Local::now().format("%Y-%m-%d").to_string();
                            *app.store.data.focus.days.entry(key).or_insert(0) +=
                                dur.as_secs_f64().round() as i32;
                            app.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
                        }
                    }
                    crate::state::FocusState::Paused => app.focus.resume(),
                }
                app.update_targets(Some(&host.factories));
            }
            HoverTarget::FocusSecondary => {
                let settings = app.store.data.settings.clone();
                if app.focus.is_break() {
                    let (_, msg) = app.focus.skip_break(&settings);
                    if let Some(m) = msg {
                        app.show_hud(m, Some(&host.factories));
                    }
                } else if let Some(dur) = app.focus.stop(&settings) {
                    let key = Local::now().format("%Y-%m-%d").to_string();
                    *app.store.data.focus.days.entry(key).or_insert(0) +=
                        dur.as_secs_f64().round() as i32;
                    app.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
                }
                app.update_targets(Some(&host.factories));
            }
            HoverTarget::FocusTaskCheck => {
                if let Some(tid) = app.store.data.focus.task_id.clone()
                    && let Some(pos) = app.store.data.tasks.iter().position(|t| t.id == tid)
                {
                    app.toggle_task(pos);
                    app.store.data.focus.task_id = None;
                }
            }
            HoverTarget::FocusTaskPick => {
                app.focus_picker_open = true;
                app.focus_picker_scroll = 0.0;
            }
            HoverTarget::FocusPickClose => app.focus_picker_open = false,
            HoverTarget::FocusPickRow(row_idx) => {
                let has_cur = app.store.data.focus.task_id.is_some();
                if has_cur && row_idx == 0 {
                    app.store.data.focus.task_id = None;
                } else {
                    let open_idx = if has_cur { row_idx - 1 } else { row_idx };
                    let picked_id = app
                        .store
                        .data
                        .tasks
                        .iter()
                        .filter(|t| !t.done)
                        .nth(open_idx)
                        .map(|t| t.id.clone());
                    app.store.data.focus.task_id = picked_id;
                }
                app.focus_picker_open = false;
                app.store_dirty_at = Some(Instant::now() + Duration::from_millis(500));
            }
            HoverTarget::WaterDrink => app.water_drink(Some(&host.factories)),
            HoverTarget::WaterUndo => app.water_undo(Some(&host.factories)),
            HoverTarget::TaskToggle(i) => app.toggle_task(i),
            HoverTarget::TaskRemove(i) => app.remove_task(i),
            HoverTarget::TaskInput => {
                app.task_input_focused = true;
                enter_keyboard_mode(host.hwnd, &mut app);
            }
            HoverTarget::TaskAddBtn => {
                let txt = std::mem::take(&mut app.task_input);
                app.add_task(&txt);
            }
            HoverTarget::ShelfTile(i) => {
                if let Some(item) = app.shelf_items.get(i)
                    && item.available
                {
                    NotchApp::open_local_path(&item.path);
                }
            }
            HoverTarget::ShelfRemove(i) => app.remove_shelf_item(i),
            HoverTarget::ShelfClear => app.clear_shelf(),
            HoverTarget::UndoBtn => app.apply_undo(),
            HoverTarget::StripGo(i) => {
                if let Some(act) = app.activities.get(i).cloned()
                    && let Some(win) = act.window
                    && !focus_host_window(win.0, win.1)
                    && let Some(a) = app.activities.iter_mut().find(|x| x.id == act.id)
                {
                    a.window = None;
                }
            }
            HoverTarget::StripChat(i) => {
                let win = app.activities.get(i).and_then(|a| a.window);
                trigger_vscode_chat_action(win, VsCodeChatAction::OpenChat);
            }
            HoverTarget::StripStop(i) => {
                if let Some(act) = app.activities.get(i).cloned() {
                    if act.icon.eq_ignore_ascii_case("copilot") {
                        trigger_vscode_chat_action(act.window, VsCodeChatAction::StopAgent);
                    } else if let Some(win) = act.window {
                        send_terminal_interrupt(win.0, win.1);
                    }
                    if let Some(a) = app.activities.get_mut(i) {
                        a.state = "Done".to_string();
                        a.badge = Some("\u{E73E}".to_string());
                        a.line = format!("{} · Stopped", a.title);
                    }
                    app.update_bubble();
                }
            }
            HoverTarget::StripDismiss(i) if i < app.activities.len() => {
                app.activities.remove(i);
                app.update_bubble();
                app.update_targets(Some(&host.factories));
            }
            HoverTarget::AgentPromptInput => {
                app.agent_prompt_focused = true;
                enter_keyboard_mode(host.hwnd, &mut app);
            }
            HoverTarget::AgentPromptSend => {
                let prompt = std::mem::take(&mut app.agent_prompt_input);
                if !prompt.trim().is_empty() {
                    let win = app
                        .activities
                        .iter()
                        .find(|a| a.icon.eq_ignore_ascii_case("copilot"))
                        .and_then(|a| a.window);
                    trigger_vscode_chat_action(win, VsCodeChatAction::SendPrompt(prompt));
                }
            }
            HoverTarget::AgentNewChat => {
                let win = app
                    .activities
                    .iter()
                    .find(|a| a.icon.eq_ignore_ascii_case("copilot"))
                    .and_then(|a| a.window);
                trigger_vscode_chat_action(win, VsCodeChatAction::NewChat);
            }
            HoverTarget::HeaderCopilotBtn => {
                let win = app
                    .activities
                    .iter()
                    .find(|a| a.icon.eq_ignore_ascii_case("copilot"))
                    .and_then(|a| a.window);
                trigger_vscode_chat_action(win, VsCodeChatAction::OpenChat);
            }
            HoverTarget::HeaderNotifBtn => {
                app.select_page(NotchPage::Notifications, Some(&host.factories));
            }
            HoverTarget::NotifItem(i) => {
                if let Some(item) = app.notifications.get_mut(i) {
                    item.unread = false;
                }
            }
            HoverTarget::NotifDismiss(i) => app.dismiss_notification(i),
            HoverTarget::NotifMarkAllRead => app.mark_all_notifications_read(),
            HoverTarget::NotifClearAll => app.clear_notifications(),
            HoverTarget::NotifOpenWinSettings => {
                NotchApp::open_external_url("ms-settings:notifications");
            }
            HoverTarget::ClipItem(i) => app.copy_clipboard_item(i),
            HoverTarget::ClipPin(i) => app.toggle_pin_clipboard_item(i),
            HoverTarget::ClipRemove(i) => app.remove_clipboard_item(i),
            HoverTarget::ClipClear => app.clear_clipboard_history(),
            HoverTarget::WeatherCityBtn => {
                app.weather_picker_open = true;
                app.weather_search_focused = true;
                enter_keyboard_mode(host.hwnd, &mut app);
            }
            HoverTarget::WeatherSearchInput => {
                app.weather_search_focused = true;
                enter_keyboard_mode(host.hwnd, &mut app);
            }
            HoverTarget::WeatherSearchBtn => {
                let q = app.weather_search_input.clone();
                if !q.trim().is_empty() {
                    app.search_weather(&q);
                }
            }
            HoverTarget::WeatherLocateBtn => {
                if let Some((lat, lon)) = locate_once() {
                    app.store.data.settings.weather_latitude = Some(lat);
                    app.store.data.settings.weather_longitude = Some(lon);
                    app.store.data.settings.weather_place = Some(format!("{lat:.2}, {lon:.2}"));
                    app.store.data.settings.weather_enabled = true;
                    app.weather_picker_open = false;
                    app.weather_places.clear();
                    app.store.save_now();
                    app.refresh_weather_sync();
                }
            }
            HoverTarget::WeatherPickPlace(i) => {
                if let Some(p) = app.weather_places.get(i).cloned() {
                    app.store.data.settings.weather_latitude = Some(p.latitude);
                    app.store.data.settings.weather_longitude = Some(p.longitude);
                    app.store.data.settings.weather_place = Some(if p.detail.is_empty() {
                        p.name
                    } else {
                        format!("{}, {}", p.name, p.detail)
                    });
                    app.store.data.settings.weather_enabled = true;
                    app.weather_picker_open = false;
                    app.weather_places.clear();
                    app.weather_search_input.clear();
                    app.store.save_now();
                    app.refresh_weather_sync();
                }
            }
            HoverTarget::WeatherClosePicker => {
                app.weather_picker_open = false;
                app.weather_places.clear();
            }
            HoverTarget::PrayCityBtn => {
                app.pray_picker_open = true;
                app.pray_search_focused = true;
                enter_keyboard_mode(host.hwnd, &mut app);
            }
            HoverTarget::PrayAlertToggle => app.toggle_pray_alerts(),
            HoverTarget::PrayPreAlertToggle => app.toggle_pray_pre_alert(),
            HoverTarget::PrayMethodCycle => app.cycle_pray_method(),
            HoverTarget::PrayMuteToggle(i) => app.toggle_pray_mute(i),
            HoverTarget::PraySearchInput => {
                app.pray_search_focused = true;
                enter_keyboard_mode(host.hwnd, &mut app);
            }
            HoverTarget::PraySearchBtn => {
                let q = app.pray_search_input.clone();
                if !q.trim().is_empty() {
                    app.search_pray_places(&q);
                }
            }
            HoverTarget::PrayLocateBtn => {
                if let Some((lat, lon)) = locate_once() {
                    app.store.data.settings.pray_latitude = Some(lat);
                    app.store.data.settings.pray_longitude = Some(lon);
                    app.store.data.settings.pray_place = Some(format!("{lat:.2}, {lon:.2}"));
                    app.pray_picker_open = false;
                    app.pray_places.clear();
                    app.store.save_now();
                    app.recompute_prayer_schedule();
                }
            }
            HoverTarget::PrayPickPlace(i) => {
                if let Some(p) = app.pray_places.get(i).cloned() {
                    app.store.data.settings.pray_latitude = Some(p.latitude);
                    app.store.data.settings.pray_longitude = Some(p.longitude);
                    app.store.data.settings.pray_place = Some(if p.detail.is_empty() {
                        p.name
                    } else {
                        format!("{}, {}", p.name, p.detail)
                    });
                    app.pray_picker_open = false;
                    app.pray_places.clear();
                    app.pray_search_input.clear();
                    app.store.save_now();
                    app.recompute_prayer_schedule();
                }
            }
            HoverTarget::PrayClosePicker => {
                app.pray_picker_open = false;
                app.pray_places.clear();
            }
            _ => {}
        }
        app.dirty = true;
    }
    if open_settings {
        open_settings_from_host(host);
    }
}

fn enter_keyboard_mode(hwnd: HWND, app: &mut NotchApp) {
    if app.keyboard_mode {
        return;
    }
    app.keyboard_mode = true;
    // SAFETY: Saving previous foreground window and removing WS_EX_NOACTIVATE to accept typing.
    unsafe {
        app.previous_foreground = GetForegroundWindow().0 as isize;
        let ex = GetWindowLongPtrW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE,
            ex & !(WS_EX_NOACTIVATE.0 as isize),
        );
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
    }
}

fn exit_keyboard_mode(hwnd: HWND, app: &mut NotchApp, restore_focus: bool) {
    if !app.keyboard_mode {
        return;
    }
    app.keyboard_mode = false;
    // SAFETY: Restoring WS_EX_NOACTIVATE and optionally returning focus to previous foreground window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE,
            ex | (WS_EX_NOACTIVATE.0 as isize),
        );
        if restore_focus && app.previous_foreground != 0 {
            let _ = SetForegroundWindow(HWND(app.previous_foreground as *mut _));
        }
    }
    app.previous_foreground = 0;
}

fn open_settings_from_host(host: &mut WindowHost) {
    host.app
        .borrow_mut()
        .set_expanded(false, Some(&host.factories));
    let app_c = Rc::clone(&host.app);
    let fac_c = Rc::clone(&host.factories);
    let hwnd_raw = host.hwnd.0 as usize;
    open_settings_window(
        app_c,
        fac_c,
        false,
        Rc::new(move || {
            // SAFETY: Posting wake message to notch window.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd_raw as *mut _)),
                    WM_WAKE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }),
    );
}

fn update_hotkeys(host: &mut WindowHost) {
    let (hk, jhk, mhk) = {
        let app = host.app.borrow();
        let s = &app.store.data.settings;
        (
            s.hotkey_enabled,
            s.jump_hotkey_enabled,
            s.mic_hotkey_enabled,
        )
    };
    let mods = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
    if hk != host.hotkey_reg {
        // SAFETY: Registering/unregistering Ctrl+Alt+Y.
        unsafe {
            if hk {
                host.hotkey_reg =
                    RegisterHotKey(Some(host.hwnd), HOTKEY_TOGGLE, mods, b'Y' as u32).is_ok();
            } else {
                let _ = UnregisterHotKey(Some(host.hwnd), HOTKEY_TOGGLE);
                host.hotkey_reg = false;
            }
        }
    }
    if jhk != host.jump_reg {
        // SAFETY: Registering/unregistering Ctrl+Alt+J.
        unsafe {
            if jhk {
                host.jump_reg =
                    RegisterHotKey(Some(host.hwnd), HOTKEY_JUMP, mods, b'J' as u32).is_ok();
            } else {
                let _ = UnregisterHotKey(Some(host.hwnd), HOTKEY_JUMP);
                host.jump_reg = false;
            }
        }
    }
    if mhk != host.mic_reg {
        // SAFETY: Registering/unregistering Ctrl+Alt+M.
        unsafe {
            if mhk {
                host.mic_reg =
                    RegisterHotKey(Some(host.hwnd), HOTKEY_MIC, mods, b'M' as u32).is_ok();
            } else {
                let _ = UnregisterHotKey(Some(host.hwnd), HOTKEY_MIC);
                host.mic_reg = false;
            }
        }
    }
    if !host.drop_registered {
        // SAFETY: Fallback file drop when OLE RegisterDragDrop is unavailable.
        unsafe {
            DragAcceptFiles(host.hwnd, true);
        }
    }
}

#[implement(IDropTarget)]
struct MochiDropTarget {
    hwnd: HWND,
}

impl MochiDropTarget {
    fn host(&self) -> Option<&'static mut WindowHost> {
        // SAFETY: `GWLP_USERDATA` holds a valid `WindowHost` pointer until `WM_DESTROY` revokes drop target.
        let ptr = unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) } as *mut WindowHost;
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *ptr })
        }
    }

    fn client_dip(&self, host: &WindowHost, pt: &POINTL) -> (f64, f64) {
        let mut p = POINT { x: pt.x, y: pt.y };
        // SAFETY: Converting screen coordinates to client coordinates.
        unsafe {
            let _ = ScreenToClient(self.hwnd, &raw mut p);
        }
        let scale = host.last_scale.max(0.5);
        (f64::from(p.x) / scale, f64::from(p.y) / scale)
    }
}

impl IDropTarget_Impl for MochiDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let Some(host) = self.host() else {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_NONE };
            }
            return Ok(());
        };
        let count = pdataobj.as_ref().and_then(extract_hdrop_count);
        if let Some(n) = count
            && n > 0
        {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_COPY };
            }
            let dip = self.client_dip(host, pt);
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.on_drag_enter(n, dip, Some(&host.factories));
            }
            render_and_present(host);
        } else {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_NONE };
            }
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.clear_drag_approach(Some(&host.factories));
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let Some(host) = self.host() else {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_NONE };
            }
            return Ok(());
        };
        if host.app.try_borrow().is_ok_and(|a| a.drop_hinting) {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_COPY };
            }
            let dip = self.client_dip(host, pt);
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.on_drag_over(dip);
            }
        } else if !pdweffect.is_null() {
            // SAFETY: Caller provides valid `pdweffect` pointer.
            unsafe { *pdweffect = DROPEFFECT_NONE };
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows_core::Result<()> {
        if let Some(host) = self.host() {
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.on_drag_leave(Some(&host.factories));
            }
            render_and_present(host);
        }
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> windows_core::Result<()> {
        let Some(host) = self.host() else {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_NONE };
            }
            return Ok(());
        };
        let paths = pdataobj
            .as_ref()
            .and_then(extract_hdrop_paths)
            .unwrap_or_default();
        if !paths.is_empty() {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_COPY };
            }
            let dip = self.client_dip(host, pt);
            let entries = NotchApp::prepare_shelf_entries(&paths, Some(&host.factories));
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.on_drag_drop_entries(entries, Some(dip), Some(&host.factories));
            }
            render_and_present(host);
        } else {
            if !pdweffect.is_null() {
                // SAFETY: Caller provides valid `pdweffect` pointer.
                unsafe { *pdweffect = DROPEFFECT_NONE };
            }
            if let Ok(mut app) = host.app.try_borrow_mut() {
                app.on_drag_leave(Some(&host.factories));
            }
            render_and_present(host);
        }
        Ok(())
    }
}

fn hdrop_format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

fn extract_hdrop_count(data_obj: &IDataObject) -> Option<usize> {
    let fmt = hdrop_format();
    // SAFETY: Querying whether `IDataObject` offers `CF_HDROP`.
    if unsafe { data_obj.QueryGetData(&fmt) }.is_err() {
        return None;
    }
    // SAFETY: Retrieving `CF_HDROP` storage medium to count dragged files.
    let mut medium = unsafe { data_obj.GetData(&fmt) }.ok()?;
    if medium.tymed != TYMED_HGLOBAL.0 as u32 {
        // SAFETY: Releasing unexpected `STGMEDIUM`.
        unsafe {
            ReleaseStgMedium(&raw mut medium);
        }
        return None;
    }
    // SAFETY: `medium.u.hGlobal` holds an `HDROP` handle for `TYMED_HGLOBAL`.
    let hdrop_ptr = unsafe { medium.u.hGlobal.0 };
    if hdrop_ptr.is_null() {
        // SAFETY: Releasing empty `STGMEDIUM`.
        unsafe {
            ReleaseStgMedium(&raw mut medium);
        }
        return None;
    }
    let hdrop = HDROP(hdrop_ptr);
    // SAFETY: Querying file count from `HDROP`.
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) } as usize;
    // SAFETY: Releasing `STGMEDIUM`.
    unsafe {
        ReleaseStgMedium(&raw mut medium);
    }
    Some(count)
}

fn extract_hdrop_paths(data_obj: &IDataObject) -> Option<Vec<String>> {
    let fmt = hdrop_format();
    // SAFETY: Retrieving `CF_HDROP` storage medium.
    let mut medium = unsafe { data_obj.GetData(&fmt) }.ok()?;
    if medium.tymed != TYMED_HGLOBAL.0 as u32 {
        // SAFETY: Releasing unexpected `STGMEDIUM`.
        unsafe {
            ReleaseStgMedium(&raw mut medium);
        }
        return None;
    }
    // SAFETY: `medium.u.hGlobal` holds an `HDROP` handle for `TYMED_HGLOBAL`.
    let hdrop_ptr = unsafe { medium.u.hGlobal.0 };
    if hdrop_ptr.is_null() {
        // SAFETY: Releasing empty `STGMEDIUM`.
        unsafe {
            ReleaseStgMedium(&raw mut medium);
        }
        return None;
    }
    let hdrop = HDROP(hdrop_ptr);
    // SAFETY: Querying file count from `HDROP`.
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        // SAFETY: Querying length of path `i`.
        let len = unsafe { DragQueryFileW(hdrop, i, None) };
        let mut buf = vec![0u16; len as usize + 1];
        // SAFETY: Reading path `i` into `buf`.
        unsafe {
            DragQueryFileW(hdrop, i, Some(&mut buf));
        }
        paths.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    // SAFETY: Releasing `STGMEDIUM`.
    unsafe {
        ReleaseStgMedium(&raw mut medium);
    }
    Some(paths)
}

fn is_ole_drag_active() -> bool {
    // SAFETY: Checking mouse button state and `SysDragImage` window visibility.
    unsafe {
        let btn_down =
            GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0;
        if !btn_down {
            return false;
        }
        let cls = to_wide("SysDragImage");
        let Ok(drag_hwnd) = FindWindowW(PCWSTR(cls.as_ptr()), PCWSTR::null()) else {
            return false;
        };
        !drag_hwnd.is_invalid() && IsWindowVisible(drag_hwnd).as_bool()
    }
}

fn sync_drag_approach(host: &mut WindowHost) {
    let approaching = host.app.try_borrow().is_ok_and(|a| a.drag_approaching);
    if !approaching {
        return;
    }
    let still_inside = if is_ole_drag_active() {
        if let Some((sx, sy)) = get_cursor_pos() {
            let mut pt = POINT { x: sx, y: sy };
            // SAFETY: Converting screen cursor position to client coordinates.
            unsafe {
                let _ = ScreenToClient(host.hwnd, &raw mut pt);
            }
            let scale = host.last_scale.max(0.5);
            let x = f64::from(pt.x) / scale;
            let y = f64::from(pt.y) / scale;
            let cx = host.app.try_borrow().map_or(322.0, |a| a.window_w * 0.5);
            (x - cx).abs() <= 290.0 && (-4.0..=125.0).contains(&y)
        } else {
            false
        }
    } else {
        false
    };
    if !still_inside && let Ok(mut app) = host.app.try_borrow_mut() {
        app.clear_drag_approach(Some(&host.factories));
    }
}

fn apply_capture_exclusion(host: &mut WindowHost) {
    let exclude = {
        let app = host.app.borrow();
        app.store.data.settings.hide_from_capture && !app.capturable
    };
    if host.excluded_capture != Some(exclude) && set_capture_exclusion(host.hwnd, exclude) {
        host.excluded_capture = Some(exclude);
    }
}

fn add_tray_icon(host: &mut WindowHost) {
    if host.tray_added {
        return;
    }
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: host.hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: host.tray_icon,
        ..Default::default()
    };
    let tip = to_wide("Mochi");
    let n = tip.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    // SAFETY: Adding tray icon via Shell_NotifyIconW.
    if unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool() {
        host.tray_added = true;
    }
}

fn remove_tray_icon(host: &mut WindowHost) {
    if host.tray_added {
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: host.hwnd,
            uID: 1,
            ..Default::default()
        };
        // SAFETY: Removing tray icon.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        host.tray_added = false;
    }
    if !host.tray_icon.is_invalid() {
        // SAFETY: Destroying loaded tray icon.
        unsafe {
            let _ = DestroyIcon(host.tray_icon);
        }
        host.tray_icon = HICON::default();
    }
}

fn show_tray_menu(host: &WindowHost) {
    let (s, hidden, demo_on) = {
        let app = host.app.borrow();
        (
            app.lang.strings(),
            app.hidden_by_user,
            app.simulated_idle_secs.is_some() || app.force_ambient,
        )
    };
    // SAFETY: Creating and showing Win32 popup menu at cursor position.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            CMD_OPEN,
            PCWSTR(to_wide(s.tray_open).as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING | if hidden { MF_CHECKED } else { MF_UNCHECKED },
            CMD_HIDE,
            PCWSTR(to_wide(s.tray_hide).as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            CMD_SETTINGS,
            PCWSTR(to_wide(s.tray_settings).as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            CMD_UPDATES,
            PCWSTR(to_wide(s.tray_check_updates).as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING | if demo_on { MF_CHECKED } else { MF_UNCHECKED },
            CMD_DEMO,
            PCWSTR(to_wide(s.tray_demo).as_ptr()),
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            CMD_QUIT,
            PCWSTR(to_wide(s.tray_quit).as_ptr()),
        );
        let mut pt = POINT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&raw mut pt);
        let _ = SetForegroundWindow(host.hwnd);
        let valign = if pt.y < host.cached_disp.y + 200 {
            TPM_TOPALIGN
        } else {
            TPM_BOTTOMALIGN
        };
        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | valign,
            pt.x,
            pt.y,
            Some(0),
            host.hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }
}

pub(crate) fn load_ico_from_bytes(ico: &[u8], target_size: u32) -> HICON {
    if ico.len() < 22 {
        return HICON::default();
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    if count == 0 || ico.len() < 6 + count * 16 {
        return HICON::default();
    }
    let mut best_idx = 0usize;
    let mut best_diff = u32::MAX;
    for i in 0..count {
        let entry = 6 + i * 16;
        let w = match ico[entry] {
            0 => 256,
            v => u32::from(v),
        };
        let diff = w.abs_diff(target_size);
        if diff < best_diff {
            best_diff = diff;
            best_idx = i;
        }
    }
    let entry = 6 + best_idx * 16;
    let size = u32::from_le_bytes([
        ico[entry + 8],
        ico[entry + 9],
        ico[entry + 10],
        ico[entry + 11],
    ]) as usize;
    let offset = u32::from_le_bytes([
        ico[entry + 12],
        ico[entry + 13],
        ico[entry + 14],
        ico[entry + 15],
    ]) as usize;
    if offset + size > ico.len() {
        return HICON::default();
    }
    let dim = target_size as i32;
    // SAFETY: Creating icon from embedded ICO resource bytes.
    unsafe {
        CreateIconFromResourceEx(
            &ico[offset..offset + size],
            true,
            0x0003_0000,
            dim,
            dim,
            LR_DEFAULTCOLOR,
        )
        .unwrap_or_default()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Vitrine {
    track: VitrineTrack,
    lrc: String,
    events: Vec<VitrineEvent>,
    activity: VitrineActivity,
    hud: VitrineHud,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VitrineTrack {
    title: String,
    artist: String,
    seconds: i64,
    position: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VitrineEvent {
    title: String,
    #[serde(rename = "In")]
    in_mins: i64,
    minutes: i64,
    meeting_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VitrineActivity {
    title: String,
    text: String,
    progress: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VitrineHud {
    text: String,
    value: String,
    level: f64,
}

fn run_vitrine_shots(app: &mut NotchApp, factories: &D2dFactories, dir: &Path) {
    let res = (|| -> Result<(), String> {
        let raw = std::fs::read_to_string(dir.join("vitrine.json")).map_err(|e| e.to_string())?;
        let vitrine: Vitrine =
            serde_json::from_str(raw.trim_start_matches('\u{FEFF}')).map_err(|e| e.to_string())?;
        let mut surf = D2dSurface::new(factories, 1600, 800, 2.0).map_err(|e| e.to_string())?;

        let mut snap_shot = |app: &mut NotchApp, name: &str| -> Result<(), String> {
            app.update_targets(Some(factories));
            app.snap_all_springs();
            app.fit_window(true);
            let pw = (app.window_w * 2.0).ceil() as i32;
            let ph = (app.window_h * 2.0).ceil() as i32;
            surf.ensure_size(pw, ph, 2.0);
            if surf.begin_draw() {
                render_notch(app, factories, &mut surf);
                surf.end_draw();
                surf.save_trimmed_png(&dir.join(format!("{name}.png")))?;
            }
            Ok(())
        };

        // 1. fechado-dormindo
        app.store.data.settings.companion_enabled = true;
        app.simulated_idle_secs = Some(1800.0);
        app.presence_state = 2;
        app.update_companion();
        snap_shot(app, "fechado-dormindo")?;

        app.store.data.settings.companion_enabled = false;
        app.simulated_idle_secs = None;
        app.presence_state = 0;
        app.update_companion();
        app.hide_news(Some(factories));

        // 2. fechado-relogio
        snap_shot(app, "fechado-relogio")?;

        // 3. fechado-reuniao & aberto-agenda
        let now = Local::now().naive_local();
        let events: Vec<CalendarEvent> = vitrine
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut start = now + chrono::Duration::minutes(e.in_mins);
                if e.in_mins.abs() > 30 {
                    let m = (start.minute() / 15) * 15;
                    start = start.date().and_hms_opt(start.hour(), m, 0).unwrap();
                }
                CalendarEvent {
                    id: format!("vitrine-{i}"),
                    title: e.title.clone(),
                    start,
                    end: start + chrono::Duration::minutes(e.minutes),
                    all_day: false,
                    meeting_url: e.meeting_url.clone(),
                    key: format!("vitrine-{i}"),
                }
            })
            .collect();
        app.calendar_events = events.clone();
        app.update_clock(Some(factories));
        snap_shot(app, "fechado-reuniao")?;

        app.show_page("agenda", 300, Some(factories));
        snap_shot(app, "aberto-agenda")?;
        app.set_expanded(false, Some(factories));

        // 4. fechado-musica & aberto-musica
        let cover_bytes = std::fs::read(dir.join("cover.png")).ok();
        let media = mochi_winrt::media::MediaState {
            title: vitrine.track.title.clone(),
            artist: vitrine.track.artist.clone(),
            source_app: String::new(),
            is_playing: true,
            position_secs: vitrine.track.position as f64,
            duration_secs: vitrine.track.seconds as f64,
            position_updated_at: Instant::now(),
            playback_rate: 1.0,
            thumbnail_bytes: cover_bytes,
            thumbnail_id: 1,
        };
        app.apply_media_snapshot(Some(media), None, Some(factories));
        app.set_lyrics(Lyrics::parse(&vitrine.lrc), Some(factories));
        app.snap_all_springs();
        snap_shot(app, "fechado-musica")?;

        app.show_page("media", 300, Some(factories));
        snap_shot(app, "aberto-musica")?;

        // 5. aberto-foco, aberto-agua, aberto-tarefas, aberto-prateleira
        app.focus.start();
        app.show_page("focus", 300, Some(factories));
        snap_shot(app, "aberto-foco")?;
        app.show_page("water", 300, Some(factories));
        snap_shot(app, "aberto-agua")?;
        app.show_page("tasks", 300, Some(factories));
        snap_shot(app, "aberto-tarefas")?;
        app.show_page("shelf", 300, Some(factories));
        snap_shot(app, "aberto-prateleira")?;

        app.win_notif_access = mochi_winrt::notifications::NotificationAccess::Allowed;
        app.push_notification(
            Some(101),
            "WhatsApp".to_string(),
            Glyphs::BELL.to_string(),
            "Alex".to_string(),
            "Meeting starts in 5 minutes".to_string(),
            now,
            false,
            true,
        );
        app.push_notification(
            None,
            "GitHub Copilot".to_string(),
            "mark:copilot".to_string(),
            "Visual Studio Code · mochi".to_string(),
            "Build succeeded".to_string(),
            now - chrono::Duration::minutes(3),
            false,
            false,
        );
        app.show_page("notifications", 300, Some(factories));
        snap_shot(app, "aberto-notificacoes")?;
        app.notifications.clear();
        app.seen_win_notif_ids.clear();

        // 5c. aberto-clipboard
        app.capture_clipboard_payload(
            101,
            ClipboardPayload::Text("cargo test --workspace --all-targets".to_string()),
            Some(factories),
        );
        app.toggle_pin_clipboard_item(0);
        app.capture_clipboard_payload(
            102,
            ClipboardPayload::Text(
                "fn render_notch(app: &mut NotchApp) {\n    // 100 Hz DWM vsync\n}".to_string(),
            ),
            Some(factories),
        );
        app.capture_clipboard_payload(
            103,
            ClipboardPayload::Files(vec![r"C:\src\Mochi\notes.md".to_string()]),
            Some(factories),
        );
        app.show_page("clipboard", 300, Some(factories));
        snap_shot(app, "aberto-clipboard")?;
        app.clipboard_items.clear();
        app.store.data.clipboard.clear();

        // 5d. aberto-weather
        app.store.data.settings.weather_enabled = true;
        app.store.data.settings.weather_place = Some("Jakarta, ID".to_string());
        app.store.data.settings.weather_latitude = Some(-6.2);
        app.store.data.settings.weather_longitude = Some(106.8);
        app.weather_report = Some(WeatherReport {
            temperature: 29.0,
            feels_like: Some(32.0),
            humidity: Some(74),
            wind_kmh: Some(12.0),
            uv_index: Some(6.0),
            code: 2,
            is_day: true,
            high: Some(32.0),
            low: Some(25.0),
            rain_at: None,
            rain_chance: 20,
            icon: "PartlyCloudy".to_string(),
            description: "Partly cloudy".to_string(),
            temperature_text: "29°".to_string(),
            hourly: vec![
                HourlyWeatherPoint {
                    label: "Now".to_string(),
                    temp: 29.0,
                    rain_chance: 10,
                    code: 2,
                    is_day: true,
                },
                HourlyWeatherPoint {
                    label: "15:00".to_string(),
                    temp: 31.0,
                    rain_chance: 20,
                    code: 2,
                    is_day: true,
                },
                HourlyWeatherPoint {
                    label: "18:00".to_string(),
                    temp: 28.0,
                    rain_chance: 65,
                    code: 61,
                    is_day: false,
                },
                HourlyWeatherPoint {
                    label: "21:00".to_string(),
                    temp: 26.0,
                    rain_chance: 40,
                    code: 61,
                    is_day: false,
                },
                HourlyWeatherPoint {
                    label: "00:00".to_string(),
                    temp: 25.0,
                    rain_chance: 15,
                    code: 1,
                    is_day: false,
                },
                HourlyWeatherPoint {
                    label: "03:00".to_string(),
                    temp: 24.0,
                    rain_chance: 10,
                    code: 0,
                    is_day: false,
                },
            ],
            fetched_at: now,
        });
        app.show_page("weather", 300, Some(factories));
        snap_shot(app, "aberto-weather")?;
        app.weather_report = None;
        app.store.data.settings.weather_enabled = false;
        app.store.data.settings.weather_place = None;

        // 5e. aberto-pray
        app.store.data.settings.pray_place = Some("Jakarta".to_string());
        app.store.data.settings.pray_latitude = Some(-6.2088);
        app.store.data.settings.pray_longitude = Some(106.8456);
        app.recompute_prayer_schedule();
        app.show_page("pray", 300, Some(factories));
        snap_shot(app, "aberto-pray")?;
        app.store.data.settings.pray_place = None;

        // 6. fechado-foco
        app.calendar_events = events
            .into_iter()
            .filter(|e| (e.start - now) > chrono::Duration::minutes(15) || e.start < now)
            .collect();
        app.update_clock(Some(factories));
        app.set_expanded(false, Some(factories));
        snap_shot(app, "fechado-foco")?;

        // 7. fechado-volume
        let mut hud_msg = HudMessage::new(Glyphs::VOLUME_2, vitrine.hud.text);
        hud_msg.value = Some(vitrine.hud.value);
        hud_msg.level = Some(vitrine.hud.level);
        hud_msg.duration_ms = 60_000;
        app.show_hud(hud_msg, Some(factories));
        snap_shot(app, "fechado-volume")?;
        app.hide_hud(Some(factories));

        // 8. fechado-atividade & aberto-atividade
        let settings = app.store.data.settings.clone();
        app.focus.stop(&settings);
        let act_json = serde_json::json!({
            "op": "set",
            "id": "vitrine",
            "title": vitrine.activity.title,
            "text": vitrine.activity.text,
            "progress": vitrine.activity.progress,
        })
        .to_string();
        app.handle_activity(&act_json, Some(factories));
        app.hide_hud(Some(factories));
        snap_shot(app, "fechado-atividade")?;

        app.show_page("media", 300, Some(factories));
        snap_shot(app, "aberto-atividade")?;

        // 9. fechado-agente-aviso & fechado-agente & aberto-copilot
        app.set_expanded(false, Some(factories));
        app.handle_activity(r#"{"op":"end","id":"vitrine"}"#, Some(factories));
        app.handle_activity(
            r#"{"op":"set","id":"vitrine-copilot","title":"Visual Studio Code · mochi","text":"Using run_in_terminal","icon":"copilot","state":"running","phrase":"working","window":{"handle":1,"pid":1}}"#,
            Some(factories),
        );
        app.copilot_hooks_on = true;
        app.agent_pulse_phase = 1.1;
        app.hide_hud(Some(factories));
        snap_shot(app, "fechado-copilot")?;
        app.show_page("media", 300, Some(factories));
        snap_shot(app, "aberto-copilot")?;
        app.set_expanded(false, Some(factories));
        app.handle_activity(r#"{"op":"end","id":"vitrine-copilot"}"#, Some(factories));
        app.handle_activity(
            r#"{"op":"set","id":"vitrine-agent","title":"Windows Terminal · mochi","icon":"claude","state":"done","phrase":"done"}"#,
            Some(factories),
        );
        snap_shot(app, "fechado-agente-aviso")?;
        app.hide_hud(Some(factories));
        snap_shot(app, "fechado-agente")?;

        // 10. fechado-fora
        app.handle_activity(r#"{"op":"end","id":"vitrine-agent"}"#, Some(factories));
        let s = app.lang.strings();
        let report = AwayReport {
            duration: Duration::from_secs(42 * 60),
            items: vec![
                AwayItem {
                    glyph: Glyphs::BELL.to_string(),
                    text: s.demo_meeting.to_string(),
                    at: now - chrono::Duration::minutes(35),
                },
                AwayItem {
                    glyph: "mark:claude".to_string(),
                    text: format!("Windows Terminal · mochi · {}", s.activity_done),
                    at: now - chrono::Duration::minutes(21),
                },
                AwayItem {
                    glyph: Glyphs::WATER.to_string(),
                    text: format!("{} · 750/2000 ml", s.water_reminder),
                    at: now - chrono::Duration::minutes(8),
                },
            ],
        };
        app.show_news(report, Some(factories));
        app.news_flight_t = 1.0;
        app.news_raven.land();
        snap_shot(app, "fechado-fora")?;

        // 11. Drag-to-notch approach & active drop zone
        app.hide_news(Some(factories));
        app.set_expanded(false, Some(factories));
        app.on_drag_approach((340.0, 52.0), 0.9, Some(factories));
        app.snap_all_springs();
        app.drag_approach = 0.9;
        app.drag_phase = 0.8;
        snap_shot(app, "fechado-drag-aproximando")?;

        app.on_drag_enter(3, (420.0, 110.0), Some(factories));
        app.snap_all_springs();
        app.drag_anim = 1.0;
        app.drag_phase = 1.2;
        snap_shot(app, "aberto-prateleira-drop")?;

        let tmp_drop = std::env::temp_dir().join("mochi-vitrine-drop.txt");
        let _ = std::fs::write(&tmp_drop, b"mochi");
        app.on_drag_drop(
            &[tmp_drop.to_string_lossy().into_owned()],
            Some((420.0, 110.0)),
            Some(factories),
        );
        let _ = std::fs::remove_file(&tmp_drop);
        app.snap_all_springs();
        app.drop_burst_t = 0.28;
        app.shelf_flash_t = 0.72;
        app.drop_celebrate_count = 3;
        snap_shot(app, "aberto-prateleira-drop-burst")?;
        app.clear_shelf();
        app.undo = None;
        app.drop_burst_t = 1.0;
        app.shelf_flash_t = 0.0;
        Ok(())
    })();

    let done_txt = match res {
        Ok(()) => "ok".to_string(),
        Err(e) => format!("erro: {e}"),
    };
    let _ = std::fs::write(dir.join("done.txt"), done_txt);
}
