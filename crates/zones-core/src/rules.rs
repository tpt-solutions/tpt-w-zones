//! Scriptable layout rules: decide where a newly seen or moved window should go.
//!
//! Rules are evaluated in file order and the **first** match wins. A rule matches when
//! every condition it sets holds:
//!
//! - `process` / `class`: case-insensitive. A single trailing or leading `*` is a
//!   wildcard, for example `process = "code*"`.
//! - `title_contains`: case-insensitive substring of the window title.
//! - `profile`: only while this profile is active.
//! - `monitor`: only on the monitor with this stable name (for example `"DISPLAY1"`).
//!
//! A rule must set at least one of `process`, `class` or `title_contains`. Without one it
//! would match every window, which is almost never intended.
//!
//! ```toml
//! [[rules]]
//! name = "editor on the left"
//! process = "code.exe"
//! layout = "halves"
//! zone = "left"
//! ```

use serde::{Deserialize, Serialize};

use crate::backend::WindowInfo;
use crate::model::ZoneId;

/// One scripted placement rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// Optional label, used in logs and error messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Executable or application id, e.g. `"code.exe"` or `"firefox*"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    /// Window class, e.g. `"Chrome_WidgetWin_1"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Substring that must appear in the window title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<String>,
    /// Restrict the rule to this profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Restrict the rule to the monitor with this stable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// Layout to apply.
    pub layout: String,
    /// Zone within that layout.
    pub zone: ZoneId,
}

/// Facts about the environment a rule is evaluated against.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'a> {
    /// Stable name of the monitor the window is on, if known.
    pub monitor: Option<&'a str>,
    /// Name of the active profile, if any.
    pub profile: Option<&'a str>,
}

impl Rule {
    /// Returns `true` if the rule has at least one window-matching condition.
    #[must_use]
    pub fn has_window_condition(&self) -> bool {
        self.process.is_some() || self.class.is_some() || self.title_contains.is_some()
    }

    /// Checks every condition of this rule against a window and its context.
    #[must_use]
    pub fn matches(&self, window: &WindowInfo, ctx: Context<'_>) -> bool {
        if let Some(p) = &self.process {
            if !wildcard_match(p, &window.process) {
                return false;
            }
        }
        if let Some(c) = &self.class {
            let class = window.class.as_deref().unwrap_or("");
            if !wildcard_match(c, class) {
                return false;
            }
        }
        if let Some(t) = &self.title_contains {
            if !contains_ignore_case(&window.title, t) {
                return false;
            }
        }
        if let Some(profile) = &self.profile {
            if ctx.profile != Some(profile.as_str()) {
                return false;
            }
        }
        if let Some(monitor) = &self.monitor {
            if ctx.monitor != Some(monitor.as_str()) {
                return false;
            }
        }
        true
    }
}

/// Returns the first rule in `rules` that matches, if any.
#[must_use]
pub fn first_match<'a>(
    rules: &'a [Rule],
    window: &WindowInfo,
    ctx: Context<'_>,
) -> Option<&'a Rule> {
    rules.iter().find(|rule| rule.matches(window, ctx))
}

/// Case-insensitive match where `*` may appear once, at the start or the end.
/// An exact pattern requires an exact (case-insensitive) match.
#[must_use]
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let text = text.to_ascii_lowercase();
    match (pattern.strip_prefix('*'), pattern.strip_suffix('*')) {
        (Some(suffix), _) if pattern.len() > 1 => text.ends_with(suffix),
        (_, Some(prefix)) if pattern.len() > 1 => text.starts_with(prefix),
        _ if pattern == "*" => true,
        _ => text == pattern,
    }
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::WindowId;
    use crate::geometry::Rect;

    fn window(process: &str, title: &str, class: Option<&str>) -> WindowInfo {
        WindowInfo {
            id: WindowId(1),
            title: title.into(),
            process: process.into(),
            class: class.map(Into::into),
            rect: Rect::new(0, 0, 100, 100),
            monitor: None,
            minimized: false,
        }
    }

    fn rule(process: &str) -> Rule {
        Rule {
            name: None,
            process: Some(process.into()),
            class: None,
            title_contains: None,
            profile: None,
            monitor: None,
            layout: "halves".into(),
            zone: ZoneId("left".into()),
        }
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("code.exe", "CODE.EXE"));
        assert!(!wildcard_match("code.exe", "code.exe.bak"));
        assert!(wildcard_match("code*", "code.exe"));
        assert!(wildcard_match("*.exe", "Firefox.EXE"));
        assert!(!wildcard_match("code*", "vscode.exe"));
        assert!(wildcard_match("*", "anything"));
    }

    #[test]
    fn first_matching_rule_wins() {
        let rules = vec![rule("code*"), rule("*")];
        let w = window("code.exe", "main.rs", None);
        let hit = first_match(&rules, &w, Context::default()).unwrap();
        assert_eq!(hit.process.as_deref(), Some("code*"));
    }

    #[test]
    fn profile_and_monitor_conditions_are_enforced() {
        let mut r = rule("code.exe");
        r.profile = Some("docked".into());
        r.monitor = Some("DISPLAY2".into());
        let w = window("code.exe", "", None);

        let docked = Context {
            monitor: Some("DISPLAY2"),
            profile: Some("docked"),
        };
        assert!(r.matches(&w, docked));
        assert!(!r.matches(
            &w,
            Context {
                monitor: Some("DISPLAY2"),
                profile: Some("laptop")
            }
        ));
        assert!(!r.matches(
            &w,
            Context {
                monitor: Some("DISPLAY1"),
                profile: Some("docked")
            }
        ));
    }

    #[test]
    fn title_and_class_conditions() {
        let mut r = rule("chrome.exe");
        r.process = None;
        r.class = Some("Chrome_WidgetWin_1".into());
        r.title_contains = Some("Mail".into());
        let w = window(
            "chrome.exe",
            "Inbox - gmail MAIL",
            Some("Chrome_WidgetWin_1"),
        );
        assert!(r.matches(&w, Context::default()));
        let other = window("chrome.exe", "Docs", Some("Chrome_WidgetWin_1"));
        assert!(!r.matches(&other, Context::default()));
    }

    #[test]
    fn rule_without_window_condition_is_detected() {
        let mut r = rule("x");
        r.process = None;
        assert!(!r.has_window_condition());
    }
}
