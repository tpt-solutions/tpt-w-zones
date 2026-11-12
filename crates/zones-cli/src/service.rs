//! The long-running tpt-zones service (`tpt-zones-cli serve`).
//!
//! One thread owns all window and hotkey state and processes a single queue of messages:
//!
//! - hotkey presses (from the hotkey thread),
//! - window events (from the event-hook thread),
//! - client requests (from the named-pipe server).
//!
//! Because every change goes through this loop, there is no shared mutable state to lock.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use zones_core::actions::Action;
use zones_core::backend::{HotkeyBackend, Subscription, WindowBackend, WindowEvent, WindowId};
use zones_core::config::Config;
use zones_core::ipc::{Request, Response, PROTOCOL_VERSION};
use zones_core::{engine, Error, Result};
use zones_win::pipe::{self, ClientMsg};
use zones_win::tray::{self, TrayCommand};
use zones_win::{autostart, window, WinBackend};

/// Delays after a new window appears before rules are re-applied. A new window often has
/// no title or is not yet visible on its first event, so one attempt is not enough.
const RULE_RETRY_DELAYS_MS: [u64; 2] = [250, 1000];

/// Everything the main loop reacts to.
enum Msg {
    Hotkey(Action),
    Event(WindowEvent),
    Client(ClientMsg),
    Tray(TrayCommand),
}

struct Service {
    backend: WinBackend,
    config_path: PathBuf,
    config: Config,
    tx: Sender<Msg>,
    /// Kept alive so the hotkeys stay registered; dropping them releases the keys.
    hotkeys: Vec<Subscription>,
    subscribers: Vec<Sender<WindowEvent>>,
}

/// Runs the service until the process is terminated.
///
/// # Errors
/// Returns an error if the config is invalid, or the event hooks or the pipe cannot start.
pub fn run(config_path: PathBuf) -> Result<()> {
    let config = load_or_default(&config_path)?;
    let (tx, rx) = mpsc::channel::<Msg>();

    let backend = WinBackend::new();
    let event_tx = tx.clone();
    let _event_subscription = backend.subscribe(Box::new(move |ev| {
        let _ = event_tx.send(Msg::Event(ev));
    }))?;

    let (client_tx, client_rx) = mpsc::channel::<ClientMsg>();
    let _pipe = pipe::serve(client_tx)?;
    spawn_forwarder(client_rx, tx.clone());

    // The tray is a convenience: if it cannot start, the service still runs.
    let (tray_tx, tray_rx) = mpsc::channel::<TrayCommand>();
    let _tray = match tray::start(tray_tx) {
        Ok(tray) => {
            tracing::info!("tray icon ready");
            let to_service = tx.clone();
            std::thread::spawn(move || {
                for cmd in tray_rx {
                    if to_service.send(Msg::Tray(cmd)).is_err() {
                        break;
                    }
                }
            });
            Some(tray)
        }
        Err(e) => {
            tracing::warn!(error = %e, "tray icon unavailable");
            None
        }
    };

    let mut service = Service {
        backend,
        config_path,
        config,
        tx,
        hotkeys: Vec::new(),
        subscribers: Vec::new(),
    };
    service.sync_autostart();
    service.register_hotkeys();
    service.apply_rules();

    tracing::info!(pipe = pipe::PIPE_NAME, "tpt-zones service running");
    service.event_loop(&rx);
    Ok(())
}

fn load_or_default(path: &Path) -> Result<Config> {
    if path.exists() {
        Config::load(path)
    } else {
        tracing::info!(path = %path.display(), "no config file; using defaults");
        Ok(Config::default())
    }
}

/// Moves client messages onto the service queue.
fn spawn_forwarder(from: Receiver<ClientMsg>, to: Sender<Msg>) {
    std::thread::spawn(move || {
        for msg in from {
            if to.send(Msg::Client(msg)).is_err() {
                break;
            }
        }
    });
}

impl Service {
    fn event_loop(&mut self, rx: &Receiver<Msg>) {
        for msg in rx {
            match msg {
                Msg::Hotkey(action) => self.run_action(&action),
                Msg::Event(event) => self.on_event(event),
                Msg::Client(ClientMsg::Subscribe(sink)) => self.subscribers.push(sink),
                Msg::Client(ClientMsg::Request(request, reply)) => {
                    let response = self.handle_request(request);
                    let _ = reply.send(response);
                }
                Msg::Tray(TrayCommand::Reload) => self.run_action(&Action::Reload),
                Msg::Tray(TrayCommand::ApplyRules) => self.run_action(&Action::ApplyRules),
                Msg::Tray(TrayCommand::Quit) => {
                    tracing::info!("quit requested from the tray");
                    break;
                }
            }
        }
    }

