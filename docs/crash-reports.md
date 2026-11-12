# Crash reports

tpt-zones can write a small report when it panics. The report stays on your machine. Nothing is
sent anywhere.

## Where

- Windows: `%LOCALAPPDATA%\tpt-zones\crashes\`
- Linux: `$XDG_STATE_HOME/tpt-zones/crashes/` (or `~/.local/state/tpt-zones/crashes/`)

The editor shows this folder in Settings, so you can find it and attach a report to an issue yourself.

## What a report contains

| Field       | Example                                  |
| ----------- | ---------------------------------------- |
| component   | `service`, `editor` or `cli`             |
| version     | `0.1.0`                                  |
| os          | `windows x86_64`                         |
| thread      | `zones-win-events`                       |
| location    | `src/events.rs:120:9`                    |
| message     | the panic message                        |

It does not contain window titles, process names, or your configuration. Those are never passed to
the panic handler. Keep it that way: a panic message must not include window titles or other
user data.

## Controls

- Settings → "Write local crash reports". Off means no files are written. The setting is stored in
  `config.toml` as `[general] crash_reports = false`.
- At most 20 reports are kept. Older ones are deleted when a new one is written.
- Panics still print to the terminal or log as before.

## Not done yet

Sending reports to a server is not implemented. It will be opt-in, default off, and go to a
Sentry-compatible endpoint chosen by the project. See `docs/security-review.md`.
