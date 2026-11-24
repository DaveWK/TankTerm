//! `Command` status segments: the first line a program prints, run every few seconds in the
//! focused pane's directory.
//!
//! Each run takes a thread of its own, and what the program printed comes back as a
//! [`Message::StatusOutput`]. The programs come from the configuration file and `-o` only,
//! never from what runs in a terminal.

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::channel::oneshot;
use log::{info, warn};

use super::{App, Message, PANE_ID_ENV};

/// Longest a program runs before it is killed, if its interval is longer.
const MAX_TIMEOUT: Duration = Duration::from_secs(10);

/// Most of a program's output that is read; the rest is thrown away.
const OUTPUT_LIMIT: u64 = 4096;

/// Most characters of a program's output that are shown.
const MAX_CHARS: usize = 200;

/// How often a running program is looked at to see whether it has exited.
const POLL: Duration = Duration::from_millis(10);

/// The program of a `Command` segment and how often it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusCommand {
    pub program: String,
    pub args: Vec<String>,
    pub interval: Duration,
}

impl StatusCommand {
    /// How long a run may take: the interval, up to 10 seconds.
    fn timeout(&self) -> Duration {
        self.interval.min(MAX_TIMEOUT)
    }
}

/// What a run of a segment's program gave.
#[derive(Clone, Debug)]
pub struct Output {
    /// The generation of the runs the run belongs to; see [`Commands`].
    generation: u64,
    /// The position of the segment among the status segments.
    index: usize,
    /// The first line of the output, cleaned up, or why there is none.
    result: Result<String, String>,
}

/// The runs of the `Command` segments' programs.
#[derive(Debug, Default)]
pub struct Commands {
    /// Counts the configurations that were read; what runs started for an earlier one give is
    /// dropped.
    generation: u64,
    /// One for every status segment, by position.
    runs: Vec<Run>,
}

/// The runs of one segment's program.
#[derive(Debug, Default)]
struct Run {
    /// When the program was started last.
    started: Option<Instant>,
    /// The terminal of the focused pane the program was started for last.
    pane: Option<u64>,
    /// The program runs.
    running: bool,
    /// What the program printed, while its last run worked. It stays while the next run is
    /// going.
    output: Option<String>,
    /// The last run failed.
    failed: bool,
}

impl Commands {
    /// Forget the runs, for a configuration that was read again. Programs that still run are
    /// left to finish, and what they print is dropped.
    pub fn reset(&mut self) {
        self.generation += 1;
        self.runs.clear();
    }

    /// What the program of the segment at `index` printed, while its last run worked.
    pub fn output(&self, index: usize) -> Option<&str> {
        self.runs.get(index)?.output.as_deref()
    }
}

impl App {
    /// Start the programs of the `Command` segments that are due: those that haven't run yet,
    /// started their interval ago, or ran for another pane than the focused one. A program
    /// that still runs isn't started again.
    pub(super) fn run_status_commands(&mut self) -> Task<Message> {
        let focused = self.focused_terminal();
        let now = Instant::now();
        let generation = self.commands.generation;
        self.commands.runs.resize_with(self.options.status.len(), Run::default);

        let mut tasks = Vec::new();
        for (index, segment) in self.options.status.iter().enumerate() {
            let Some(command) = &segment.command else { continue };
            let run = &mut self.commands.runs[index];
            let due = run.started.is_none_or(|started| now - started >= command.interval)
                || run.pane != focused;
            if run.running || !due {
                continue;
            }

            run.running = true;
            run.started = Some(now);
            run.pane = focused;
            let shell = focused.and_then(|id| self.terminals.get(&id));
            let directory = shell.and_then(|shell| shell.directory());
            tasks.push(start(command.clone(), directory, focused, generation, index));
        }

        Task::batch(tasks)
    }

