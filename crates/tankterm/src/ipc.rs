//! The TankTerm socket: `tankTerm msg` sends a message to the running TankTerm through it.
//!
//! A message is one line of JSON, answered with one line of JSON. The shells of a TankTerm find
//! its socket in `$TANKTERM_SOCKET` and their own pane in `$TANKTERM_PANE_ID`
//! ([`crate::app::PANE_ID_ENV`]).

use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{env, fs, process, thread};

use iced::Subscription;
use iced::futures::channel::mpsc;
use iced::futures::executor::block_on;
use iced::futures::stream::BoxStream;
use iced::futures::{SinkExt, StreamExt};
use log::warn;
use serde::{Deserialize, Serialize};

use crate::cli::{MessageOptions, OutputFormat, SocketMessage};

/// Environment variable with the path of the socket, for `tankTerm msg`.
pub const SOCKET_ENV: &str = "TANKTERM_SOCKET";

/// How long a connection may take to send its message.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The answer to a socket message.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SocketReply {
    /// The message was carried out.
    Done,
    /// The ID of the pane that was opened.
    Pane(u64),
    /// The panes of all tabs, tab by tab in the order of the tab bar.
    Panes(Vec<PaneInfo>),
    /// Lines of text.
    Text(Vec<String>),
    /// The message couldn't be carried out.
    Error(String),
}

/// A pane, as `tankTerm msg list` shows it.
///
/// The fields are named like those of `wezterm cli list --format json`, so scripts can read
/// both.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PaneInfo {
    /// Always 0: TankTerm has one window.
    pub window_id: u64,
    /// ID of the pane's tab, which stays the same while the tab is open.
    pub tab_id: u64,
    /// Position of the pane's tab, counting from 0.
    pub tab_index: usize,
    pub pane_id: u64,
    pub size: PaneSize,
    pub title: String,
    /// The shell's working directory, where it can be told; empty otherwise.
    pub cwd: String,
    pub tab_title: String,
    /// Whether the pane has the focus in its tab.
    pub is_active: bool,
    /// Whether the pane is zoomed to fill its tab.
    pub is_zoomed: bool,
    /// Whether the pane's tab is shown.
    pub is_active_tab: bool,
}

/// Size of a pane in cells.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneSize {
    pub rows: usize,
    pub cols: usize,
}

/// A message received on the socket, with the connection to answer it on.
#[derive(Debug, Clone)]
pub struct Request {
    pub message: SocketMessage,
    connection: Arc<UnixStream>,
}

impl Request {
    /// Answer the message. The reply is written from a thread of its own, so a client that is
    /// slow to read it holds up nobody.
    pub fn reply(self, reply: SocketReply) {
        thread::spawn(move || {
            if let Err(err) = write_reply(&self.connection, &reply) {
                warn!("Unable to answer a socket message: {err}");
            }
        });
    }
}

/// The socket, while it waits for connections.
///
/// Listeners are told apart by the path of their socket. The socket is removed again when the
/// last clone of the listener is dropped.
#[derive(Debug, Clone)]
pub struct Listener {
    socket: Arc<Socket>,
}

#[derive(Debug)]
struct Socket {
    path: PathBuf,
    /// Taken by the thread that accepts connections once the subscription starts.
    listener: Mutex<Option<UnixListener>>,
}

impl Drop for Socket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl Hash for Listener {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.socket.path.hash(state);
    }
}

impl Listener {
    /// Create the socket at `path`, or at a new path in the socket directory.
    pub fn bind(path: Option<PathBuf>) -> io::Result<Self> {
        let path = path.unwrap_or_else(|| {
            socket_dir().join(format!("{}-{}.sock", socket_prefix(), process::id()))
        });
        let listener = UnixListener::bind(&path)?;
        let socket = Socket { path, listener: Mutex::new(Some(listener)) };

        Ok(Self { socket: Arc::new(socket) })
    }

    pub fn path(&self) -> &Path {
        &self.socket.path
    }

    /// The messages that arrive on the socket.
    pub fn requests(&self) -> Subscription<Request> {
        Subscription::run_with(self.clone(), accept)
    }
}

/// Accept connections from a thread of its own and read their messages.
pub(crate) fn accept(listener: &Listener) -> BoxStream<'static, Request> {
    let (sender, receiver) = mpsc::channel(16);

    let socket = listener.socket.listener.lock().unwrap().take();
    if let Some(socket) = socket {
        thread::spawn(move || {
            for connection in socket.incoming() {
                let connection = match connection {
                    Ok(connection) => connection,
                    Err(err) => {
                        warn!("Unable to accept a socket connection: {err}");
                        continue;
                    },
                };

                // Nobody is listening once the application is gone.
                if !receive(connection, sender.clone()) {
                    break;
                }
            }
        });
    }

    receiver.boxed()
}

