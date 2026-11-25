# Glossary — Mochi Workspace

| Term | Meaning | Crate / Symbol |
| --- | --- | --- |
| `Spring` | Damped spring physics (`stiffness`, `damping_ratio`, `Spring::step` at 1/240s substeps) | `mochi_spring::port::Spring` |
| `build_notch_geometry` | Concave Bezier ears (`EAR = 8`, `ear_h = 6`) + rounded bottom corners (`radius`) | `mochi_ui::d2d::D2dFactories::build_notch_geometry` |
| `SoftShadow` | 9-slice pre-blurred rounded rectangle shadow cached by `(pad, opacity)` | `mochi_ui::d2d::D2dSurface::draw_soft_shadow` |
| `NotchPage` | Open notch tabs: `Media`, `Agenda`, `Focus`, `Water`, `Pray`, `Tasks`, `Shelf`, `Notifications`, `Clipboard`, `Weather` | `mochi_ui::state::NotchPage` |
| `Activity` / `ActivityRequest` | In-memory external program activities from `mochi-cli` over named pipe | `mochi_activities` / `mochi_ui::features::activities` |
| `ActivityPipe` / `ActivityClient` | Named pipe `\\.\pipe\Mochi.{SID}.{SessionId}` UTF-8 line IPC | `mochi_winrt::pipe::ActivityPipe` |
| `AgentHook` | Translates Claude Code / Cursor / Codex / Copilot hook JSON into activity requests | `mochi_activities::port::translate_agent_hook` |
| `FocusTimer` / `FocusLog` | Pomodoro state machine (`Ready`/`Running`/`Paused`, `Focus`/`ShortBreak`/`LongBreak`) + daily log | `mochi_focus` / `mochi_ui::features::focus` |
| `WaterTracker` | Daily water intake, +cup, undo, hourly reminders (08:00–22:00), 60-day retention | `mochi_water` / `mochi_ui::features::water` |
| `PrayerSchedule` | Astronomical prayer times, Hijri date, countdowns & pre-alerts | `mochi_prayer` / `mochi_ui::features::prayer` |
| `NotificationCenter` | Windows toast listener + Mochi reminder history & unread tracking | `mochi_notifications` / `mochi_ui::features::notifications` |
| `ClipboardHistory` | Clipboard text/files/image capture, pinning & live preview | `mochi_clipboard` / `mochi_ui::features::clipboard` |
| `WeatherReport` | Open-Meteo forecast, hourly curve, geocoding & rain alerts | `mochi_weather` / `mochi_winrt::weather` / `mochi_ui::features::weather` |
| `TaskList` / `TaskData` | To-do list with trailing time parser (`3pm`, `15:30`, `pukul 9`), reminders, Google Tasks sync | `mochi_tasks` / `mochi_winrt::tasks` / `mochi_ui::features::tasks` |
| `Shelf` | Path-only file shelf with thumbnails, availability check, drag-in/drag-out | `mochi_shelf` / `mochi_ui::features::shelf` |
| `Lyrics` / `LyricsService` | LRC parser, binary search `index_at`, LRCLIB lookup + SHA-1 disk cache | `mochi_lyrics` / `mochi_winrt::lyrics::LyricsService` |
| `CalendarEvents` / `CalendarReminders` | Google Calendar parser, SafeLinks unwrap, `next_within`, `due`, `until` | `mochi_calendar` / `mochi_winrt::calendar` / `mochi_ui::features::agenda` |
| `HudQueue` / `HudMessage` | Priority queue for transient notices vs important reminders | `mochi_ui::features::hud` / `mochi_winrt::paths::HudMessage` |
| `PresenceTracker` | `GetLastInputInfo` idle tracking, `Active`/`Idle`/`Asleep`, `AwayReport` | `mochi_presence` / `mochi_ui::state::AwayReport` |
| `EyeGazeController` | Cursor tracking, autonomous glances, single/double blink curves, sleep breathing | `mochi_gaze::port::EyeGazeController` |
| `CompanionAnimator` / `RoboVisor` | AI Robo companion animator + glowing visor eyes | `mochi_ui::features::companion` |
| `ColorExtractor` | Weighted HSL accent color extraction from album art | `mochi_media` / `mochi_ui::features::media` |
| `AppStore` / `StoreData` / `SettingsData` | `%LOCALAPPDATA%\Mochi\data.json` persistence + clamping normalization | `mochi_store` / `mochi_winrt::paths::AppStore` |
| `Updates` | GitHub release tag check & semantic version comparison | `mochi_updates` / `mochi_winrt::updates` / `mochi_ui::features::updates` |
