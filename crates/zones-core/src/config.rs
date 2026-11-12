//! User configuration file (`config.toml`).
//!
//! ```toml
//! schema_version = 1
//! active_profile = "docked"
//!
//! [general]
//! autostart = true
//! log_level = "info"
//!
//! [hotkeys]
//! "snap:halves:left" = "Ctrl+Alt+Left"
//!
//! [[layouts]]
//! id = "halves"
//! name = "Halves"
//! zones = [
//!   { id = "left",  name = "Left",  rect = { x = 0.0, y = 0.0, w = 0.5, h = 1.0 } },
//!   { id = "right", name = "Right", rect = { x = 0.5, y = 0.0, w = 0.5, h = 1.0 } },
//! ]
//!
//! [[profiles]]
//! name = "docked"
//! monitors = { "DISPLAY1" = "halves" }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::actions::Action;
use crate::backend::WindowInfo;
use crate::hotkey::Hotkey;
use crate::model::{Layout, Profile};
use crate::rules::{first_match, Context, Rule};
use crate::{Error, Result};

/// The config schema version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

/// Top-level configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Name of the profile currently in effect, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_profile: Option<String>,
    /// General preferences.
    #[serde(default)]
    pub general: General,
    /// Action name → hotkey (e.g. `"snap:halves:left" = "Ctrl+Alt+Left"`, see `actions`).
    #[serde(default)]
    pub hotkeys: BTreeMap<String, Hotkey>,
    /// Available layouts.
    #[serde(default, rename = "layouts")]
    pub layouts: Vec<Layout>,
    /// Available profiles.
    #[serde(default, rename = "profiles")]
    pub profiles: Vec<Profile>,
    /// Placement rules, evaluated in order; the first match wins.
    #[serde(default, rename = "rules")]
    pub rules: Vec<Rule>,
}

/// General preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct General {
    /// Start the manager automatically at login.
    pub autostart: bool,
    /// Log filter, e.g. `"info"` or `"zones_core=debug"`.
    pub log_level: String,
    /// Write local crash reports when the program panics. Nothing is sent over the network.
    pub crash_reports: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            autostart: false,
            log_level: "info".into(),
            crash_reports: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            active_profile: None,
            general: General::default(),
            hotkeys: BTreeMap::new(),
            layouts: Vec::new(),
            profiles: Vec::new(),
            rules: Vec::new(),
        }
    }
}

/// Per-user configuration location: `%APPDATA%\tpt-zones\config.toml` on Windows, and
/// `$XDG_CONFIG_HOME/tpt-zones/config.toml` (or `~/.config/...`) elsewhere.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("tpt-zones").join("config.toml"))
}