    /// Take what a program printed. A program that failed hides its segment, and a warning
    /// says why the first time.
    pub(super) fn status_output(&mut self, output: Output) {
        let Output { generation, index, result } = output;
        if generation != self.commands.generation {
            return;
        }
        let Some(run) = self.commands.runs.get_mut(index) else { return };
        let command = self.options.status[index].command.as_ref();
        let program = command.map_or("", |command| command.program.as_str());

        run.running = false;
        match result {
            Ok(output) => {
                if run.failed {
                    info!("The status command {program:?} works again");
                }
                run.failed = false;
                run.output = Some(output);
            },
            Err(err) => {
                if !run.failed {
                    warn!("Hiding the status command {program:?}: {err}");
                }
                run.failed = true;
                run.output = None;
            },
        }
    }
}

/// Run a segment's program in a thread of its own, in `directory` and with the ID of the
/// focused pane in its environment.
fn start(
    command: StatusCommand,
    directory: Option<PathBuf>,
    pane: Option<u64>,
    generation: u64,
    index: usize,
) -> Task<Message> {
    let (sender, receiver) = oneshot::channel();
    let spawned = thread::Builder::new().name("status command".into()).spawn(move || {
        let _ = sender.send(run(&command, directory.as_deref(), pane));
    });

    // A thread that can't be started drops the sender, which ends the run.
    if let Err(err) = spawned {
        warn!("Unable to start a thread for a status command: {err}");
    }
    Task::perform(receiver, move |result| {
        let result = result.unwrap_or_else(|_| Err("the command's thread ended".into()));
        Message::StatusOutput(Output { generation, index, result })
    })
}

/// Run a program and return the first line it prints, cleaned up, or why there is none.
///
/// The program gets no input, and what it writes to stderr is dropped. A program that runs
/// longer than its timeout is killed, along with the processes it started.
fn run(
    command: &StatusCommand,
    directory: Option<&Path>,
    pane: Option<u64>,
) -> Result<String, String> {
    let mut process = Command::new(&command.program);
    process.args(&command.args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(directory) = directory {
        process.current_dir(directory);
    }
    if let Some(pane) = pane {
        process.env(PANE_ID_ENV, pane.to_string());
    }
    isolate(&mut process);
    let mut child = process.spawn().map_err(|err| err.to_string())?;

    let timeout = command.timeout();
    let deadline = Instant::now() + timeout;
    let timed_out = || format!("it ran longer than {} ms", timeout.as_millis());

    // The output is read in a thread of its own, so a program that keeps it open doesn't hold
    // the run up past the deadline.
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        let reader = move || {
            let mut stdout = stdout;
            let _ = sender.send(first_bytes(&mut stdout));
            // The rest is read and thrown away, so the program isn't stopped by a full pipe.
            let _ = io::copy(&mut stdout, &mut io::sink());
        };
        if let Err(err) = thread::Builder::new().name("status output".into()).spawn(reader) {
            kill(&mut child);
            return Err(err.to_string());
        }
    }
    let Ok(output) = receiver.recv_timeout(timeout) else {
        kill(&mut child);
        return Err(timed_out());
    };

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => {
                kill(&mut child);
                return Err(timed_out());
            },
            Err(err) => {
                kill(&mut child);
                return Err(err.to_string());
            },
        }
    };

    if !status.success() {
        return Err(format!("it {status}"));
    }
    Ok(first_line(&output))
}

/// The start of what `reader` gives, up to the end of the first line or [`OUTPUT_LIMIT`]
/// bytes.
fn first_bytes(reader: &mut impl Read) -> Vec<u8> {
    let mut reader = reader.take(OUTPUT_LIMIT);
    let mut output = Vec::new();
    let mut chunk = [0; 512];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                output.extend_from_slice(&chunk[..read]);
                if chunk[..read].contains(&b'\n') {
                    break;
                }
            },
            Err(err) if err.kind() == io::ErrorKind::Interrupted => (),
            Err(_) => break,
        }
    }
    output
}

/// The first line of a program's output as text: without escape sequences and control
/// characters, tabs as spaces, without spaces at either end, and at most [`MAX_CHARS`] long.
fn first_line(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    let line = text.split('\n').next().unwrap_or_default();

    let mut cleaned = String::new();
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\x1b' => skip_escape(&mut chars),
            '\t' => cleaned.push(' '),
            character if character.is_control() => (),
            character => cleaned.push(character),
        }
    }
    cleaned.trim().chars().take(MAX_CHARS).collect()
}

