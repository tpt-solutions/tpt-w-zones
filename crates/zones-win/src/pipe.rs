//! Named-pipe IPC server speaking `zones_core::ipc`.
//!
//! Each client connection gets its own thread. The thread checks the protocol handshake,
//! then forwards requests to the service over a channel and writes the replies. The
//! service itself owns all window and hotkey state; the pipe only moves messages.
//!
//! The pipe is created with the default security descriptor, which limits access to the
//! current user, the administrators group, and SYSTEM.

#![allow(unsafe_code)]

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use zones_core::backend::WindowEvent;
use zones_core::ipc::{self, Request, Response, PROTOCOL_VERSION};
use zones_core::{Error, Result};

/// Well-known pipe name for the service.
pub const PIPE_NAME: &str = r"\\.\pipe\tpt-zones";

/// Name the service reports in its handshake.
const SERVER_NAME: &str = "tpt-zones";

/// Messages from pipe clients to the service.
pub enum ClientMsg {
    /// A request expecting exactly one response on `reply`.
    Request(Request, Sender<Response>),
    /// The client wants window events; they are sent to this channel until it closes.
    Subscribe(Sender<WindowEvent>),
}

/// Starts the accept loop on a background thread. Client messages go to `to_service`.
///
/// # Errors
/// Returns [`Error::Backend`] if the accept thread cannot be started.
pub fn serve(to_service: Sender<ClientMsg>) -> Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("zones-win-pipe".into())
        .spawn(move || accept_loop(&to_service))
        .map_err(|e| Error::Backend(e.to_string()))
}

fn accept_loop(to_service: &Sender<ClientMsg>) {
    let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let security = match pipe_security() {
        Ok(sa) => sa,
        Err(e) => {
            tracing::error!(error = %e, "cannot build pipe security; not serving clients");
            return;
        }
    };
    loop {
        // SAFETY: `name` is a NUL-terminated wide string that outlives the call; the other
        // arguments are plain values, and a null security descriptor is allowed.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                Some(&security.attributes),
            )
        };
        if handle.is_invalid() {
            tracing::error!("CreateNamedPipeW failed; retrying");
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }

        // SAFETY: `handle` is a valid pipe instance created above; no overlapped I/O is used.
        let connected = unsafe { ConnectNamedPipe(handle, None) };
        // A client that connected between create and connect is fine.
        let already = unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected.is_err() && !already {
            close(handle);
            continue;
        }

        // SAFETY: ownership of the pipe handle moves into the File, which closes it on drop.
        let file = unsafe { File::from_raw_handle(handle.0) };
        let to = to_service.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("zones-win-pipe-client".into())
            .spawn(move || handle_client(file, &to))
        {
            tracing::warn!(error = %e, "could not start client thread");
        }
    }
}

/// Access list for the pipe: the creating user, Administrators and SYSTEM. Other local users
/// are denied, so the pipe is not a way to move windows or reload config for someone else.
const PIPE_SDDL: PCWSTR = windows::core::w!("D:(A;;GA;;;OW)(A;;GA;;;BA)(A;;GA;;;SY)");

struct PipeSecurity {
    attributes: SECURITY_ATTRIBUTES,
    descriptor: PSECURITY_DESCRIPTOR,
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: the descriptor was allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW,
        // which requires LocalFree, and it is freed exactly once here.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
    }
}

fn pipe_security() -> Result<PipeSecurity> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: the SDDL string is static and NUL-terminated; the output pointer is a valid local.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PIPE_SDDL,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(|e| Error::Backend(format!("building pipe security descriptor: {e}")))?;
    Ok(PipeSecurity {
        attributes: SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        },
        descriptor,
    })
}

fn close(handle: HANDLE) {
    // SAFETY: the handle came from CreateNamedPipeW and is closed exactly once here.
    let _ = unsafe { CloseHandle(handle) };
}

fn handle_client(mut file: File, to_service: &Sender<ClientMsg>) {
    if let Err(e) = session(&mut file, to_service) {
        tracing::debug!(error = %e, "client session ended");
    }
    // SAFETY: the raw handle belongs to `file`, which is still alive; disconnecting
    // before the File is dropped lets the next client reuse the pipe name cleanly.
    let _ = unsafe { DisconnectNamedPipe(HANDLE(file.as_raw_handle())) };
}

fn session(file: &mut File, to_service: &Sender<ClientMsg>) -> io::Result<()> {
    let reader = file.try_clone()?;
    let mut lines = BufReader::new(reader).lines();

    // Handshake: the first message must be Hello with a matching protocol version.
    let first = read_request(&mut lines)?;
    match first {
        Ok(Request::Hello { protocol, .. }) => {
            if let Err(e) = ipc::check_version(protocol) {
                write_response(
                    file,
                    &Response::Error {
                        message: e.to_string(),
                    },
                )?;
                return Ok(());
            }
            write_response(
                file,
                &Response::Hello {
                    server: SERVER_NAME.into(),
                    protocol: PROTOCOL_VERSION,
                },
            )?;
        }
        Ok(_) => {
            write_response(
                file,
                &Response::Error {
                    message: "first message must be hello".into(),
                },
            )?;
            return Ok(());
        }
        Err(message) => {
            write_response(file, &Response::Error { message })?;
            return Ok(());
        }
    }

    for line in lines {
        let line = line?;
        let request = match ipc::decode::<Request>(&line) {
            Ok(req) => req,
            Err(e) => {
                write_response(
                    file,
                    &Response::Error {
                        message: e.to_string(),
                    },
                )?;
                continue;
            }
        };

        if let Request::Subscribe = request {
            return stream_events(file, to_service);
        }

        let (reply_tx, reply_rx) = mpsc::channel();
        let response = match to_service.send(ClientMsg::Request(request, reply_tx)) {
            Ok(()) => reply_rx.recv().unwrap_or_else(|_| Response::Error {
                message: "service stopped".into(),
            }),
            Err(_) => Response::Error {
                message: "service stopped".into(),
            },
        };
        write_response(file, &response)?;
    }
    Ok(())
}

/// Reads one request line. The outer `io::Result` is transport failure; the inner
/// `Result` is a request that could not be understood, reported back to the client.
fn read_request(
    lines: &mut io::Lines<BufReader<File>>,
) -> io::Result<std::result::Result<Request, String>> {
    match lines.next() {
        None => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "client closed",
        )),
        Some(line) => Ok(ipc::decode::<Request>(&line?).map_err(|e| e.to_string())),
    }
}

fn stream_events(file: &mut File, to_service: &Sender<ClientMsg>) -> io::Result<()> {
    let (event_tx, event_rx) = mpsc::channel();
    to_service
        .send(ClientMsg::Subscribe(event_tx))
        .map_err(|_| io::Error::other("service stopped"))?;
    write_response(file, &Response::Ok)?;
    // The loop ends when the service drops the sender or a write fails (client gone).
    for event in event_rx {
        write_response(file, &Response::Event { event })?;
    }
    Ok(())
}

fn write_response(file: &mut File, response: &Response) -> io::Result<()> {
    let line = ipc::encode(response).map_err(|e| io::Error::other(e.to_string()))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()
}
