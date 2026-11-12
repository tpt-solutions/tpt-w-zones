//! `tpt-zones-cli`: scripting entry point for tpt-zones.
//!
//! ```text
//! tpt-zones-cli validate [FILE]           check a config file (default: the user config)
//! tpt-zones-cli import FILE               validate FILE and install it as the user config
//! tpt-zones-cli export FILE               write the user config to FILE
//! tpt-zones-cli monitors                  list monitors
//! tpt-zones-cli windows                   list manageable windows
//! tpt-zones-cli apply [FILE] [--profile P]  apply placement rules once
//! ```
//!
//! Live window operations need a backend, which exists on Windows only for now.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod ctl;
#[cfg(windows)]
mod service;

use zones_core::config::Config;
use zones_core::{Error, Result};

const USAGE: &str = "\
usage:
  tpt-zones-cli validate [FILE]
  tpt-zones-cli import FILE
  tpt-zones-cli export FILE
  tpt-zones-cli monitors
  tpt-zones-cli windows
  tpt-zones-cli apply [FILE] [--profile PROFILE]
  tpt-zones-cli serve [FILE]                run the service (hotkeys, rules, IPC)
  tpt-zones-cli ctl ping|windows|monitors|reload|watch
  tpt-zones-cli ctl snap WINDOW_ID LAYOUT ZONE";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let component = if args.first().map(String::as_str) == Some("serve") {
        "service"
    } else {
        "cli"
    };
    zones_core::crash::install(component);
    // The service logs its actions at info; one-shot commands stay quiet unless asked.
    let default_level = if args.first().map(String::as_str) == Some("serve") {
        "info"
    } else {
        "warn"
    };
    zones_core::logging::init(default_level);
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Config(msg)) if msg.starts_with("usage") => {
            eprintln!("{msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let Some((command, rest)) = args.split_first() else {
        return Err(usage("missing command"));
    };
    match command.as_str() {
        "validate" => {
            let path = optional_path(rest)?;
            let cfg = Config::load(&path)?;
            println!(
                "{}: ok ({} layouts, {} profiles, {} rules)",
                path.display(),
                cfg.layouts.len(),
                cfg.profiles.len(),
                cfg.rules.len()
            );
            Ok(())
        }
        "import" => {
            let source = required_path(rest, "import")?;
            let cfg = Config::load(&source)?;
            let target = user_config_path()?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Error::Config(format!("creating {}: {e}", parent.display())))?;
            }
            cfg.save(&target)?;
            println!("imported {} -> {}", source.display(), target.display());
            Ok(())
        }
        "export" => {
            let destination = required_path(rest, "export")?;
            let cfg = Config::load(&user_config_path()?)?;
            cfg.save(&destination)?;
            println!("exported to {}", destination.display());
            Ok(())
        }
        "monitors" => list_monitors(),
        "windows" => list_windows(),
        "serve" => serve(rest),
        "ctl" => ctl::run(rest),
        "apply" => {
            let (path, profile) = parse_apply_args(rest)?;
            let cfg = Config::load(&path)?;
            let profile = profile.or_else(|| cfg.active_profile.clone());
            apply_rules(&cfg, profile.as_deref())
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(usage(&format!("unknown command '{other}'"))),
    }
}

fn usage(reason: &str) -> Error {
    Error::Config(format!("usage: {reason}"))
}

fn optional_path(rest: &[String]) -> Result<PathBuf> {
    match rest.first() {
        Some(p) => Ok(PathBuf::from(p)),
        None => user_config_path(),
    }
}

fn required_path(rest: &[String], command: &str) -> Result<PathBuf> {
    rest.first()
        .map(PathBuf::from)
        .ok_or_else(|| usage(&format!("'{command}' needs a FILE argument")))
}

fn parse_apply_args(rest: &[String]) -> Result<(PathBuf, Option<String>)> {
    let mut path = None;
    let mut profile = None;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        if arg == "--profile" {
            profile = Some(
                iter.next()
                    .ok_or_else(|| usage("--profile needs a name"))?
                    .clone(),
            );
        } else if path.is_none() {
            path = Some(PathBuf::from(arg));
        } else {
            return Err(usage(&format!("unexpected argument '{arg}'")));
        }
    }
    Ok((path.map_or_else(user_config_path, Ok)?, profile))
}

