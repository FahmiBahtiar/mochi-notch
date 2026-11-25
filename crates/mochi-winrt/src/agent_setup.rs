use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, RegCloseKey, RegCreateKeyExW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows_core::PCWSTR;

use crate::native::to_wide;
use crate::paths::{AppPaths, SettingsData, atomic_write};

pub const EMBEDDED_SKILL: &str = mochi_activities::SKILL_MD;

const SKILL_NAME: &str = "mochi-notch";
const AGENTS: [&str; 4] = ["copilot", "claude", "cursor", "codex"];

const COPILOT_EVENTS: [(&str, u64); 9] = [
    ("SessionStart", 5),
    ("UserPromptSubmit", 5),
    ("PreToolUse", 5),
    ("PostToolUse", 5),
    ("SubagentStart", 5),
    ("SubagentStop", 5),
    ("Stop", 5),
    ("ErrorOccurred", 5),
    ("SessionEnd", 2),
];

const CLAUDE_EVENTS: [(&str, u64); 8] = [
    ("UserPromptSubmit", 5),
    ("PostToolUse", 5),
    ("PostToolUseFailure", 5),
    ("PermissionRequest", 5),
    ("Notification", 5),
    ("Stop", 5),
    ("StopFailure", 5),
    ("SessionEnd", 1),
];

const CURSOR_EVENTS: [(&str, u64); 6] = [
    ("beforeSubmitPrompt", 5),
    ("postToolUse", 5),
    ("afterShellExecution", 5),
    ("afterFileEdit", 5),
    ("stop", 5),
    ("sessionEnd", 5),
];

const CODEX_EVENTS: [(&str, u64); 6] = [
    ("UserPromptSubmit", 5),
    ("PostToolUse", 5),
    ("PermissionRequest", 5),
    ("Stop", 5),
    ("Interrupt", 2),
    ("SessionEnd", 2),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    NotFound,
    Off,
    On,
    OnByPlugin,
}

pub struct AgentSetup {
    home: PathBuf,
    cli: PathBuf,
    skill: String,
}

impl AgentSetup {
    pub fn new(home: PathBuf, cli: PathBuf, skill: String) -> Self {
        Self { home, cli, skill }
    }

