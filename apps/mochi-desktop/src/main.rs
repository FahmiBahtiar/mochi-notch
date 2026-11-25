#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::process::ExitCode;

use mochi_ui::{LaunchOptions, run_desktop};

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> ExitCode {
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "panic".to_string()
        };
        mochi_winrt::paths::Log::error("panic", &format!("{loc}: {payload}"));
    }));

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--remove-agent-hooks") {
        mochi_winrt::agent_setup::AgentSetup::remove_all();
        return ExitCode::from(0);
    }

    let show_page = args.iter().position(|a| a == "--show").map(|i| {
        let page = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "media".to_string());
        let secs = args
            .get(i + 2)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(20);
        (page, secs)
    });

    let opts = LaunchOptions {
        bench: arg_value(&args, "--bench"),
        bench_variant: arg_value(&args, "--bench-variant"),
        shots_dir: arg_value(&args, "--shots").map(PathBuf::from),
        frames_path: arg_value(&args, "--frames").map(PathBuf::from),
        show_page,
        open_settings: args.iter().any(|a| a == "--settings"),
        demo: args.iter().any(|a| a == "--demo"),
        ambient: args.iter().any(|a| a == "--ambient" || a == "--visor"),
        autostart: args.iter().any(|a| a == "--autostart"),
    };

    let code = run_desktop(opts);
    ExitCode::from(code as u8)
}
