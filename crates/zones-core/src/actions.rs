//! Named actions that hotkeys can trigger.
//!
//! In `config.toml`, the key of an entry in `[hotkeys]` is an action name:
//!
//! | Action name            | Effect                                              |
//! | ---------------------- | --------------------------------------------------- |
//! | `snap:LAYOUT:ZONE`     | Move the focused window into `ZONE` of `LAYOUT`      |
//! | `apply_rules`          | Apply every placement rule to the current windows    |
//! | `reload`               | Re-read `config.toml` and re-register hotkeys        |
//!
//! ```toml
//! [hotkeys]
//! "snap:halves:left" = "Ctrl+Alt+Left"
//! apply_rules = "Ctrl+Alt+R"
//! ```

use std::fmt;
use std::str::FromStr;

use crate::model::ZoneId;
use crate::{Error, Result};

/// Something a hotkey (or a script) can ask the service to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Snap the focused window into a zone.
    SnapFocused {
        /// Layout id.
        layout: String,
        /// Zone id within the layout.
        zone: ZoneId,
    },
    /// Apply placement rules to all current windows.
    ApplyRules,
    /// Reload the configuration from disk.
    Reload,
}

impl FromStr for Action {
    type Err = Error;

    /// Parses an action name such as `snap:halves:left`.
    ///
    /// # Errors
    /// Returns [`Error::Config`] for unknown names or a malformed `snap:` action.
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "apply_rules" => return Ok(Self::ApplyRules),
            "reload" => return Ok(Self::Reload),
            _ => {}
        }
        let rest = s
            .strip_prefix("snap:")
            .ok_or_else(|| Error::Config(format!("unknown action '{s}'")))?;
        match rest.split_once(':') {
            Some((layout, zone)) if !layout.is_empty() && !zone.is_empty() => {
                Ok(Self::SnapFocused {
                    layout: layout.to_owned(),
                    zone: ZoneId(zone.to_owned()),
                })
            }
            _ => Err(Error::Config(format!(
                "action '{s}' must look like snap:LAYOUT:ZONE"
            ))),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SnapFocused { layout, zone } => write!(f, "snap:{layout}:{}", zone.0),
            Self::ApplyRules => f.write_str("apply_rules"),
            Self::Reload => f.write_str("reload"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_actions() {
        assert_eq!(Action::from_str("reload").unwrap(), Action::Reload);
        assert_eq!(Action::from_str("apply_rules").unwrap(), Action::ApplyRules);
        assert_eq!(
            Action::from_str("snap:halves:left").unwrap(),
            Action::SnapFocused {
                layout: "halves".into(),
                zone: ZoneId("left".into()),
            }
        );
    }

    #[test]
    fn rejects_malformed_actions() {
        assert!(Action::from_str("snap_left").is_err());
        assert!(Action::from_str("snap:halves").is_err());
        assert!(Action::from_str("snap::left").is_err());
        assert!(Action::from_str("explode").is_err());
    }

    #[test]
    fn display_round_trips() {
        let a = Action::SnapFocused {
            layout: "thirds".into(),
            zone: ZoneId("main".into()),
        };
        assert_eq!(a.to_string().parse::<Action>().unwrap(), a);
    }
}