    pub fn installed_cli() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("mochi-cli.exe")))
            .unwrap_or_else(|| PathBuf::from("mochi-cli.exe"))
    }

    pub fn cli_dir() -> PathBuf {
        Self::installed_cli()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    pub fn has_cli() -> bool {
        Self::installed_cli().exists()
    }

    pub fn for_this_user() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default();
        Self::new(home, Self::installed_cli(), EMBEDDED_SKILL.to_string())
    }

    pub fn remove_all() {
        let setup = Self::for_this_user();
        for agent in AGENTS {
            if setup.status(agent) == AgentStatus::On {
                let _ = setup.set(agent, false);
            }
        }
    }

    pub fn upgrade_copilot_if_needed(&self) {
        if !Self::has_cli() {
            return;
        }
        let file = self.copilot_file();
        let expected = format!(
            "& '{}' hook copilot",
            self.cli.to_string_lossy().replace('\'', "''")
        );
        if let Ok(text) = fs::read_to_string(&file)
            && text.contains("hook copilot")
            && !text.contains(&expected)
        {
            let _ = self.set("copilot", true);
        }
    }

    fn copilot_dir(&self) -> PathBuf {
        self.home.join(".copilot")
    }
    fn copilot_file(&self) -> PathBuf {
        self.copilot_dir().join("hooks").join("mochi.json")
    }
    fn claude_dir(&self) -> PathBuf {
        self.home.join(".claude")
    }
    fn claude_file(&self) -> PathBuf {
        self.claude_dir().join("settings.json")
    }
    fn cursor_dir(&self) -> PathBuf {
        self.home.join(".cursor")
    }
    fn cursor_file(&self) -> PathBuf {
        self.cursor_dir().join("hooks.json")
    }
    fn codex_dir(&self) -> PathBuf {
        self.home.join(".codex")
    }
    fn codex_file(&self) -> PathBuf {
        self.codex_dir().join("hooks.json")
    }

    pub fn status(&self, agent: &str) -> AgentStatus {
        match agent {
            "copilot" => {
                let has_vscode = std::env::var_os("APPDATA")
                    .is_some_and(|a| PathBuf::from(a).join("Code").exists());
                if !self.copilot_dir().exists() && !has_vscode {
                    return AgentStatus::NotFound;
                }
                let Ok(root) = read_json_obj(&self.copilot_file()) else {
                    return AgentStatus::Off;
                };
                if has_ours(&root, "copilot", false) {
                    AgentStatus::On
                } else {
                    AgentStatus::Off
                }
            }
            "claude" => {
                if !self.claude_dir().exists() {
                    return AgentStatus::NotFound;
                }
                let Ok(root) = read_json_obj(&self.claude_file()) else {
                    return AgentStatus::Off;
                };
                if has_plugin(&root) {
                    AgentStatus::OnByPlugin
                } else if has_ours(&root, "claude", true) {
                    AgentStatus::On
                } else {
                    AgentStatus::Off
                }
            }
            "cursor" => {
                if !self.cursor_dir().exists() {
                    return AgentStatus::NotFound;
                }
                let Ok(root) = read_json_obj(&self.cursor_file()) else {
                    return AgentStatus::Off;
                };
                if has_ours(&root, "cursor", false) {
                    AgentStatus::On
                } else {
                    AgentStatus::Off
                }
            }
            "codex" => {
                if !self.codex_dir().exists() {
                    return AgentStatus::NotFound;
                }
                let Ok(root) = read_json_obj(&self.codex_file()) else {
                    return AgentStatus::Off;
                };
                if has_ours(&root, "codex", true) {
                    AgentStatus::On
                } else {
                    AgentStatus::Off
                }
            }
            _ => AgentStatus::NotFound,
        }
    }

    pub fn set(&self, agent: &str, on: bool) -> Result<(), String> {
        let cli_str = self.cli.to_string_lossy().into_owned();
        match agent {
            "copilot" => {
                let file = self.copilot_file();
                update_json_obj(&file, |root| {
                    set_copilot_flat(root, &COPILOT_EVENTS, on, &cli_str);
                })?;
                if !on
                    && let Ok(root) = read_json_obj(&file)
                    && root.is_empty()
                {
                    let _ = fs::remove_file(&file);
                }
            }
            "claude" => update_json_obj(&self.claude_file(), |root| {
                set_grouped(root, "claude", &CLAUDE_EVENTS, on, |timeout| {
                    json!({
                        "type": "command",
                        "command": "mochi-cli hook claude",
                        "timeout": timeout,
                    })
                });
            })?,
            "cursor" => update_json_obj(&self.cursor_file(), |root| {
                if !root.contains_key("version") {
                    root.insert("version".to_string(), json!(1));
                }
                set_flat(root, "cursor", &CURSOR_EVENTS, on, &cli_str);
            })?,
            "codex" => update_json_obj(&self.codex_file(), |root| {
                set_grouped(root, "codex", &CODEX_EVENTS, on, |timeout| {
                    json!({
                        "type": "command",
                        "command": "mochi-cli hook codex",
                        "timeout": timeout,
                    })
                });
            })?,
            _ => return Err(format!("Unknown agent {agent}")),
        }
        self.sync_skills();
        Ok(())
    }

    fn sync_skills(&self) {
        let copilot_on = self.status("copilot") == AgentStatus::On;
        let claude_on = self.status("claude") == AgentStatus::On;
        let cursor_or_codex =
            self.status("cursor") == AgentStatus::On || self.status("codex") == AgentStatus::On;
        self.set_skill(
            &self.home.join(".copilot").join("skills").join(SKILL_NAME),
            copilot_on,
        );
        self.set_skill(
            &self.home.join(".claude").join("skills").join(SKILL_NAME),
            claude_on,
        );
        self.set_skill(
            &self.home.join(".agents").join("skills").join(SKILL_NAME),
            cursor_or_codex,
        );
    }

    fn set_skill(&self, dir: &Path, on: bool) {
        let file = dir.join("SKILL.md");
        if on {
            let _ = fs::create_dir_all(dir);
            let _ = fs::write(&file, self.skill.as_bytes());
        } else if file.exists() {
            let _ = fs::remove_file(&file);
            if fs::read_dir(dir).is_ok_and(|mut it| it.next().is_none()) {
                let _ = fs::remove_dir(dir);
            }
        }
    }
}