/// Skip the rest of an escape sequence, after its `ESC`.
fn skip_escape(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>) {
    match chars.next() {
        // CSI: parameters and intermediates, up to the final character.
        Some('[') => {
            for character in chars.by_ref() {
                if ('\x40'..='\x7e').contains(&character) {
                    break;
                }
            }
        },
        // OSC and the other strings: up to BEL or ST (`ESC \`).
        Some(']' | 'P' | 'X' | '^' | '_') => {
            while let Some(character) = chars.next() {
                match character {
                    '\x07' => break,
                    '\x1b' => {
                        chars.next_if_eq(&'\\');
                        break;
                    },
                    _ => (),
                }
            }
        },
        // Others: intermediates, then the final character.
        Some(' '..='/') => {
            while chars.next_if(|character| (' '..='/').contains(character)).is_some() {}
            chars.next();
        },
        _ => (),
    }
}

/// Start the program in a process group of its own, so it can be killed along with what it
/// started.
#[cfg(unix)]
fn isolate(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

/// Start a console program without a window.
#[cfg(windows)]
fn isolate(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn isolate(_command: &mut Command) {}

/// Kill a program that hasn't exited, and the processes of its group, and wait for it.
#[cfg(unix)]
fn kill(child: &mut Child) {
    // The program leads its process group until it is waited for, so the group is its own.
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        // SAFETY: `kill` takes any process group ID; a negative one names a group.
        unsafe { libc::kill(-group, libc::SIGKILL) };
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::tab_tests::{Warnings, app_with, has_text, perform, wait_until};
    use super::*;
    use crate::hotkeys::{AppAction, Hotkeys};

    fn sh(script: &str, interval: Duration) -> StatusCommand {
        let args = vec!["-c".into(), script.into(), "sh".into()];
        StatusCommand { program: "sh".into(), args, interval }
    }

    fn run_sh(script: &str) -> Result<String, String> {
        run(&sh(script, Duration::from_secs(5)), None, None)
    }

    #[test]
    fn the_first_line_is_shown() {
        assert_eq!(run_sh("echo hi").as_deref(), Ok("hi"));
        assert_eq!(
            run_sh("printf ' \\033[32mgreen\\033[0m\\tok \\nsecond\\n'").as_deref(),
            Ok("green ok")
        );
        assert_eq!(run_sh("true").as_deref(), Ok(""));

        // Long output is cut short, and what isn't read doesn't make the program fail.
        let long = run_sh("head -c 100000 /dev/zero | tr '\\0' x").unwrap();
        assert_eq!(long, "x".repeat(MAX_CHARS));
    }

    #[test]
    fn programs_run_in_the_directory_and_with_the_pane_given() {
        let directory = tempfile::tempdir().unwrap();
        let command = sh("echo \"$TANKTERM_PANE_ID $(pwd -P)\"", Duration::from_secs(5));
        let output = run(&command, Some(directory.path()), Some(7)).unwrap();

        let path = directory.path().canonicalize().unwrap();
        assert_eq!(output, format!("7 {}", path.display()));
    }

    #[test]
    fn failing_programs_say_why() {
        assert!(run_sh("echo out; exit 3").unwrap_err().contains('3'));

        let missing = StatusCommand {
            program: "tankterm-no-such-program".into(),
            args: Vec::new(),
            interval: Duration::from_secs(1),
        };
        assert!(run(&missing, None, None).is_err());
    }

    #[test]
    fn programs_that_run_too_long_are_killed_with_what_they_started() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("pid");
        // The shell waits for a `sleep` it started, which keeps the output open.
        let script = format!("sleep 10 & echo $! > '{}'; wait", pid_file.display());

        let started = Instant::now();
        let result = run(&sh(&script, Duration::from_millis(500)), None, None);
        assert!(result.unwrap_err().contains("longer than 500 ms"));
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());

        let pid: libc::pid_t = std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        // Nobody may be left to collect a killed process, like in a container whose first process
        // isn't an init, which leaves it as a zombie.
        let is_zombie = || {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            // The state is after the program's name, which is in parentheses.
            stat.rsplit(')').next().is_some_and(|rest| rest.trim_start().starts_with('Z'))
        };
        // SAFETY: Signal 0 only asks whether the process exists.
        let is_gone = || unsafe { libc::kill(pid, 0) } == -1;
        wait_until("the program's child kept running", || is_gone() || is_zombie());
    }

    #[test]
    fn output_is_cleaned_up() {
        assert_eq!(
            first_line(b"\x1b[1;31mred\x1b[0m \x1b]0;title\x07a\x1b]8;;x\x1b\\b\x1b(Bc\r\n2"),
            "red abc"
        );
        assert_eq!(first_line(b"\x07bell\x00 \xff"), "bell \u{fffd}");
        assert_eq!(first_line("é".repeat(300).as_bytes()), "é".repeat(MAX_CHARS));

        let mut long = io::repeat(b'x');
        assert_eq!(first_bytes(&mut long).len() as u64, OUTPUT_LIMIT);
        assert_eq!(first_bytes(&mut &b"one\ntwo\n"[..]), b"one\ntwo\n");
    }

    const CONFIG: &str = r#"
        [tabs]
        show = "Always"

        [[tabs.status]]
        segment = "Command"
        format = "<{output}>"
        command = { program = "sh", args = ["-c", "echo hi"] }
        interval = 60
    "#;

    fn output(app: &App, result: Result<&str, &str>) -> Message {
        let result = result.map(str::to_owned).map_err(str::to_owned);
        Message::StatusOutput(Output { generation: app.commands.generation, index: 0, result })
    }

    #[test]
    fn programs_run_when_due_and_one_at_a_time() {
        let mut app = app_with(CONFIG, Hotkeys::default());
        assert_eq!(app.segment_text(&app.options.status[0]), None);

        // The first update starts the program, and a second doesn't while it runs.
        let _ = app.update(Message::Tick);
        let started = app.commands.runs[0].started;
        assert!(app.commands.runs[0].running && started.is_some());
        let _ = app.update(Message::Tick);
        assert_eq!(app.commands.runs[0].started, started);

        // What it printed is shown, and isn't parsed as a format.
        let _ = app.update(output(&app, Ok("{output} hi")));
        assert!(!app.commands.runs[0].running);
        assert_eq!(app.segment_text(&app.options.status[0]).as_deref(), Some("<{output} hi>"));
        assert!(has_text(&app, "<{output} hi>"));

        // Before its interval is over it isn't started again.
        let _ = app.update(Message::Tick);
        assert_eq!(app.commands.runs[0].started, started);

        // It is after that, and the last output stays while it runs.
        app.commands.runs[0].started = Some(Instant::now() - Duration::from_secs(61));
        let _ = app.update(Message::Tick);
        assert!(app.commands.runs[0].running);
        assert_eq!(app.segment_text(&app.options.status[0]).as_deref(), Some("<{output} hi>"));

        // Another pane taking the focus starts it again, for that pane, once it has finished.
        let _ = app.update(output(&app, Ok("hi")));
        perform(&mut app, &[AppAction::NewTab]);
        assert!(app.commands.runs[0].running);
        assert_eq!(app.commands.runs[0].pane, app.focused_terminal());
    }

    #[test]
    fn failing_programs_hide_their_segment_and_are_reported_once() {
        Warnings::keep();
        let mut app = app_with(CONFIG, Hotkeys::default());
        let _ = app.update(Message::Tick);
        let _ = app.update(output(&app, Ok("hi")));
        assert!(has_text(&app, "<hi>"));

        for _ in 0..2 {
            let _ = app.update(output(&app, Err("it failed in a test")));
            assert_eq!(app.segment_text(&app.options.status[0]), None);
        }
        assert_eq!(Warnings::mentioning("it failed in a test"), 1);

        let _ = app.update(output(&app, Ok("back")));
        assert!(has_text(&app, "<back>"));
    }

    #[test]
    fn output_of_runs_for_an_earlier_configuration_is_dropped() {
        let mut app = app_with(CONFIG, Hotkeys::default());
        let _ = app.update(Message::Tick);
        let stale = output(&app, Ok("stale"));

        app.commands.reset();
        let _ = app.update(stale);
        assert_eq!(app.segment_text(&app.options.status[0]), None);
        assert!(app.commands.runs[0].running);
    }
}
