//! Logging for TankTerm.
//!
//! The main executable is supposed to call `initialize()` exactly once during
//! startup. All logging messages are written to stdout, given that their
//! log-level is sufficient for the level configured in `cli::Options`.

use std::fs::{File, OpenOptions};
use std::io::{self, LineWriter, Stdout, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use std::{env, process};

use log::{Level, LevelFilter};

use crate::cli::Options;

/// Logging target for IPC config error messages.
pub const LOG_TARGET_IPC_CONFIG: &str = "alacritty_log_window_config";

/// Name for the environment variable containing the log file's path.
const TANKTERM_LOG_ENV: &str = "TANKTERM_LOG";

/// Logging target for config error messages.
pub const LOG_TARGET_CONFIG: &str = "alacritty_config_derive";

/// Logging target for winit events.
pub const LOG_TARGET_WINIT: &str = "alacritty_winit_event";

/// Name for the environment variable containing extra logging targets.
///
/// The targets are semicolon separated.
const TANKTERM_EXTRA_LOG_TARGETS_ENV: &str = "TANKTERM_EXTRA_LOG_TARGETS";

/// User configurable extra log targets to include.
fn extra_log_targets() -> &'static [String] {
    static EXTRA_LOG_TARGETS: OnceLock<Vec<String>> = OnceLock::new();

    EXTRA_LOG_TARGETS.get_or_init(|| {
        env::var(TANKTERM_EXTRA_LOG_TARGETS_ENV)
            .map_or(Vec::new(), |targets| targets.split(';').map(ToString::to_string).collect())
    })
}

/// List of targets which will be logged by TankTerm.
const ALLOWED_TARGETS: &[&str] = &[
    LOG_TARGET_IPC_CONFIG,
    LOG_TARGET_CONFIG,
    LOG_TARGET_WINIT,
    "alacritty_config_derive",
    "tankterm_terminal",
    env!("CARGO_CRATE_NAME"),
    "crossfont",
];

/// Initialize the logger to its defaults.
pub fn initialize(options: &Options) -> Result<Option<PathBuf>, log::SetLoggerError> {
    log::set_max_level(options.log_level());

    let logger = Logger::new();
    let path = logger.file_path();
    log::set_boxed_logger(Box::new(logger))?;

    Ok(path)
}

pub struct Logger {
    logfile: Mutex<OnDemandLogFile>,
    stdout: Mutex<LineWriter<Stdout>>,
    start: Instant,
}

impl Logger {
    fn new() -> Self {
        let logfile = Mutex::new(OnDemandLogFile::new());
        let stdout = Mutex::new(LineWriter::new(io::stdout()));

        Logger { logfile, stdout, start: Instant::now() }
    }

    fn file_path(&self) -> Option<PathBuf> {
        let logfile_lock = self.logfile.lock().ok()?;
        Some(logfile_lock.path().clone())
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        // Get target crate.
        let index = record.target().find(':').unwrap_or_else(|| record.target().len());
        let target = &record.target()[..index];

        // Only log our own crates, except when logging at Level::Trace.
        if !self.enabled(record.metadata()) || !is_allowed_target(record.level(), target) {
            return;
        }

        // Create log message for the given `record` and `target`.
        let message = create_log_message(record, target, self.start);

        // Write to logfile.
        if let Ok(mut logfile) = self.logfile.lock() {
            let _ = logfile.write_all(message.as_ref());
        }

        // Write to stdout.
        if let Ok(mut stdout) = self.stdout.lock() {
            let _ = stdout.write_all(message.as_ref());
        }
    }

    fn flush(&self) {}
}

fn create_log_message(record: &log::Record<'_>, target: &str, start: Instant) -> String {
    let runtime = start.elapsed();
    let secs = runtime.as_secs();
    let nanos = runtime.subsec_nanos();
    let mut message = format!("[{}.{:0>9}s] [{:<5}] [{}] ", secs, nanos, record.level(), target);

    // Alignment for the lines after the first new line character in the payload. We don't deal
    // with fullwidth/unicode chars here, so just `message.len()` is sufficient.
    let alignment = message.len();

    // Push lines with added extra padding on the next line, which is trimmed later.
    let lines = record.args().to_string();
    for line in lines.split('\n') {
        let line = format!("{}\n{:width$}", line, "", width = alignment);
        message.push_str(&line);
    }

    // Drop extra trailing alignment.
    message.truncate(message.len() - alignment);
    message
}

/// Check if log messages from a crate should be logged.
fn is_allowed_target(level: Level, target: &str) -> bool {
    match (level, log::max_level()) {
        (Level::Error, LevelFilter::Trace) | (Level::Warn, LevelFilter::Trace) => true,
        _ => ALLOWED_TARGETS.contains(&target) || extra_log_targets().iter().any(|t| t == target),
    }
}

struct OnDemandLogFile {
    file: Option<LineWriter<File>>,
    created: Arc<AtomicBool>,
    path: PathBuf,
}

impl OnDemandLogFile {
    fn new() -> Self {
        let mut path = env::temp_dir();
        path.push(format!("TankTerm-{}.log", process::id()));

        // Set log path as an environment variable.
        unsafe { env::set_var(TANKTERM_LOG_ENV, path.as_os_str()) };

        OnDemandLogFile { path, file: None, created: Arc::new(AtomicBool::new(false)) }
    }

    fn file(&mut self) -> Result<&mut LineWriter<File>, io::Error> {
        // Allow to recreate the file if it has been deleted at runtime.
        if self.file.is_some() && !self.path.as_path().exists() {
            self.file = None;
        }

        // Create the file if it doesn't exist yet.
        if self.file.is_none() {
            let file = OpenOptions::new().append(true).create_new(true).open(&self.path);

            match file {
                Ok(file) => {
                    self.file = Some(io::LineWriter::new(file));
                    self.created.store(true, Ordering::Relaxed);
                    let _ =
                        writeln!(io::stdout(), "Created log file at \"{}\"", self.path.display());
                },
                Err(e) => {
                    let _ = writeln!(io::stdout(), "Unable to create log file: {e}");
                    return Err(e);
                },
            }
        }

        Ok(self.file.as_mut().unwrap())
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Write for OnDemandLogFile {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.file()?.write(buf)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.file()?.flush()
    }
}
