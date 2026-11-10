//! Logging for TankTerm.
//!
//! The main executable is supposed to call `initialize()` exactly once during
//! startup. All logging messages are written to stdout, given that their
//! log-level is sufficient for the level configured in `cli::Options`.

use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, LineWriter, Stdout, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use std::{env, process};

use log::{Level, LevelFilter};

use crate::cli::Options;

/// Logging target for config override error messages.
pub const LOG_TARGET_IPC_CONFIG: &str = "tankterm_log_window_config";

/// Name for the environment variable containing the log file's path.
const TANKTERM_LOG_ENV: &str = "TANKTERM_LOG";

/// Logging target for config error messages.
pub const LOG_TARGET_CONFIG: &str = "tankterm_config_derive";

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
    "tankterm_config_derive",
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
    format: LogFormat,
    /// Whether stdout gets styled text: it's a terminal and `NO_COLOR` isn't set.
    styled_stdout: bool,
    start: Instant,
}

impl Logger {
    fn new() -> Self {
        let logfile = Mutex::new(OnDemandLogFile::new());
        let stdout = Mutex::new(LineWriter::new(io::stdout()));
        let format = LogFormat::from_env();
        let styled_stdout = io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none();

        Logger { logfile, stdout, format, styled_stdout, start: Instant::now() }
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
        let line = LogLine {
            elapsed: self.start.elapsed(),
            level: record.level(),
            target,
            message: record.args().to_string(),
        };

        if let Ok(mut logfile) = self.logfile.lock() {
            // Write to logfile, never styled.
            let _ = logfile.write_all(line.format(self.format, false).as_bytes());
        }

        // Write to stdout.
        if let Ok(mut stdout) = self.stdout.lock() {
            let _ = stdout.write_all(line.format(self.format, self.styled_stdout).as_bytes());
        }
    }

    fn flush(&self) {}
}

/// Name for the environment variable choosing the log format.
const TANKTERM_LOG_FORMAT_ENV: &str = "TANKTERM_LOG_FORMAT";

/// Log output formats, like charmbracelet/log's formatters.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum LogFormat {
    #[default]
    Text,
    Json,
    Logfmt,
}

impl LogFormat {
    /// Format chosen with `TANKTERM_LOG_FORMAT`, defaulting to text.
    fn from_env() -> Self {
        env::var(TANKTERM_LOG_FORMAT_ENV).map(|format| Self::parse(&format)).unwrap_or_default()
    }

    fn parse(format: &str) -> Self {
        match format.to_lowercase().as_str() {
            "json" => Self::Json,
            "logfmt" => Self::Logfmt,
            _ => Self::Text,
        }
    }
}

/// A log record ready to be formatted.
struct LogLine<'a> {
    elapsed: Duration,
    level: Level,
    target: &'a str,
    message: String,
}

impl LogLine<'_> {
    /// Format the line, ending in a newline.
    ///
    /// Text follows charmbracelet/log's layout, `<time> <LEVEL> <prefix>: <message>`. When
    /// `styled`, the level is bold and colored and the prefix and separator are faint, like its
    /// default styles; JSON and logfmt are never styled.
    fn format(&self, format: LogFormat, styled: bool) -> String {
        let time = format!("{}.{:0>9}s", self.elapsed.as_secs(), self.elapsed.subsec_nanos());
        let level = self.level.as_str().to_lowercase();

        match format {
            LogFormat::Json => {
                let line = serde_json::json!({
                    "time": time,
                    "level": level,
                    "prefix": self.target,
                    "msg": self.message,
                });
                format!("{line}\n")
            },
            LogFormat::Logfmt => format!(
                "time={time} level={level} prefix={} msg={}\n",
                logfmt_value(self.target),
                logfmt_value(&self.message),
            ),
            LogFormat::Text => self.format_text(&time, styled),
        }
    }

    fn format_text(&self, time: &str, styled: bool) -> String {
        let label = format!("{:<5}", self.level.as_str());
        let (level, prefix, separator) = if styled {
            (
                format!("\x1b[{}m{label}\x1b[0m", level_style(self.level)),
                format!("\x1b[1;2m{}\x1b[0m", self.target),
                "\x1b[2m:\x1b[0m",
            )
        } else {
            (label, self.target.to_owned(), ":")
        };

        let mut text = format!("{time} {level} {prefix}{separator} ");

        // Align lines after the first with the start of the message. The escape sequences take
        // no space, so measure the unstyled header.
        let alignment = time.len() + 1 + 5 + 1 + self.target.len() + 2;
        let mut lines = self.message.split('\n');
        text.push_str(lines.next().unwrap_or_default());
        for line in lines {
            text.push('\n');
            text.push_str(&" ".repeat(alignment));
            text.push_str(line);
        }
        text.push('\n');
        text
    }
}