/// Read the message of a connection and hand it on, returning whether anybody still listens.
fn receive(connection: UnixStream, mut sender: mpsc::Sender<Request>) -> bool {
    let _ = connection.set_read_timeout(Some(READ_TIMEOUT));

    let mut line = String::new();
    if let Err(err) = BufReader::new(&connection).read_line(&mut line) {
        warn!("Unable to read a socket message: {err}");
        return true;
    }

    match serde_json::from_str(&line) {
        Ok(message) => {
            let request = Request { message, connection: Arc::new(connection) };
            block_on(sender.send(request)).is_ok()
        },
        Err(err) => {
            let reply = SocketReply::Error(format!("not a message TankTerm knows: {err}"));
            let _ = write_reply(&connection, &reply);
            true
        },
    }
}

fn write_reply(mut connection: &UnixStream, reply: &SocketReply) -> io::Result<()> {
    let mut json = serde_json::to_string(reply).map_err(io::Error::other)?;
    json.push('\n');
    connection.write_all(json.as_bytes())?;
    connection.flush()
}

/// Send a message to the running TankTerm and print its answer (`tankTerm msg`).
pub fn send(options: MessageOptions) -> Result<(), Box<dyn std::error::Error>> {
    let MessageOptions { socket, mut message } = options;

    // Paths are relative to where `tankTerm msg` runs, not to where TankTerm does.
    let program = match &mut message {
        SocketMessage::Spawn(options) => Some(&mut options.program),
        SocketMessage::SplitPane(options) => Some(&mut options.program),
        _ => None,
    };
    if let Some(cwd) = program.and_then(|program| program.cwd.as_mut()) {
        *cwd = std::path::absolute(&*cwd)?;
    }

    // Without text on the command line, the text comes from standard input.
    if let SocketMessage::SendText(options) = &mut message
        && options.text.is_none()
    {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        options.text = Some(text);
    }

    let json =
        matches!(&message, SocketMessage::List(options) if options.format == OutputFormat::Json);
    let reply = request(socket, &message)?;

    let mut stdout = io::stdout().lock();
    match reply {
        SocketReply::Done => (),
        SocketReply::Pane(id) => writeln!(stdout, "{id}")?,
        SocketReply::Panes(panes) if json => {
            writeln!(stdout, "{}", serde_json::to_string_pretty(&panes)?)?;
        },
        SocketReply::Panes(panes) => write!(stdout, "{}", table(&panes))?,
        SocketReply::Text(lines) => {
            for line in lines {
                writeln!(stdout, "{line}")?;
            }
        },
        SocketReply::Error(err) => return Err(err.into()),
    }

    Ok(())
}

/// Send a message to the socket at `socket`, or to the one TankTerm runs, and wait for the
/// answer.
pub fn request(socket: Option<PathBuf>, message: &SocketMessage) -> io::Result<SocketReply> {
    let mut connection = find_socket(socket)?;

    let mut json = serde_json::to_string(message)?;
    json.push('\n');
    connection.write_all(json.as_bytes())?;
    connection.flush()?;
    // Closing the writing end tells TankTerm the message is complete.
    connection.shutdown(Shutdown::Write)?;

    let mut line = String::new();
    BufReader::new(&connection).read_line(&mut line)?;
    if line.is_empty() {
        return Err(io::Error::new(ErrorKind::UnexpectedEof, "TankTerm didn't answer"));
    }

    serde_json::from_str(&line)
        .map_err(|err| io::Error::new(ErrorKind::InvalidData, format!("invalid answer: {err}")))
}

/// The panes as a table, like `wezterm cli list` prints it.
fn table(panes: &[PaneInfo]) -> String {
    let mut rows = vec![["TABID", "PANEID", "SIZE", "TITLE", "CWD"].map(String::from)];
    for pane in panes {
        rows.push([
            pane.tab_id.to_string(),
            pane.pane_id.to_string(),
            format!("{}x{}", pane.size.cols, pane.size.rows),
            pane.title.clone(),
            pane.cwd.clone(),
        ]);
    }

    let last = rows[0].len() - 1;
    let width = |column: usize| rows.iter().map(|row| row[column].chars().count()).max();
    let widths: Vec<usize> = (0..last).map(|column| width(column).unwrap_or(0)).collect();

    let mut table = String::new();
    for row in rows {
        let mut line = String::new();
        for (cell, width) in row[..last].iter().zip(&widths) {
            line.push_str(&format!("{cell:<width$}  "));
        }
        line.push_str(&row[last]);
        table.push_str(line.trim_end());
        table.push('\n');
    }

    table
}

/// Connect to the socket at `socket`, at `$TANKTERM_SOCKET`, or to the first one in the socket
/// directory that is still answered. Sockets nobody answers any longer are removed.
fn find_socket(socket: Option<PathBuf>) -> io::Result<UnixStream> {
    if let Some(path) = socket {
        return UnixStream::connect(&path).map_err(|err| {
            io::Error::new(err.kind(), format!("invalid socket path {}: {err}", path.display()))
        });
    }

    if let Some(path) = env::var_os(SOCKET_ENV)
        && let Ok(connection) = UnixStream::connect(path)
    {
        return Ok(connection);
    }

    let prefix = socket_prefix();
    let entries = fs::read_dir(socket_dir())?.filter_map(Result::ok);
    for path in entries.map(|entry| entry.path()) {
        let name = path.file_name().and_then(OsStr::to_str);
        if !name.is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".sock")) {
            continue;
        }

        match UnixStream::connect(&path) {
            Ok(connection) => return Ok(connection),
            Err(err) if err.kind() == ErrorKind::ConnectionRefused => {
                let _ = fs::remove_file(&path);
            },
            // Sockets of other users, for example.
            Err(_) => (),
        }
    }

    Err(io::Error::new(ErrorKind::NotFound, "no running TankTerm found"))
}