impl Config {
    /// Parses and validates a configuration from TOML text.
    ///
    /// # Errors
    /// Returns [`Error::Config`] on syntax errors, unknown fields, an unsupported
    /// schema version, or any failed consistency check (see [`Config::validate`]).
    pub fn from_toml(text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).map_err(|e| Error::Config(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    /// Serialises the configuration to TOML.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if serialisation fails.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))
    }

    /// Checks cross-field consistency.
    ///
    /// # Errors
    /// Returns [`Error::Config`] for: wrong schema version, duplicate or invalid
    /// layouts, duplicate profile names, profiles referencing unknown layouts, or an
    /// `active_profile` that does not exist.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Config(format!(
                "unsupported schema_version {} (expected {SCHEMA_VERSION})",
                self.schema_version
            )));
        }

        let mut layout_ids = BTreeSet::new();
        for layout in &self.layouts {
            layout.validate()?;
            if !layout_ids.insert(layout.id.as_str()) {
                return Err(Error::Config(format!(
                    "duplicate layout id '{}'",
                    layout.id
                )));
            }
        }

        let mut profile_names = BTreeSet::new();
        for profile in &self.profiles {
            if !profile_names.insert(profile.name.as_str()) {
                return Err(Error::Config(format!(
                    "duplicate profile name '{}'",
                    profile.name
                )));
            }
            for (monitor, layout) in &profile.monitors {
                if !layout_ids.contains(layout.as_str()) {
                    return Err(Error::Config(format!(
                        "profile '{}' maps monitor '{monitor}' to unknown layout '{layout}'",
                        profile.name
                    )));
                }
            }
        }

        if let Some(active) = &self.active_profile {
            if !profile_names.contains(active.as_str()) {
                return Err(Error::Config(format!(
                    "active_profile '{active}' does not exist"
                )));
            }
        }

        for name in self.hotkeys.keys() {
            let action: Action = name.parse()?;
            if let Action::SnapFocused { layout, zone } = &action {
                let layout_def = self.layout(layout).ok_or_else(|| {
                    Error::Config(format!(
                        "hotkey '{name}' references unknown layout '{layout}'"
                    ))
                })?;
                if layout_def.zone(zone).is_none() {
                    return Err(Error::Config(format!(
                        "hotkey '{name}' references unknown zone '{}'",
                        zone.0
                    )));
                }
            }
        }

        for (index, rule) in self.rules.iter().enumerate() {
            let label = rule
                .name
                .clone()
                .unwrap_or_else(|| format!("#{}", index + 1));
            if !rule.has_window_condition() {
                return Err(Error::Config(format!(
                    "rule {label} must set process, class or title_contains"
                )));
            }
            let layout = self.layout(&rule.layout).ok_or_else(|| {
                Error::Config(format!(
                    "rule {label} references unknown layout '{}'",
                    rule.layout
                ))
            })?;
            if layout.zone(&rule.zone).is_none() {
                return Err(Error::Config(format!(
                    "rule {label} references unknown zone '{}' in layout '{}'",
                    rule.zone.0, rule.layout
                )));
            }
            if let Some(profile) = &rule.profile {
                if !profile_names.contains(profile.as_str()) {
                    return Err(Error::Config(format!(
                        "rule {label} references unknown profile '{profile}'"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Reads, parses and validates a configuration file.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if the file cannot be read or is invalid.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
        Self::from_toml(&text)
    }

    /// Validates and writes the configuration to `path` atomically: the data goes to a
    /// temporary sibling file that is then renamed over the target, so a crash cannot
    /// leave a half-written config behind.
    ///
    /// # Errors
    /// Returns [`Error::Config`] if validation or any file operation fails.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = self.to_toml()?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)
            .map_err(|e| Error::Config(format!("writing {}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| Error::Config(format!("replacing {}: {e}", path.display())))
    }

    /// Evaluates the rules for `window` and returns the first matching rule, if any.
    #[must_use]
    pub fn placement_for<'a>(&'a self, window: &WindowInfo, ctx: Context<'_>) -> Option<&'a Rule> {
        first_match(&self.rules, window, ctx)
    }

    /// Finds a layout by id.
    #[must_use]
    pub fn layout(&self, id: &str) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.id == id)
    }

    /// Finds a profile by name.
    #[must_use]
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// The profile currently in effect, if one is set and exists.
    #[must_use]
    pub fn active(&self) -> Option<&Profile> {
        self.active_profile.as_deref().and_then(|n| self.profile(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
schema_version = 1
active_profile = "docked"

[general]
autostart = true
log_level = "debug"

[hotkeys]
"snap:halves:left" = "Ctrl+Alt+Left"

[[layouts]]
id = "halves"
name = "Halves"
zones = [
  { id = "left",  name = "Left",  rect = { x = 0.0, y = 0.0, w = 0.5, h = 1.0 } },
  { id = "right", name = "Right", rect = { x = 0.5, y = 0.0, w = 0.5, h = 1.0 } },
]

[[profiles]]
name = "docked"
monitors = { "DISPLAY1" = "halves" }
"#;

    #[test]
    fn parses_sample() {
        let cfg = Config::from_toml(SAMPLE).unwrap();
        assert!(cfg.general.autostart);
        assert_eq!(cfg.hotkeys["snap:halves:left"].to_string(), "Ctrl+Alt+Left");
        assert_eq!(cfg.active().unwrap().layout_for("DISPLAY1"), Some("halves"));
    }

    #[test]
    fn round_trips_through_toml() {
        let cfg = Config::from_toml(SAMPLE).unwrap();
        let text = cfg.to_toml().unwrap();
        assert_eq!(Config::from_toml(&text).unwrap(), cfg);
    }

    #[test]
    fn rejects_unknown_layout_reference() {
        let bad = SAMPLE.replace(r#""DISPLAY1" = "halves""#, r#""DISPLAY1" = "nope""#);
        assert!(matches!(Config::from_toml(&bad), Err(Error::Config(_))));
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let bad = SAMPLE.replace("schema_version = 1", "schema_version = 1\nmystery = 1");
        assert!(Config::from_toml(&bad).is_err());
    }

    #[test]
    fn rules_reference_known_layouts_zones_and_profiles() {
        let base = format!(
            "{SAMPLE}\n[[rules]]\nprocess = \"code.exe\"\nlayout = \"halves\"\nzone = \"left\"\n"
        );
        assert!(Config::from_toml(&base).is_ok());

        let bad_zone = base.replace("zone = \"left\"", "zone = \"nope\"");
        assert!(Config::from_toml(&bad_zone).is_err());

        let bad_layout = base.replace("layout = \"halves\"\nzone", "layout = \"ghost\"\nzone");
        assert!(Config::from_toml(&bad_layout).is_err());

        let no_condition = format!("{SAMPLE}\n[[rules]]\nlayout = \"halves\"\nzone = \"left\"\n");
        assert!(Config::from_toml(&no_condition).is_err());
    }

    #[test]
    fn save_then_load_round_trips() {
        let cfg = Config::from_toml(SAMPLE).unwrap();
        let dir = std::env::temp_dir().join(format!("tpt-zones-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), cfg);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let bad = SAMPLE.replace("schema_version = 1", "schema_version = 2");
        assert!(Config::from_toml(&bad).is_err());
    }
}