/// SGR parameters for a level label: bold, with charmbracelet/log's ANSI 256 colors.
fn level_style(level: Level) -> &'static str {
    match level {
        Level::Error => "1;38;5;204",
        Level::Warn => "1;38;5;192",
        Level::Info => "1;38;5;86",
        Level::Debug => "1;38;5;63",
        // charmbracelet/log has no trace level; show it like debug, but faint.
        Level::Trace => "1;2;38;5;63",
    }
}

/// Quote a logfmt value when it contains spaces, quotes, `=` or isn't printable.
fn logfmt_value(value: &str) -> String {
    let plain = !value.is_empty()
        && value.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '=' && c != '\\');
    if plain { value.to_owned() } else { format!("{value:?}") }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(level: Level, message: &str) -> LogLine<'static> {
        LogLine { elapsed: Duration::new(1, 5), level, target: "tankTerm", message: message.into() }
    }

    #[test]
    fn plain_text_follows_charm_layout() {
        let text = line(Level::Info, "Welcome").format(LogFormat::Text, false);
        assert_eq!(text, "1.000000005s INFO  tankTerm: Welcome\n");
    }

    #[test]
    fn multiline_messages_are_aligned() {
        let text = line(Level::Warn, "first\nsecond").format(LogFormat::Text, false);
        let header = "1.000000005s WARN  tankTerm: ";
        assert_eq!(text, format!("{header}first\n{}second\n", " ".repeat(header.len())));
    }

    #[test]
    fn styled_text_uses_charm_level_colors() {
        let text = line(Level::Error, "boom").format(LogFormat::Text, true);

        assert_eq!(
            text,
            "1.000000005s \x1b[1;38;5;204mERROR\x1b[0m \x1b[1;2mtankTerm\x1b[0m\x1b[2m:\x1b[0m \
             boom\n"
        );
        assert!(line(Level::Info, "").format(LogFormat::Text, true).contains("38;5;86m"));
        assert!(line(Level::Warn, "").format(LogFormat::Text, true).contains("38;5;192m"));
        assert!(line(Level::Debug, "").format(LogFormat::Text, true).contains("38;5;63m"));
    }

    #[test]
    fn json_lines() {
        let json = line(Level::Info, "has \"quotes\"").format(LogFormat::Json, true);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(json.ends_with('\n') && !json.contains('\x1b'));
        assert_eq!(value["level"], "info");
        assert_eq!(value["prefix"], "tankTerm");
        assert_eq!(value["msg"], "has \"quotes\"");
        assert_eq!(value["time"], "1.000000005s");
    }

    #[test]
    fn logfmt_lines_quote_when_needed() {
        assert_eq!(
            line(Level::Debug, "plain").format(LogFormat::Logfmt, false),
            "time=1.000000005s level=debug prefix=tankTerm msg=plain\n"
        );
        assert_eq!(
            line(Level::Debug, "two words").format(LogFormat::Logfmt, false),
            "time=1.000000005s level=debug prefix=tankTerm msg=\"two words\"\n"
        );
        assert_eq!(logfmt_value("a=b"), "\"a=b\"");
        assert_eq!(logfmt_value(""), "\"\"");
    }

    #[test]
    fn format_names() {
        assert_eq!(LogFormat::parse("json"), LogFormat::Json);
        assert_eq!(LogFormat::parse("LogFmt"), LogFormat::Logfmt);
        assert_eq!(LogFormat::parse("text"), LogFormat::Text);
        assert_eq!(LogFormat::parse("unknown"), LogFormat::Text);
    }
}
