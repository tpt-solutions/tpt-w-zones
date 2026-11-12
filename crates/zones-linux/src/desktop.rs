//! Desktop-environment detection from the session environment.
//!
//! The answer picks the compositor adapter. Detection is a pure function of the environment
//! variables it reads, so it is tested on any platform.

/// The desktop or compositor family the session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desktop {
    /// KDE Plasma (KWin).
    Kde,
    /// GNOME (Mutter).
    Gnome,
    /// A wlroots compositor such as Sway, Hyprland or River.
    Wlroots,
    /// Something else; the value of `XDG_CURRENT_DESKTOP`.
    Other(String),
    /// No recognisable desktop.
    Unknown,
}

/// Decides the desktop from `lookup`, which returns an environment variable's value.
#[must_use]
pub fn detect(lookup: impl Fn(&str) -> Option<String>) -> Desktop {
    let current = lookup("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let upper = current.to_ascii_uppercase();
    let has = |needle: &str| upper.split(':').any(|part| part == needle);

    if has("KDE") {
        return Desktop::Kde;
    }
    if has("GNOME") || has("UNITY") {
        return Desktop::Gnome;
    }
    let wlroots_env =
        lookup("SWAYSOCK").is_some() || lookup("HYPRLAND_INSTANCE_SIGNATURE").is_some();
    let wlroots_name = ["SWAY", "HYPRLAND", "RIVER", "WAYFIRE"]
        .iter()
        .any(|n| has(n));
    if wlroots_env || wlroots_name {
        return Desktop::Wlroots;
    }
    if current.is_empty() {
        Desktop::Unknown
    } else {
        Desktop::Other(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn detects_kde_and_gnome() {
        assert_eq!(detect(env(&[("XDG_CURRENT_DESKTOP", "KDE")])), Desktop::Kde);
        assert_eq!(
            detect(env(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")])),
            Desktop::Gnome
        );
    }

    #[test]
    fn detects_wlroots_by_socket_or_name() {
        assert_eq!(detect(env(&[("SWAYSOCK", "/run/sway")])), Desktop::Wlroots);
        assert_eq!(
            detect(env(&[("XDG_CURRENT_DESKTOP", "Hyprland")])),
            Desktop::Wlroots
        );
    }

    #[test]
    fn unknown_and_other() {
        assert_eq!(detect(env(&[])), Desktop::Unknown);
        assert_eq!(
            detect(env(&[("XDG_CURRENT_DESKTOP", "XFCE")])),
            Desktop::Other("XFCE".into())
        );
    }
}
