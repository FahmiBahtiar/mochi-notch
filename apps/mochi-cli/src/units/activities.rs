#[path = "win.rs"]
mod win;

use std::ffi::OsString;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::ArgMatches;
use serde_json::{Map, Value, json};

use win::{HostWindow, send_activity};

const SKILL_MD: &str = mochi_activities::SKILL_MD;

const USAGE: &str = "\
mochi-cli: the console side of Mochi, for scripts and AI agents.

  mochi-cli activity ...                            show a task in the notch (mochi-cli activity --help)
  mochi-cli hook copilot|claude|cursor|codex        called by an agent's hooks: its state in the notch
  mochi-cli hook setup copilot|claude|cursor|codex  print the hooks configuration to paste
  mochi-cli skill                                   print the agent skill (SKILL.md)
  mochi-cli skill install                           install it for Copilot, Claude Code, Cursor and Codex";

const ACTIVITY_USAGE: &str = "\
Usage:
  mochi-cli activity set <id> --title <text> [--text <text>] [--progress <0-1 or N%>] [--icon <name>] [--ttl <seconds>]
  mochi-cli activity attention <id> [same options]   needs you: warning color, stays until ended (up to 1 h)
  mochi-cli activity done <id> [same options]        finished: shows a check, then goes away
  mochi-cli activity fail <id> [same options]        failed: warning color, stays until ended (up to 1 h)
  mochi-cli activity end <id>                        remove it now (an id already gone is fine)
Send --title every time: a running activity nobody updates expires (10 min, or --ttl), and
an id that is gone needs a title to come back.
Icons: activity, code, build, download, upload, sync, agent, bell, timer, claude, cursor, codex, copilot
Exit codes: 0 shown, 1 bad arguments, 2 Mochi isn't running, 3 refused (the reason is printed).";

fn write_crlf_line(out: &mut impl Write, text: &str) {
    let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let _ = out.write_all(normalized.as_bytes());
    let _ = out.write_all(b"\r\n");
    let _ = out.flush();
}

fn write_stdout_line(text: &str) {
    let mut out = std::io::stdout().lock();
    write_crlf_line(&mut out, text);
}

fn write_stderr_line(text: &str) {
    let mut err = std::io::stderr().lock();
    write_crlf_line(&mut err, text);
}

fn collect_args(matches: &ArgMatches) -> Vec<String> {
    matches
        .get_many::<OsString>("args")
        .into_iter()
        .flatten()
        .map(|s| s.to_string_lossy().into_owned())
        .collect()
}

pub fn run(command: &str, matches: &ArgMatches) -> ExitCode {
    let args = collect_args(matches);

    match command {
        "help" => {
            write_stdout_line(USAGE);
            ExitCode::from(0)
        }
        "mochi-cli" => {
            if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
                write_stdout_line(USAGE);
                ExitCode::from(0)
            } else {
                write_stderr_line(USAGE);
                ExitCode::from(1)
            }
        }
        "skill" => {
            if args.first().map(String::as_str) == Some("install") {
                install_skill()
            } else {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(SKILL_MD.as_bytes());
                let _ = out.flush();
                ExitCode::from(0)
            }
        }
        "hook" => {
            if args.len() >= 2 && args[0] == "setup" {
                match mochi_activities::port::agent_hook_setup(&args[1]) {
                    Some(text) => {
                        write_stdout_line(text);
                        ExitCode::from(0)
                    }
                    None => {
                        write_stderr_line(USAGE);
                        ExitCode::from(1)
                    }
                }
            } else {
                run_hook(&args)
            }
        }
        "activity" => run_activity(&args),
        _ => {
            write_stderr_line(USAGE);
            ExitCode::from(1)
        }
    }
}

fn install_skill() -> ExitCode {
    let Some(home) = std::env::var_os("USERPROFILE") else {
        return ExitCode::from(1);
    };
    let home = PathBuf::from(home);
    for root in [".claude", ".agents", ".copilot"] {
        let dir = home.join(root).join("skills").join("mochi-notch");
        if std::fs::create_dir_all(&dir).is_err() {
            return ExitCode::from(1);
        }
        let file = dir.join("SKILL.md");
        if std::fs::write(&file, SKILL_MD.as_bytes()).is_err() {
            return ExitCode::from(1);
        }
        write_stdout_line(&file.to_string_lossy());
    }
    ExitCode::from(0)
}

