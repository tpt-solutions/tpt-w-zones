//! Local crash reports. Nothing here touches the network.
//!
//! When a panic happens, [`install`] writes a small text report to a per-user folder and then
//! runs the previous panic hook, so the usual stderr output is unchanged. The report holds the
//! tpt-zones version, the component, the OS and architecture, the time, the thread name, the
//! panic message and its source location. It does not hold window titles, process names or
//! configuration: those are never passed to the panic machinery by this code.
//!
//! Reports are kept to the newest [`KEEP`] files. Turn the feature off with
//! `[general] crash_reports = false`; the service applies that setting at startup and reload.

use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many reports to keep before the oldest are deleted.
pub const KEEP: usize = 20;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Turns report writing on or off. Installing the hook is separate, so this is cheap to change.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Whether report writing is currently on.
#[must_use]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The folder reports are written to: `%LOCALAPPDATA%\tpt-zones\crashes` on Windows, and
/// `$XDG_STATE_HOME/tpt-zones/crashes` (or `~/.local/state/...`) elsewhere.
#[must_use]
pub fn crash_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
    };
    base.map(|b| b.join("tpt-zones").join("crashes"))
}

/// Installs the panic hook for one component (`service`, `editor`, `cli`). Call it once,
/// as early as possible in `main`.
pub fn install(component: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if is_enabled() {
            if let Some(dir) = crash_dir() {
                let report = format_report(component, &describe(info));
                let stamp = unix_now();
                // Writing a report must never make a panic worse, so errors are ignored.
                let _ = write_report(&dir, component, stamp, &report);
                let _ = prune(&dir, KEEP);
            }
        }
        previous(info);
    }));
}

/// What a panic said, without any of our state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicDetails {
    /// The panic message.
    pub message: String,
    /// `file:line:column`, when the compiler recorded one.
    pub location: Option<String>,
    /// Name of the thread that panicked.
    pub thread: String,
}

fn describe(info: &PanicHookInfo<'_>) -> PanicDetails {
    PanicDetails {
        message: payload_text(info.payload()),
        location: info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
        thread: std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_owned(),
    }
}

/// Extracts a panic payload as text. Most panics carry a `&str` or `String`.
#[must_use]
pub fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic with a non-text payload".to_owned()
    }
}

/// Builds the report text. Pure, so it can be tested without panicking.
#[must_use]
pub fn format_report(component: &str, details: &PanicDetails) -> String {
    format!(
        "tpt-zones crash report\n\
         component: {component}\n\
         version: {version}\n\
         os: {os} {arch}\n\
         thread: {thread}\n\
         location: {location}\n\
         message: {message}\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        thread = details.thread,
        location = details.location.as_deref().unwrap_or("unknown"),
        message = details.message,
    )
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes one report and returns its path.
///
/// # Errors
/// Returns an I/O error if the folder cannot be created or the file cannot be written.
pub fn write_report(
    dir: &Path,
    component: &str,
    stamp: u64,
    report: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("crash-{stamp}-{component}.txt"));
    std::fs::write(&path, report)?;
    Ok(path)
}

/// Deletes all but the newest `keep` reports in `dir`. Only files named `crash-*.txt` are touched.
///
/// # Errors
/// Returns an I/O error if the folder cannot be read.
pub fn prune(dir: &Path, keep: usize) -> std::io::Result<()> {
    let mut reports: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        })
        .collect();
    // File names start with a Unix timestamp, so sorting by name sorts by age.
    reports.sort();
    let excess = reports.len().saturating_sub(keep);
    for old in reports.into_iter().take(excess) {
        std::fs::remove_file(old)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tpt-crash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn payload_text_handles_str_string_and_other() {
        let s: Box<dyn std::any::Any + Send> = Box::new("boom");
        assert_eq!(payload_text(s.as_ref()), "boom");
        let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("bang"));
        assert_eq!(payload_text(owned.as_ref()), "bang");
        let other: Box<dyn std::any::Any + Send> = Box::new(42_u32);
        assert_eq!(
            payload_text(other.as_ref()),
            "panic with a non-text payload"
        );
    }

    #[test]
    fn report_contains_only_the_expected_fields() {
        let details = PanicDetails {
            message: "index out of range".into(),
            location: Some("src/x.rs:1:2".into()),
            thread: "zones-win-events".into(),
        };
        let text = format_report("service", &details);
        for field in [
            "component: service",
            "version:",
            "os:",
            "thread: zones-win-events",
            "location: src/x.rs:1:2",
            "message: index out of range",
        ] {
            assert!(text.contains(field), "missing {field} in {text}");
        }
        assert!(!text.contains("title"));
    }

    #[test]
    fn write_then_prune_keeps_newest() {
        let dir = temp_dir("prune");
        for stamp in 1..=5_u64 {
            write_report(&dir, "cli", stamp, "x").unwrap();
        }
        prune(&dir, 2).unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        left.sort();
        assert_eq!(left, vec!["crash-4-cli.txt", "crash-5-cli.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn prune_ignores_unrelated_files() {
        let dir = temp_dir("unrelated");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "keep me").unwrap();
        for stamp in 1..=3_u64 {
            write_report(&dir, "cli", stamp, "x").unwrap();
        }
        prune(&dir, 1).unwrap();
        assert!(dir.join("notes.txt").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn panic_writes_a_report_when_enabled() {
        let root = temp_dir("hook");
        std::env::set_var("LOCALAPPDATA", &root);
        std::env::set_var("XDG_STATE_HOME", &root);
        install("test");
        set_enabled(true);
        let _ = std::panic::catch_unwind(|| panic!("test boom"));
        let dir = crash_dir().expect("dir");
        let report = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| p.to_string_lossy().ends_with("-test.txt"))
            .expect("a report file");
        let text = std::fs::read_to_string(report).unwrap();
        assert!(text.contains("component: test"), "{text}");
        assert!(text.contains("message: test boom"), "{text}");
        set_enabled(false);
        let _ = std::panic::catch_unwind(|| panic!("silent"));
        set_enabled(true);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn toggle_round_trips() {
        set_enabled(false);
        assert!(!is_enabled());
        set_enabled(true);
        assert!(is_enabled());
    }
}