fn read_json_obj(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let trimmed = text.trim_start_matches('\u{FEFF}').trim();
    if trimmed.is_empty() {
        return Ok(Map::new());
    }
    let val: Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    val.as_object()
        .cloned()
        .ok_or_else(|| format!("{} is not a JSON object", path.display()))
}

fn update_json_obj(
    path: &Path,
    change: impl FnOnce(&mut Map<String, Value>),
) -> Result<(), String> {
    let mut root = read_json_obj(path)?;
    let before = serde_json::to_string(&root).unwrap_or_default();
    change(&mut root);
    let after = serde_json::to_string(&root).unwrap_or_default();
    if before == after {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if path.exists() {
        let mut backup = path.as_os_str().to_os_string();
        backup.push(".mochi-backup");
        let _ = fs::copy(path, PathBuf::from(backup));
    }
    let pretty = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    atomic_write(path, pretty.as_bytes()).map_err(|e| e.to_string())
}

fn has_plugin(root: &Map<String, Value>) -> bool {
    root.get("enabledPlugins")
        .and_then(Value::as_object)
        .and_then(|p| p.get("mochi@mochi"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn is_ours(handler: &Value, agent: &str) -> bool {
    let Some(obj) = handler.as_object() else {
        return false;
    };
    let cmd = obj.get("command").and_then(Value::as_str).unwrap_or("");
    if !cmd.to_ascii_lowercase().contains("mochi-cli") {
        return false;
    }
    if let Some(args) = obj.get("args").and_then(Value::as_array) {
        return args.len() >= 2
            && args[0].as_str() == Some("hook")
            && args[1].as_str() == Some(agent);
    }
    cmd.ends_with(&format!("hook {agent}"))
}

fn has_ours(root: &Map<String, Value>, agent: &str, grouped: bool) -> bool {
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    hooks.values().any(|list| {
        let Some(arr) = list.as_array() else {
            return false;
        };
        if grouped {
            arr.iter().any(|g| {
                g.get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|hs| hs.iter().any(|h| is_ours(h, agent)))
            })
        } else {
            arr.iter().any(|h| is_ours(h, agent))
        }
    })
}

fn set_grouped(
    root: &mut Map<String, Value>,
    agent: &str,
    events: &[(&str, u64)],
    on: bool,
    handler: impl Fn(u64) -> Value,
) {
    if let Some(Value::Object(hooks)) = root.get_mut("hooks") {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for name in keys {
            if let Some(Value::Array(groups)) = hooks.get_mut(&name) {
                for g in groups.iter_mut() {
                    if let Some(Value::Array(hs)) = g.get_mut("hooks") {
                        hs.retain(|h| !is_ours(h, agent));
                    }
                }
                groups.retain(|g| {
                    g.get("hooks")
                        .and_then(Value::as_array)
                        .is_none_or(|hs| !hs.is_empty())
                });
            }
            if hooks
                .get(&name)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                hooks.remove(&name);
            }
        }
        if hooks.is_empty() && !on {
            root.remove("hooks");
        }
    }
    if !on {
        return;
    }
    let target = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks_obj) = target.as_object_mut() else {
        return;
    };
    for &(name, timeout) in events {
        let groups = hooks_obj
            .entry(name.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = groups.as_array_mut() {
            arr.push(json!({ "hooks": [handler(timeout)] }));
        }
    }
}

fn set_flat(
    root: &mut Map<String, Value>,
    agent: &str,
    events: &[(&str, u64)],
    on: bool,
    cli: &str,
) {
    if let Some(Value::Object(hooks)) = root.get_mut("hooks") {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for name in keys {
            if let Some(Value::Array(handlers)) = hooks.get_mut(&name) {
                handlers.retain(|h| !is_ours(h, agent));
            }
            if hooks
                .get(&name)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                hooks.remove(&name);
            }
        }
    }
    if !on {
        return;
    }
    let target = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks_obj) = target.as_object_mut() else {
        return;
    };
    let command = format!("& '{}' hook cursor", cli.replace('\'', "''"));
    for &(name, timeout) in events {
        let handlers = hooks_obj
            .entry(name.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = handlers.as_array_mut() {
            arr.push(json!({ "command": command, "timeout": timeout }));
        }
    }
}

fn set_copilot_flat(root: &mut Map<String, Value>, events: &[(&str, u64)], on: bool, cli: &str) {
    if let Some(Value::Object(hooks)) = root.get_mut("hooks") {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for name in keys {
            if let Some(Value::Array(handlers)) = hooks.get_mut(&name) {
                handlers.retain(|h| !is_ours(h, "copilot"));
            }
            if hooks
                .get(&name)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                hooks.remove(&name);
            }
        }
        if hooks.is_empty() && !on {
            root.remove("hooks");
        }
    }
    if !on {
        return;
    }
    let target = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks_obj) = target.as_object_mut() else {
        return;
    };
    let command = format!("& '{}' hook copilot", cli.replace('\'', "''"));
    for &(name, timeout) in events {
        let handlers = hooks_obj
            .entry(name.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = handlers.as_array_mut() {
            arr.push(json!({
                "type": "command",
                "command": command,
                "timeout": timeout,
            }));
        }
    }
}

