//! Platform-neutral hotkey description and its text form (`"Ctrl+Alt+Left"`).
//!
//! Backends translate [`Hotkey`] into their native representation (Win32
//! `RegisterHotKey` virtual keys, or Wayland/compositor bindings).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::Error;

/// Modifier keys held alongside the main key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Modifiers {
    /// Control key.
    pub ctrl: bool,
    /// Alt key (Option on macOS, not a target platform).
    pub alt: bool,
    /// Shift key.
    pub shift: bool,
    /// Windows / Super / Meta key.
    pub super_key: bool,
}

/// The non-modifier key of a hotkey, normalised to an upper-case name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Key(String);

impl Key {
    /// Parses and normalises a key name. Single characters are upper-cased;
    /// named keys (`Left`, `F12`, `Space`) are matched case-insensitively.
    ///
    /// # Errors
    /// Returns [`Error::Hotkey`] if the name is empty or contains whitespace.
    pub fn new(name: &str) -> crate::Result<Self> {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
            return Err(Error::Hotkey(name.to_owned()));
        }
        let normalised = if trimmed.chars().count() == 1 {
            trimmed.to_ascii_uppercase()
        } else {
            canonical_name(trimmed)
        };
        Ok(Self(normalised))
    }

    /// The normalised key name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}

fn canonical_name(name: &str) -> String {
    const NAMED: &[&str] = &[
        "Left",
        "Right",
        "Up",
        "Down",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Space",
        "Enter",
        "Tab",
        "Escape",
        "Backspace",
        "Delete",
        "Insert",
    ];
    NAMED
        .iter()
        .find(|n| n.eq_ignore_ascii_case(name))
        .map_or_else(|| name.to_ascii_uppercase(), |n| (*n).to_owned())
}

impl TryFrom<String> for Key {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<Key> for String {
    fn from(key: Key) -> Self {
        key.0
    }
}

/// A complete hotkey: a set of modifiers plus one key.
///
/// Serialised as its text form, e.g. `"Ctrl+Alt+Left"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hotkey {
    /// Modifier keys.
    pub modifiers: Modifiers,
    /// Main key.
    pub key: Key,
}

impl TryFrom<String> for Hotkey {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Hotkey> for String {
    fn from(hk: Hotkey) -> Self {
        hk.to_string()
    }
}

impl FromStr for Hotkey {
    type Err = Error;

    /// Parses strings such as `"Ctrl+Alt+Left"` or `"Super+Shift+1"`.
    /// Modifier names are case-insensitive; `Win`, `Windows` and `Meta` alias `Super`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let key_part = parts.pop().filter(|k| !k.is_empty());
        let key_part = key_part.ok_or_else(|| Error::Hotkey(s.to_owned()))?;

        let mut modifiers = Modifiers::default();
        for part in parts {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "super" | "win" | "windows" | "meta" => modifiers.super_key = true,
                _ => return Err(Error::Hotkey(s.to_owned())),
            }
        }
        Ok(Self {
            modifiers,
            key: Key::new(key_part)?,
        })
    }
}

impl fmt::Display for Hotkey {
    /// Formats in canonical order: `Super+Ctrl+Alt+Shift+Key`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.modifiers;
        let mods = [
            (m.super_key, "Super"),
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
        ];
        for (on, name) in mods {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(self.key.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_named_key() {
        let hk: Hotkey = "ctrl+alt+left".parse().unwrap();
        assert!(hk.modifiers.ctrl && hk.modifiers.alt && !hk.modifiers.shift);
        assert_eq!(hk.key.name(), "Left");
    }

    #[test]
    fn win_aliases_super() {
        let hk: Hotkey = "Win+Shift+1".parse().unwrap();
        assert!(hk.modifiers.super_key && hk.modifiers.shift);
        assert_eq!(hk.key.name(), "1");
    }

    #[test]
    fn display_is_canonical_and_round_trips() {
        let hk: Hotkey = "Shift+Ctrl+a".parse().unwrap();
        assert_eq!(hk.to_string(), "Ctrl+Shift+A");
        assert_eq!(hk.to_string().parse::<Hotkey>().unwrap(), hk);
    }

    #[test]
    fn rejects_unknown_modifier_and_empty_key() {
        assert!("Hyper+A".parse::<Hotkey>().is_err());
        assert!("Ctrl+".parse::<Hotkey>().is_err());
        assert!("".parse::<Hotkey>().is_err());
    }
}
