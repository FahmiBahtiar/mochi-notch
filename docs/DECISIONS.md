# Architecture Decision Records — Mochi

## ADR-001: Dual-Binary Workspace Architecture
Status: accepted
Context: Mochi consists of two Windows executables backed by modular pure-Rust domain crates and a Win32/WinRT platform layer:
- `apps/mochi-desktop` (`Mochi.exe`): borderless transparent topmost Windows notch overlay with 100 Hz spring animations, WinRT SMTC media, Core Audio, WMI, Google OAuth, and 10 interactive tabs (`Media`, `Agenda`, `Focus`, `Water`, `Pray`, `Tasks`, `Shelf`, `Notifications`, `Clipboard`, `Weather`).
- `apps/mochi-cli` (`mochi-cli.exe`): console CLI for scripts and AI agent hooks (`Claude Code`, `Cursor`, `Codex`, `Copilot`).
Decision:
1. Keep pure business logic, math, parsing, and animation state machines in dedicated per-feature `#![forbid(unsafe_code)]` crates under `crates/mochi-{activities,calendar,clipboard,focus,gaze,lyrics,media,notifications,prayer,presence,shelf,spring,store,tasks,updates,water,weather}`.
2. Isolate Win32/WinRT/COM/CoreAudio/DPAPI/WinHTTP bindings into per-feature modules in `crates/mochi-winrt`.
3. Implement the Direct2D/DirectWrite notch overlay and Settings window in `crates/mochi-ui`, with per-feature UI state and rendering organized under `crates/mochi-ui/src/features/<feature>/`.
4. Verify `mochi-cli` end-to-end with `trycmd` fixtures in `apps/mochi-cli/tests/cases/`.
Consequences: Clean separation between safe domain logic, Windows platform integration, and GPU-accelerated Direct2D rendering.

## ADR-002: Native Win32 + Direct2D + DirectWrite UI Renderer
Status: accepted
Context: Mochi requires subtree opacity cross-fades (`CompactView`/`ExpandedView`/`HudView`/`NewsView`/`AmbientView`), arbitrary Bezier path clipping (`build_notch_geometry` concave ears), absolute-coordinate `LinearGradientBrush` opacity masks (`LyricsPanel`, `MarqueeText`), dynamic click-through (`WS_EX_TRANSPARENT` toggled outside interactive hit targets + `WS_EX_LAYERED`), `WS_EX_NOACTIVATE` focus-free interaction with dynamic keyboard mode for Tasks, and 100 Hz DWM-vsync-locked rendering without DXGI swapchain resize stutter.
Decision: Implement `mochi-ui` directly on Win32 + Direct2D + DirectWrite + `UpdateLayeredWindow` via `windows-rs`, locked to `DwmFlush()` vsync, with cached 9-slice `SoftShadow` bitmaps and `ID2D1Layer` geometric/gradient masks.
Consequences: Smooth 100 Hz animations, zero swapchain resize jank, native `Segoe UI Variable` + `Segoe Fluent Icons` rendering, native drag-and-drop, and low idle CPU/memory usage.

## ADR-003: On-Disk, Wire, and IPC Compatibility
Status: accepted
Context: Mochi stores user state locally and communicates with `mochi-cli` and AI agent hooks over a local named pipe, while migrating existing data from legacy `Yata` / `Notch` directories seamlessly.
Decision:
- Data directory: `%LOCALAPPDATA%\Mochi\` (automatically migrates `%LOCALAPPDATA%\Yata\` or `%LOCALAPPDATA%\Notch\` on first run if `%LOCALAPPDATA%\Mochi\` does not yet exist; override via `MOCHI_DATA_DIR`).
- Store file: `%LOCALAPPDATA%\Mochi\data.json` (`StoreData` schema v1, PascalCase JSON keys, clamping rules).
- OAuth token store: `%LOCALAPPDATA%\Mochi\google.dat` (DPAPI with entropy `b"Mochi.SecretFile.v1"`, with automatic fallback decryption from legacy `b"Yata.SecretFile.v1"`).
- Clipboard history store: `%LOCALAPPDATA%\Mochi\clipboard.dat` (DPAPI-encrypted via `SecretFile`, automatically migrating any legacy plaintext entries out of `data.json`).
- Lyrics cache: `%LOCALAPPDATA%\Mochi\lyrics\*.lrc` (SHA-1 hex cache keys of `"{artist}\n{title}".ToLowerInvariant()`).
- Named pipe: `\\.\pipe\Mochi.{UserSID}.{SessionID}` (UTF-8 newline-terminated JSON request in, plain text `"ok\n"` or error line out; fail-closed user SID allow + NetworkSid deny ACL + `PIPE_REJECT_REMOTE_CLIENTS`).
- Single-instance mutex: `Mochi.SingleInstance`.
- `mochi-cli` exit codes: `0` ok, `1` bad args, `2` Mochi not running, `3` refused.
- Companion visuals: procedural Direct2D Cozmo eyes & visor (`crates/mochi-ui/src/features/companion/render.rs`).
Consequences: Seamless upgrade path for existing users and reliable local IPC for agent hooks.

## ADR-004: Quality & Performance Gate
Status: accepted
Context: Every change must keep the workspace clean, warning-free, and responsive at 100 Hz.
Decision:
- `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass with zero warnings.
- `cargo test --workspace --all-targets` (including all 317 `trycmd` CLI tests and Direct2D offscreen render tests) must pass.
- `--bench transicoes --frames` and `--bench vitrine --shots` verify animation pacing and visual output across all notch states.
Consequences: Regressions in formatting, lints, CLI behavior, or rendering are caught automatically.