pub struct UserPath;

impl UserPath {
    const KEY: &str = "Environment";

    fn entries() -> Vec<String> {
        let sub = to_wide(Self::KEY);
        let val = to_wide("Path");
        let mut hkey = Default::default();
        // SAFETY: `sub` is NUL-terminated UTF-16; `hkey` is closed before returning.
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                Some(0),
                KEY_READ,
                &raw mut hkey,
            )
        }
        .is_err()
        {
            return Vec::new();
        }
        let mut bytes = 0u32;
        // SAFETY: Querying required buffer size.
        let _ = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(val.as_ptr()),
                None,
                None,
                None,
                Some(&raw mut bytes),
            )
        };
        let mut out = Vec::new();
        if bytes > 0 {
            let mut buf = vec![0u16; (bytes as usize / 2) + 1];
            // SAFETY: `buf` has `bytes` capacity.
            if unsafe {
                RegQueryValueExW(
                    hkey,
                    PCWSTR(val.as_ptr()),
                    None,
                    None,
                    Some(buf.as_mut_ptr().cast()),
                    Some(&raw mut bytes),
                )
            }
            .is_ok()
            {
                let len = (bytes as usize / 2).min(buf.len());
                let s = String::from_utf16_lossy(&buf[..len])
                    .trim_end_matches('\0')
                    .to_string();
                out = s
                    .split(';')
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect();
            }
        }
        // SAFETY: `hkey` is valid.
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        out
    }

    fn same(a: &str, b: &str) -> bool {
        a.trim_end_matches('\\')
            .eq_ignore_ascii_case(b.trim_end_matches('\\'))
    }

    pub fn contains(dir: &Path) -> bool {
        let d = dir.to_string_lossy();
        Self::entries().iter().any(|e| Self::same(e, &d))
    }

    pub fn set(dir: &Path, on: bool) -> bool {
        let d = dir.to_string_lossy().into_owned();
        let mut entries: Vec<String> = Self::entries()
            .into_iter()
            .filter(|e| !Self::same(e, &d))
            .collect();
        if on {
            entries.push(d);
        }
        let joined = entries.join(";");
        let sub = to_wide(Self::KEY);
        let val = to_wide("Path");
        let mut hkey = Default::default();
        // SAFETY: `sub` is NUL-terminated UTF-16; `hkey` is closed before returning.
        if unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                Some(0),
                None,
                Default::default(),
                KEY_WRITE,
                None,
                &raw mut hkey,
                None,
            )
        }
        .is_err()
        {
            return false;
        }
        let wide = to_wide(&joined);
        // SAFETY: `wide` is NUL-terminated UTF-16 and `wide.len() * 2` is its byte length.
        let bytes =
            unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
        // SAFETY: `hkey` is valid.
        let ok = unsafe {
            RegSetValueExW(
                hkey,
                PCWSTR(val.as_ptr()),
                Some(0),
                REG_EXPAND_SZ,
                Some(bytes),
            )
        }
        .is_ok();
        // SAFETY: `hkey` is valid.
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        if ok {
            let env = to_wide("Environment");
            // SAFETY: Broadcasting WM_SETTINGCHANGE for Environment.
            unsafe {
                let _ = SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SETTINGCHANGE,
                    WPARAM(0),
                    LPARAM(env.as_ptr() as isize),
                    SMTO_ABORTIFHUNG,
                    3000,
                    None,
                );
            }
        }
        ok
    }
}