fn run_activity(args: &[String]) -> ExitCode {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        write_stderr_line(ACTIVITY_USAGE);
        return ExitCode::from(if args.is_empty() { 1 } else { 0 });
    }
    let hw = HostWindow::find();
    let json = match mochi_activities::port::build_activity_command(
        args,
        hw.map(|w| w.handle),
        hw.map(|w| i64::from(w.pid)),
    ) {
        Ok(j) => j,
        Err(problem) => {
            write_stderr_line(&format!("{problem}\r\n\r\n{ACTIVITY_USAGE}"));
            return ExitCode::from(1);
        }
    };
    match send_activity(&json, None, 2000) {
        None => {
            write_stderr_line("Mochi isn't running.");
            ExitCode::from(2)
        }
        Some(reply) if reply == "ok" => ExitCode::from(0),
        Some(reply) => {
            write_stderr_line(&reply);
            ExitCode::from(3)
        }
    }
}

fn read_hook_payload(extra: &[String]) -> Option<String> {
    if let Some(last) = extra.last()
        && last.trim_start().starts_with('{')
    {
        return Some(last.clone());
    }
    if std::io::stdin().is_terminal() {
        return None;
    }
    let mut buf = Vec::new();
    let mut stdin = std::io::stdin().lock();
    let _ = stdin.by_ref().take(1024 * 1024).read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let trimmed = text.trim_start_matches('\u{FEFF}');
    let capped: String = trimmed.chars().take(256 * 1024).collect();
    Some(capped)
}

fn enrich_hook_request(
    agent: &str,
    payload: Option<&str>,
    req_json: &str,
    hw: HostWindow,
) -> String {
    let Ok(mut doc) = serde_json::from_str::<Value>(req_json) else {
        return req_json.to_string();
    };
    let Some(map) = doc.as_object_mut() else {
        return req_json.to_string();
    };
    if map.get("op").and_then(Value::as_str) != Some("set") {
        return req_json.to_string();
    }
    let mut win = Map::new();
    win.insert("handle".to_string(), json!(hw.handle));
    win.insert("pid".to_string(), json!(hw.pid));
    map.insert("window".to_string(), Value::Object(win));

    if let Some(host) = hw.app_name() {
        let product = match agent {
            "copilot" => "GitHub Copilot",
            "claude" => "Claude Code",
            "cursor" => "Cursor",
            "codex" => "Codex",
            _ => "",
        };
        let folder = payload
            .and_then(|p| serde_json::from_str::<Value>(p).ok())
            .and_then(|v| match agent {
                "cursor" => v
                    .get("workspace_roots")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                    .map(str::to_string),
                _ => v.get("cwd").and_then(Value::as_str).map(str::to_string),
            });
        let project = mochi_activities::port::agent_hook_project_name(folder.as_deref());
        let title = mochi_activities::port::agent_hook_title(product, &project, Some(&host));
        map.insert("title".to_string(), Value::String(title));
    }
    serde_json::to_string(&doc).unwrap_or_else(|_| req_json.to_string())
}

fn run_hook(args: &[String]) -> ExitCode {
    let agent = args.first().map(String::as_str).unwrap_or("");
    if !matches!(agent, "copilot" | "claude" | "cursor" | "codex") {
        write_stderr_line(&format!(
            "mochi-cli hook: unknown agent \"{agent}\" (use copilot, claude, cursor, codex)"
        ));
        return ExitCode::from(0);
    }
    let payload = read_hook_payload(&args[1..]);
    let res = mochi_activities::port::translate_agent_hook(agent, payload.as_deref());
    if let Some(output) = res.output {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(output.as_bytes());
        let _ = out.flush();
    }
    if let Some(req) = res.request {
        let host_win = HostWindow::find().or_else(|| {
            if agent == "copilot" {
                HostWindow::find_vscode()
            } else {
                None
            }
        });
        let final_req = match host_win {
            Some(hw) => enrich_hook_request(agent, payload.as_deref(), &req, hw),
            None => req,
        };
        let _ = send_activity(&final_req, None, 2000);
    }
    ExitCode::from(0)
}
