# Scripting: layout rules and the CLI

Written for: power users who want to automate window placement, and people writing scripts
around `tpt-zones-cli`.

## Rules

A rule maps a kind of window to a zone. Rules are checked in file order, and the **first**
match wins.

```toml
[[rules]]
name = "editor on main"        # optional label, shown in logs and output
process = "code.exe"           # executable name, case-insensitive
profile = "docked"             # optional: only while this profile is active
monitor = "DISPLAY1"           # optional: only on this monitor (stable name)
layout = "thirds"              # required: layout id
zone = "main"                  # required: zone id within that layout
```

### Conditions

| Field            | Matches                                             | Notes                          |
| ---------------- | --------------------------------------------------- | ------------------------------ |
| `process`        | Executable name, e.g. `code.exe`                    | Case-insensitive              |
| `class`          | Window class, e.g. `Chrome_WidgetWin_1`             | Case-insensitive              |
| `title_contains` | Substring of the window title                       | Case-insensitive              |
| `profile`        | The active profile                                  | Optional scope                |
| `monitor`        | Stable monitor name, e.g. `DISPLAY2`                | Optional scope                |

A rule needs at least one of `process`, `class` or `title_contains`. Without one, it would
match every window, and the config is rejected.

### Wildcards

`process` and `class` accept one `*` at the start or end:

| Pattern     | Matches                           | Does not match         |
| ----------- | --------------------------------- | ---------------------- |
| `code.exe`  | `code.exe`, `CODE.EXE`            | `vscode.exe`           |
| `code*`     | `code.exe`, `codex.exe`           | `vscode.exe`           |
| `*.exe`     | any `.exe`                        | `app.dll`              |
| `*`         | everything                        |                        |

A `*` in the middle is not supported. `a*b` is treated as an exact string.

### Stable monitor names

Monitors are named after their device, with the `\\.\` prefix removed. `\\.\DISPLAY1`
becomes `DISPLAY1`. These names survive reboots, unlike monitor ids, which change between
sessions. Use `tpt-zones-cli monitors` to see the names on your machine.

## Profiles

A profile binds a layout to each monitor by stable name. The active profile is set with
`active_profile` in the config, or overridden on the command line.

```toml
[[profiles]]
name = "docked"
monitors = { "DISPLAY1" = "thirds", "DISPLAY2" = "halves" }
```

## Command-line interface

```text
tpt-zones-cli validate [FILE]             check a config file (default: the user config)
tpt-zones-cli import FILE                 validate FILE and install it as the user config
tpt-zones-cli export FILE                 write the user config to FILE
tpt-zones-cli monitors                    list monitors
tpt-zones-cli windows                     list manageable windows
tpt-zones-cli apply [FILE] [--profile P]  apply placement rules once
```

The user config is `%APPDATA%\tpt-zones\config.toml` on Windows and
`$XDG_CONFIG_HOME/tpt-zones/config.toml` (or `~/.config/...`) elsewhere.

### Examples

Check the example config before installing it:

```powershell
tpt-zones-cli validate docs/examples/config.toml
```

Apply the `docked` profile from a script, for example from a scheduled task when you dock
the laptop:

```powershell
tpt-zones-cli apply docs/examples/config.toml --profile docked
```

Exit codes: `0` on success, `1` on runtime errors (including a failed validation), and `2`
on usage errors.

`apply` is idempotent. Windows already in their zone are skipped, so running it again is safe.

## Writing and checking rules

1. Run `tpt-zones-cli windows` and note the `process` and title of the window you want to match.
2. Add a rule with the most specific conditions you need.
3. Run `tpt-zones-cli validate FILE`. It reports unknown layouts, zones, profiles and rules
   without a condition.
4. Run `tpt-zones-cli apply FILE` and check the output.

The example file `docs/examples/config.toml` shows a complete config.