/// Per-user configuration location: `%APPDATA%\tpt-zones\config.toml` on Windows,
/// `$XDG_CONFIG_HOME/tpt-zones/config.toml` (or `~/.config/...`) elsewhere.
fn user_config_path() -> Result<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
    };
    base.map(|b| b.join("tpt-zones").join("config.toml"))
        .ok_or_else(|| Error::Config("cannot locate the user config directory".into()))
}

#[cfg(windows)]
fn list_monitors() -> Result<()> {
    use zones_core::backend::WindowBackend;
    let backend = zones_win::WinBackend::new();
    for m in backend.monitors()? {
        println!(
            "{:<10} {}{} bounds={}x{}+{}+{} work={}x{}+{}+{} scale={:.2}",
            m.name,
            if m.primary { "[primary] " } else { "" },
            format_args!("id={}", m.id.0),
            m.bounds.width,
            m.bounds.height,
            m.bounds.x,
            m.bounds.y,
            m.work_area.width,
            m.work_area.height,
            m.work_area.x,
            m.work_area.y,
            m.scale_factor,
        );
    }
    Ok(())
}

#[cfg(windows)]
fn list_windows() -> Result<()> {
    use zones_core::backend::WindowBackend;
    let backend = zones_win::WinBackend::new();
    for w in backend.windows()? {
        println!(
            "{:>10}  {:<20} {}  [{}x{}+{}+{}]{}",
            w.id.0,
            w.process,
            w.title,
            w.rect.width,
            w.rect.height,
            w.rect.x,
            w.rect.y,
            if w.minimized { " (minimised)" } else { "" },
        );
    }
    Ok(())
}

#[cfg(windows)]
fn apply_rules(cfg: &Config, profile: Option<&str>) -> Result<()> {
    let backend = zones_win::WinBackend::new();
    let applied = zones_core::engine::apply(&backend, cfg, profile)?;
    println!("applied {} placement(s)", applied.len());
    for p in &applied {
        println!(
            "  window {} -> {}x{}+{}+{} ({})",
            p.window.0,
            p.target.width,
            p.target.height,
            p.target.x,
            p.target.y,
            p.rule.as_deref().unwrap_or("unnamed rule")
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn list_monitors() -> Result<()> {
    Err(Error::Backend(
        "no live backend for this platform yet".into(),
    ))
}

#[cfg(not(windows))]
fn list_windows() -> Result<()> {
    Err(Error::Backend(
        "no live backend for this platform yet".into(),
    ))
}

#[cfg(not(windows))]
fn apply_rules(_cfg: &Config, _profile: Option<&str>) -> Result<()> {
    Err(Error::Backend(
        "no live backend for this platform yet".into(),
    ))
}

#[cfg(windows)]
fn serve(rest: &[String]) -> Result<()> {
    let path = optional_path(rest)?;
    service::run(path)
}

#[cfg(not(windows))]
fn serve(_rest: &[String]) -> Result<()> {
    Err(Error::Backend(
        "the service runs on Windows only for now".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_args_accept_profile_flag_in_either_position() {
        let args: Vec<String> = ["cfg.toml", "--profile", "docked"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (path, profile) = parse_apply_args(&args).unwrap();
        assert_eq!(path, PathBuf::from("cfg.toml"));
        assert_eq!(profile.as_deref(), Some("docked"));
    }

    #[test]
    fn apply_args_reject_extra_positionals() {
        let args: Vec<String> = ["a.toml", "b.toml"].iter().map(|s| s.to_string()).collect();
        assert!(parse_apply_args(&args).is_err());
    }

    #[test]
    fn unknown_command_is_usage_error() {
        let err = run(&["nope".to_string()]).unwrap_err();
        assert!(matches!(err, Error::Config(ref m) if m.starts_with("usage")));
    }
}
