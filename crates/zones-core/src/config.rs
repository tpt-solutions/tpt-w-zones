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
//! snap_left = "Ctrl+Alt+Left"
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

use serde::{Deserialize, Serialize};

use crate::hotkey::Hotkey;
use crate::model::{Layout, Profile};
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
    /// Action name → hotkey (e.g. `snap_left = "Ctrl+Alt+Left"`).
    #[serde(default)]
    pub hotkeys: BTreeMap<String, Hotkey>,
    /// Available layouts.
    #[serde(default, rename = "layouts")]
    pub layouts: Vec<Layout>,
    /// Available profiles.
    #[serde(default, rename = "profiles")]
    pub profiles: Vec<Profile>,
}

/// General preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct General {
    /// Start the manager automatically at login.
    pub autostart: bool,
    /// Log filter, e.g. `"info"` or `"zones_core=debug"`.
    pub log_level: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            autostart: false,
            log_level: "info".into(),
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
        }
    }
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
        Ok(())
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
snap_left = "Ctrl+Alt+Left"

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
        assert_eq!(cfg.hotkeys["snap_left"].to_string(), "Ctrl+Alt+Left");
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
    fn rejects_wrong_schema_version() {
        let bad = SAMPLE.replace("schema_version = 1", "schema_version = 2");
        assert!(Config::from_toml(&bad).is_err());
    }
}
