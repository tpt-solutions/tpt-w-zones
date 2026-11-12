//! `tpt-zones-cli ctl`: talks to a running service over its named pipe.

use zones_core::Result;

#[cfg(windows)]
mod imp {
    use std::fs::{File, OpenOptions};
    use std::io::{BufRead, BufReader, Write};

    use zones_core::backend::WindowEvent;
    use zones_core::ipc::{self, Request, Response, PROTOCOL_VERSION};
    use zones_core::model::ZoneId;
    use zones_core::{Error, Result};
    use zones_win::pipe::PIPE_NAME;

    /// Connects, performs the handshake, and returns the open pipe.
    fn connect() -> Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(PIPE_NAME)
            .map_err(|e| {
                Error::Backend(format!(
                    "cannot reach the service ({e}); is `serve` running?"
                ))
            })?;
        let mut session = Session::new(file)?;
        session.send(&Request::Hello {
            client: "tpt-zones-cli".into(),
            protocol: PROTOCOL_VERSION,
        })?;
        match session.receive()? {
            Response::Hello { protocol, .. } => ipc::check_version(protocol)?,
            Response::Error { message } => return Err(Error::Ipc(message)),
            other => return Err(Error::Ipc(format!("unexpected handshake reply: {other:?}"))),
        }
        Ok(session.file)
    }

    struct Session {
        file: File,
        lines: std::io::Lines<BufReader<File>>,
    }

    impl Session {
        fn new(file: File) -> Result<Self> {
            let reader = file.try_clone().map_err(|e| Error::Ipc(e.to_string()))?;
            Ok(Self {
                file,
                lines: BufReader::new(reader).lines(),
            })
        }

        fn send(&mut self, request: &Request) -> Result<()> {
            let line = ipc::encode(request)?;
            writeln!(self.file, "{line}").map_err(|e| Error::Ipc(e.to_string()))
        }

        fn receive(&mut self) -> Result<Response> {
            let line = self
                .lines
                .next()
                .ok_or_else(|| Error::Ipc("service closed the connection".into()))?
                .map_err(|e| Error::Ipc(e.to_string()))?;
            ipc::decode(&line)
        }
    }

    fn request(req: &Request) -> Result<Response> {
        let mut session = Session::new(connect()?)?;
        session.send(req)?;
        session.receive()
    }

    pub fn run(args: &[String]) -> Result<()> {
        let arg = |i: usize| args.get(i).map(String::as_str);
        match arg(0) {
            Some("ping") => {
                let file = connect()?;
                drop(file);
                println!("service is up (protocol {PROTOCOL_VERSION})");
                Ok(())
            }
            Some("windows") => match request(&Request::ListWindows)? {
                Response::Windows { windows } => {
                    for w in windows {
                        println!("{:>10}  {:<20} {}", w.id.0, w.process, w.title);
                    }
                    Ok(())
                }
                other => unexpected(other),
            },
            Some("monitors") => match request(&Request::ListMonitors)? {
                Response::Monitors { monitors } => {
                    for m in monitors {
                        println!(
                            "{:<10} id={} work={}x{}+{}+{}",
                            m.name,
                            m.id.0,
                            m.work_area.width,
                            m.work_area.height,
                            m.work_area.x,
                            m.work_area.y
                        );
                    }
                    Ok(())
                }
                other => unexpected(other),
            },
            Some("reload") => ok_or_fail(request(&Request::ReloadConfig)?),
            Some("snap") => {
                let window: u64 = arg(1)
                    .and_then(|w| w.parse().ok())
                    .ok_or_else(|| Error::Ipc("snap needs a numeric WINDOW_ID".into()))?;
                let layout = arg(2).ok_or_else(|| Error::Ipc("snap needs LAYOUT".into()))?;
                let zone = arg(3).ok_or_else(|| Error::Ipc("snap needs ZONE".into()))?;
                ok_or_fail(request(&Request::SnapWindow {
                    window: zones_core::backend::WindowId(window),
                    layout: layout.to_owned(),
                    zone: ZoneId(zone.to_owned()),
                })?)
            }
            Some("watch") => watch(),
            _ => Err(Error::Ipc(
                "usage: ctl ping|windows|monitors|reload|snap WINDOW_ID LAYOUT ZONE|watch".into(),
            )),
        }
    }

    fn watch() -> Result<()> {
        let mut session = Session::new(connect()?)?;
        session.send(&Request::Subscribe)?;
        match session.receive()? {
            Response::Ok => {}
            other => return unexpected(other),
        }
        loop {
            match session.receive()? {
                Response::Event { event } => println!("{}", describe(&event)),
                other => return unexpected(other),
            }
        }
    }

    fn describe(event: &WindowEvent) -> String {
        match event {
            WindowEvent::Created(id) => format!("created   {}", id.0),
            WindowEvent::Destroyed(id) => format!("destroyed {}", id.0),
            WindowEvent::Focused(id) => format!("focused   {}", id.0),
            WindowEvent::Moved { id, rect } => format!(
                "moved     {} -> {}x{}+{}+{}",
                id.0, rect.width, rect.height, rect.x, rect.y
            ),
            WindowEvent::MinimizedChanged { id, minimized } => {
                format!("minimised {} = {minimized}", id.0)
            }
            WindowEvent::MonitorsChanged => "monitors changed".into(),
        }
    }

    fn ok_or_fail(response: Response) -> Result<()> {
        match response {
            Response::Ok => {
                println!("ok");
                Ok(())
            }
            other => unexpected(other),
        }
    }

    fn unexpected(response: Response) -> Result<()> {
        match response {
            Response::Error { message } => Err(Error::Backend(message)),
            other => Err(Error::Ipc(format!("unexpected reply: {other:?}"))),
        }
    }
}

#[cfg(windows)]
pub fn run(args: &[String]) -> Result<()> {
    imp::run(args)
}

#[cfg(not(windows))]
pub fn run(_args: &[String]) -> Result<()> {
    Err(zones_core::Error::Backend(
        "ctl needs the Windows service".into(),
    ))
}
