//! Logging setup shared by the core service, editor and CLI.
//!
//! Filter precedence: the `TPT_ZONES_LOG` environment variable, then the
//! `general.log_level` config value, then `"info"`.

use tracing_subscriber::{fmt, EnvFilter};

/// Environment variable that overrides the configured log level.
pub const ENV_VAR: &str = "TPT_ZONES_LOG";

/// Builds the filter from the environment or the supplied default.
///
/// Invalid directives fall back to `"info"` rather than failing startup.
#[must_use]
pub fn filter_from_env(default_level: &str) -> EnvFilter {
    EnvFilter::try_from_env(ENV_VAR)
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new("info"))
}

/// Installs a global `tracing` subscriber writing to stderr.
///
/// Safe to call more than once: later calls are ignored.
pub fn init(default_level: &str) {
    let _ = fmt()
        .with_env_filter(filter_from_env(default_level))
        .with_writer(std::io::stderr)
        .with_target(true)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_default_falls_back_without_panicking() {
        let _ = filter_from_env("this is ][ not a filter");
    }
}