pub fn copy_text_to_clipboard(text: &str) -> bool {
    let wide = to_wide(text);
    let bytes = wide.len() * 2;
    // SAFETY: Standard Win32 clipboard sequence with CF_UNICODETEXT (13).
    unsafe {
        if OpenClipboard(Some(HWND::default())).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let Ok(hmem): Result<HGLOBAL, _> = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
            let _ = CloseClipboard();
            return false;
        };
        let ptr = GlobalLock(hmem);
        if ptr.is_null() {
            let _ = GlobalFree(Some(hmem));
            let _ = CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), ptr.cast::<u8>(), bytes);
        let _ = GlobalUnlock(hmem);
        let ok = SetClipboardData(13, Some(HANDLE(hmem.0))).is_ok();
        if !ok {
            let _ = GlobalFree(Some(hmem));
        }
        let _ = CloseClipboard();
        ok
    }
}

pub fn diagnostics_summary(settings: &SettingsData, google_status: &str) -> String {
    format!(
        "Mochi {} (Rust)\nOS: Windows\nDataDir: {}\nLanguage: {}\nGoogle: {}\nWeather: {}\nActivities: {}\n",
        env!("CARGO_PKG_VERSION"),
        AppPaths::data_dir().display(),
        settings.language.as_deref().unwrap_or("auto"),
        google_status,
        settings.weather_enabled,
        settings.activities_enabled,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_setup_installs_and_removes_hooks_and_skills() {
        let dir = std::env::temp_dir().join(format!("mochi-agent-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".copilot")).unwrap();
        fs::create_dir_all(dir.join(".claude")).unwrap();
        fs::create_dir_all(dir.join(".cursor")).unwrap();
        fs::create_dir_all(dir.join(".codex")).unwrap();

        let setup = AgentSetup::new(
            dir.clone(),
            PathBuf::from(r"C:\Program Files\Mochi\mochi-cli.exe"),
            EMBEDDED_SKILL.to_string(),
        );
        assert_eq!(setup.status("copilot"), AgentStatus::Off);
        assert_eq!(setup.status("claude"), AgentStatus::Off);
        assert_eq!(setup.status("cursor"), AgentStatus::Off);
        assert_eq!(setup.status("codex"), AgentStatus::Off);

        setup.set("copilot", true).unwrap();
        setup.set("claude", true).unwrap();
        setup.set("cursor", true).unwrap();
        setup.set("codex", true).unwrap();
        let claude_json = fs::read_to_string(dir.join(".claude").join("settings.json")).unwrap();
        assert!(claude_json.contains("\"command\": \"mochi-cli hook claude\""));
        assert!(!claude_json.contains("\"args\""));
        assert_eq!(setup.status("copilot"), AgentStatus::On);
        assert_eq!(setup.status("claude"), AgentStatus::On);
        assert_eq!(setup.status("cursor"), AgentStatus::On);
        assert_eq!(setup.status("codex"), AgentStatus::On);
        assert!(
            dir.join(".copilot")
                .join("skills")
                .join("mochi-notch")
                .join("SKILL.md")
                .exists()
        );
        assert!(
            dir.join(".claude")
                .join("skills")
                .join("mochi-notch")
                .join("SKILL.md")
                .exists()
        );
        assert!(
            dir.join(".agents")
                .join("skills")
                .join("mochi-notch")
                .join("SKILL.md")
                .exists()
        );

        setup.set("copilot", false).unwrap();
        setup.set("claude", false).unwrap();
        setup.set("cursor", false).unwrap();
        setup.set("codex", false).unwrap();
        assert_eq!(setup.status("copilot"), AgentStatus::Off);
        assert_eq!(setup.status("claude"), AgentStatus::Off);
        assert_eq!(setup.status("cursor"), AgentStatus::Off);
        assert_eq!(setup.status("codex"), AgentStatus::Off);
        assert!(
            !dir.join(".copilot")
                .join("skills")
                .join("mochi-notch")
                .join("SKILL.md")
                .exists()
        );
        assert!(
            !dir.join(".claude")
                .join("skills")
                .join("mochi-notch")
                .join("SKILL.md")
                .exists()
        );
        assert!(
            !dir.join(".agents")
                .join("skills")
                .join("mochi-notch")
                .join("SKILL.md")
                .exists()
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