/// Directory the sockets are created in.
#[cfg(not(target_os = "macos"))]
fn socket_dir() -> PathBuf {
    let runtime = xdg::BaseDirectories::with_prefix("tankterm").get_runtime_directory().cloned();
    runtime.ok().filter(|path| fs::create_dir_all(path).is_ok()).unwrap_or_else(env::temp_dir)
}

#[cfg(target_os = "macos")]
fn socket_dir() -> PathBuf {
    env::temp_dir()
}

/// Start of the names of all sockets, with the display so that TankTerms on different display
/// servers of the same user don't take each other's.
#[cfg(not(target_os = "macos"))]
fn socket_prefix() -> String {
    let display = env::var("WAYLAND_DISPLAY").or_else(|_| env::var("DISPLAY")).unwrap_or_default();
    format!("TankTerm-{}", display.replace('/', "-"))
}

#[cfg(target_os = "macos")]
fn socket_prefix() -> String {
    String::from("TankTerm")
}

#[cfg(test)]
mod tests {
    use super::*;

    use iced::futures::executor::block_on_stream;

    use crate::cli::{ActivateTabOptions, GetTextOptions, ListOptions};

    #[test]
    fn messages_are_received_and_answered() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sock");
        let listener = Listener::bind(Some(path.clone())).unwrap();
        let mut requests = block_on_stream(accept(&listener));

        let message = SocketMessage::GetText(GetTextOptions {
            pane_id: Some(3),
            start_line: Some(-2),
            end_line: None,
        });
        let client = {
            let (path, message) = (path.clone(), message.clone());
            thread::spawn(move || request(Some(path), &message).unwrap())
        };

        let received = requests.next().unwrap();
        assert_eq!(received.message, message);
        received.reply(SocketReply::Text(vec!["one".into(), "two".into()]));
        assert_eq!(client.join().unwrap(), SocketReply::Text(vec!["one".into(), "two".into()]));

        // The socket is gone with the listener.
        drop(requests);
        drop(listener);
        assert!(!path.exists());
    }

    #[test]
    fn messages_tankterm_does_not_know_are_answered_with_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sock");
        let listener = Listener::bind(Some(path.clone())).unwrap();
        let _requests = accept(&listener);

        let mut connection = UnixStream::connect(&path).unwrap();
        connection.write_all(b"{\"Explode\":{}}\n").unwrap();
        connection.shutdown(Shutdown::Write).unwrap();

        let mut line = String::new();
        BufReader::new(&connection).read_line(&mut line).unwrap();
        let reply: SocketReply = serde_json::from_str(&line).unwrap();
        assert!(matches!(reply, SocketReply::Error(_)), "{reply:?}");
    }

    #[test]
    fn a_missing_socket_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let message = SocketMessage::List(ListOptions::default());
        let err = request(Some(directory.path().join("gone.sock")), &message).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(err.to_string().contains("gone.sock"));
    }

    #[test]
    fn panes_are_listed_in_columns() {
        let pane = |tab_id, pane_id, title: &str, cwd: &str| PaneInfo {
            window_id: 0,
            tab_id,
            tab_index: 0,
            pane_id,
            size: PaneSize { rows: 24, cols: 80 },
            title: title.into(),
            cwd: cwd.into(),
            tab_title: String::new(),
            is_active: false,
            is_zoomed: false,
            is_active_tab: false,
        };
        let panes = [
            pane(0, 0, "bash", "/home"),
            pane(1, 2, "a longer title", ""),
            pane(1, 14, "vim", "/tmp"),
        ];

        assert_eq!(
            table(&panes),
            concat!(
                "TABID  PANEID  SIZE   TITLE           CWD\n",
                "0      0       80x24  bash            /home\n",
                "1      2       80x24  a longer title\n",
                "1      14      80x24  vim             /tmp\n",
            )
        );
    }

    #[test]
    fn activate_tab_needs_exactly_one_tab() {
        use clap::Parser;

        use crate::cli::{Options, Subcommands};

        let parse = |args: &[&str]| {
            let args = ["tankTerm", "msg", "activate-tab"].iter().chain(args);
            Options::try_parse_from(args).map(|options| options.subcommands)
        };

        assert!(parse(&[]).is_err());
        assert!(parse(&["--tab-id", "1", "--tab-index", "2"]).is_err());
        assert!(parse(&["--tab-index", "1", "--no-wrap"]).is_err());
        let Ok(Some(Subcommands::Msg(options))) = parse(&["--tab-index", "-1"]) else { panic!() };
        let expected = ActivateTabOptions { tab_index: Some(-1), ..Default::default() };
        assert_eq!(options.message, SocketMessage::ActivateTab(expected));
    }
}