    fn on_event(&mut self, event: WindowEvent) {
        self.subscribers
            .retain(|sink| sink.send(event.clone()).is_ok());
        if let WindowEvent::Created(_) = event {
            self.schedule_rule_retries();
        }
    }

    fn schedule_rule_retries(&self) {
        for delay in RULE_RETRY_DELAYS_MS {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(delay));
                let _ = tx.send(Msg::Hotkey(Action::ApplyRules));
            });
        }
    }

    fn run_action(&mut self, action: &Action) {
        match action {
            Action::ApplyRules => self.apply_rules(),
            Action::Reload => {
                if let Err(e) = self.reload() {
                    tracing::warn!(error = %e, "reload failed; keeping previous config");
                }
            }
            Action::SnapFocused { layout, zone } => match window::foreground() {
                Some(id) => self.snap(id, layout, zone),
                None => tracing::debug!("snap requested but no window has focus"),
            },
        }
    }

    fn handle_request(&mut self, request: Request) -> Response {
        match request {
            Request::Hello { .. } => Response::Hello {
                server: "tpt-zones".into(),
                protocol: PROTOCOL_VERSION,
            },
            Request::ListWindows => match self.backend.windows() {
                Ok(windows) => Response::Windows { windows },
                Err(e) => error_response(&e),
            },
            Request::ListMonitors => match self.backend.monitors() {
                Ok(monitors) => Response::Monitors { monitors },
                Err(e) => error_response(&e),
            },
            Request::SnapWindow {
                window,
                layout,
                zone,
            } => match engine::snap(&self.backend, &self.config, window, &layout, &zone) {
                Ok(_) => Response::Ok,
                Err(e) => error_response(&e),
            },
            Request::ReloadConfig => match self.reload() {
                Ok(()) => Response::Ok,
                Err(e) => error_response(&e),
            },
            // Subscriptions are handled by the pipe thread and never reach here.
            Request::Subscribe => Response::Ok,
        }
    }

    fn snap(&self, id: WindowId, layout: &str, zone: &zones_core::model::ZoneId) {
        match engine::snap(&self.backend, &self.config, id, layout, zone) {
            Ok(rect) => tracing::info!(window = id.0, ?rect, "snapped window"),
            Err(e) => tracing::warn!(window = id.0, error = %e, "snap failed"),
        }
    }

    fn apply_rules(&self) {
        let profile = self.config.active_profile.as_deref();
        match engine::apply(&self.backend, &self.config, profile) {
            Ok(applied) if !applied.is_empty() => {
                tracing::info!(count = applied.len(), "applied placement rules");
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "could not apply rules"),
        }
    }

    /// Re-reads the config file and, if it is valid, switches to it. An invalid file leaves
    /// the running configuration untouched.
    fn reload(&mut self) -> Result<()> {
        let config = Config::load(&self.config_path)?;
        self.config = config;
        self.sync_autostart();
        self.register_hotkeys();
        self.apply_rules();
        tracing::info!("configuration reloaded");
        Ok(())
    }

    /// Registers every hotkey in the config, replacing any previous registrations.
    fn register_hotkeys(&mut self) {
        self.hotkeys.clear();
        for (name, hotkey) in &self.config.hotkeys {
            let action: Action = match name.parse() {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(hotkey = %name, error = %e, "skipping hotkey");
                    continue;
                }
            };
            let tx = self.tx.clone();
            let handler_action = action.clone();
            let handler = Box::new(move || {
                let _ = tx.send(Msg::Hotkey(handler_action.clone()));
            });
            match self.backend.register(hotkey, handler) {
                Ok(sub) => {
                    tracing::info!(%hotkey, %action, "hotkey registered");
                    self.hotkeys.push(sub);
                }
                Err(e) => tracing::warn!(%hotkey, %action, error = %e, "hotkey not registered"),
            }
        }
    }

    /// Makes the Run-at-logon entry match `general.autostart`.
    fn sync_autostart(&self) {
        let want = self.config.general.autostart;
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                tracing::warn!(error = %e, "cannot locate executable; autostart unchanged");
                return;
            }
        };
        match autostart::is_enabled() {
            Ok(have) if have != want => {
                if let Err(e) = autostart::set_enabled(want, &exe) {
                    tracing::warn!(error = %e, "could not update autostart");
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "could not read autostart"),
        }
    }
}

fn error_response(e: &Error) -> Response {
    Response::Error {
        message: e.to_string(),
    }
}
