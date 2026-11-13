use std::cmp::max;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueHint};
use log::{LevelFilter, error};
use serde::{Deserialize, Serialize};
use tankterm_config::SerdeReplace;
use toml::Value;

use tankterm_terminal::tty::Options as PtyOptions;

use crate::color::ColorContext;
use crate::config::UiConfig;
use crate::config::ui_config::{Program, accessible_env};
use crate::config::window::{Class, Identity};
use crate::logging::LOG_TARGET_IPC_CONFIG;

/// CLI options for the main TankTerm executable.
#[derive(Parser, Default, Debug)]
#[clap(name = "tankTerm", author, about = "A slopped together frankenterminal shamelessly forked by a clanker", version = env!("VERSION"))]
pub struct Options {
    /// Specify alternative configuration file [default:
    /// $XDG_CONFIG_HOME/tankterm/tankterm.toml].
    #[cfg(not(any(target_os = "macos", windows)))]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Specify alternative configuration file [default: %APPDATA%\tankterm\tankterm.toml].
    #[cfg(windows)]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Specify alternative configuration file [default: $HOME/.config/tankterm/tankterm.toml].
    #[cfg(target_os = "macos")]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Path for IPC socket creation.
    #[cfg(unix)]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub socket: Option<PathBuf>,

    /// Reduces the level of verbosity (the min level is -qq).
    #[clap(short, conflicts_with("verbose"), action = ArgAction::Count)]
    quiet: u8,

    /// Increases the level of verbosity (the max level is -vvv).
    #[clap(short, conflicts_with("quiet"), action = ArgAction::Count)]
    verbose: u8,

    /// Do not spawn an initial window.
    #[clap(long)]
    pub daemon: bool,

    /// Theme to use, by built-in name or file path. Overrides the configuration's theme.
    #[clap(long, env = "TANKTERM_THEME", value_name = "NAME|PATH")]
    pub theme: Option<String>,

    /// CLI options for config overrides.
    #[clap(skip)]
    pub config_options: ParsedOptions,

    /// Options which can be passed via IPC.
    #[clap(flatten)]
    pub window_options: WindowOptions,

    /// Subcommand passed to the CLI.
    #[clap(subcommand)]
    pub subcommands: Option<Subcommands>,
}

impl Options {
    pub fn new() -> Self {
        let mut options = Self::parse();

        // Parse CLI config overrides.
        options.config_options = options.window_options.config_overrides();

        options
    }

    /// Override configuration file with options from the CLI.
    pub fn override_config(&mut self, config: &mut UiConfig) {
        #[cfg(unix)]
        if self.socket.is_some() {
            config.ipc_socket = Some(true);
        }

        config.debug.log_level = max(config.debug.log_level, self.log_level());

        // Replace CLI options.
        self.config_options.override_config(config);
    }

    /// Logging filter level.
    pub fn log_level(&self) -> LevelFilter {
        match (self.quiet, self.verbose) {
            // Default.
            (0, 0) => LevelFilter::Warn,

            // Verbose.
            (_, 1) => LevelFilter::Info,
            (_, 2) => LevelFilter::Debug,
            (0, _) => LevelFilter::Trace,

            // Quiet.
            (1, _) => LevelFilter::Error,
            (..) => LevelFilter::Off,
        }
    }
}

/// Parse the class CLI parameter.
fn parse_class(input: &str) -> Result<Class, String> {
    let (general, instance) = match input.split_once(',') {
        // Warn the user if they've passed too many values.
        Some((_, instance)) if instance.contains(',') => {
            return Err(String::from("Too many parameters"));
        },
        Some((general, instance)) => (general, instance),
        None => (input, input),
    };

    Ok(Class::new(general, instance))
}

/// Terminal specific cli options which can be passed to new windows via IPC.
#[derive(Serialize, Deserialize, Args, Default, Debug, Clone, PartialEq, Eq)]
pub struct TerminalOptions {
    /// Start the shell in the specified working directory.
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub working_directory: Option<PathBuf>,

    /// Remain open after child process exit.
    #[clap(long)]
    pub hold: bool,

    /// Command and args to execute (must be last argument).
    #[clap(short = 'e', long, allow_hyphen_values = true, num_args = 1..)]
    command: Vec<String>,
}

impl TerminalOptions {
    /// Shell override passed through the CLI.
    pub fn command(&self) -> Option<Program> {
        let (program, args) = self.command.split_first()?;
        Some(Program::WithArgs { program: program.clone(), args: args.to_vec() })
    }

    /// Override the [`PtyOptions`]'s fields with the [`TerminalOptions`].
    pub fn override_pty_config(&self, pty_config: &mut PtyOptions) {
        if let Some(working_directory) = &self.working_directory {
            if working_directory.is_dir() {
                pty_config.working_directory = Some(working_directory.to_owned());
            } else {
                error!("Invalid working directory: {working_directory:?}");
            }
        }

        if let Some(command) = self.command() {
            pty_config.shell = Some(command.into());
        }

        pty_config.drain_on_exit |= self.hold;
    }
}

impl From<TerminalOptions> for PtyOptions {
    fn from(mut options: TerminalOptions) -> Self {
        PtyOptions {
            working_directory: options.working_directory.take(),
            shell: options.command().map(Into::into),
            drain_on_exit: options.hold,
            env: HashMap::new(),
            #[cfg(target_os = "windows")]
            escape_args: false,
        }
    }
}

/// Window specific cli options which can be passed to new windows via IPC.
#[derive(Serialize, Deserialize, Args, Default, Debug, Clone, PartialEq, Eq)]
pub struct WindowIdentity {
    /// Defines the window title [default: TankTerm].
    #[clap(short = 'T', short_alias('t'), long)]
    pub title: Option<String>,

    /// Defines window class/app_id on X11/Wayland [default: TankTerm].
    #[clap(long, value_name = "general> | <general>,<instance", value_parser = parse_class)]
    pub class: Option<Class>,
}

impl WindowIdentity {
    /// Override the [`Identity`]'s fields with the [`WindowIdentity`].
    pub fn override_identity_config(&self, identity: &mut Identity) {
        if let Some(title) = &self.title {
            identity.title.clone_from(title);
        }
        if let Some(class) = &self.class {
            identity.class.clone_from(class);
        }
    }
}

/// Available CLI subcommands.
#[derive(Subcommand, Debug)]
pub enum Subcommands {
    #[cfg(unix)]
    Msg(MessageOptions),
    Migrate(MigrateOptions),
}

/// Send a message to the TankTerm socket.
#[cfg(unix)]
#[derive(Args, Debug)]
pub struct MessageOptions {
    /// IPC socket connection path override.
    #[clap(short, long, value_hint = ValueHint::FilePath)]
    pub socket: Option<PathBuf>,

    /// Message which should be sent.
    #[clap(subcommand)]
    pub message: SocketMessage,
}

/// Available socket messages.
#[cfg(unix)]
#[derive(Subcommand, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SocketMessage {
    /// List the tabs and their panes.
    List(ListOptions),

    /// Open a new tab and print the ID of its pane.
    Spawn(SpawnOptions),

    /// Split a pane and print the ID of the new pane.
    SplitPane(SplitOptions),

    /// Send text to a pane, as if it was pasted.
    SendText(SendTextOptions),

    /// Print the text of a pane.
    GetText(GetTextOptions),

    /// Focus a pane and show its tab.
    ActivatePane(PaneOptions),

    /// Show a tab.
    ActivateTab(ActivateTabOptions),

    /// Close a pane.
    KillPane(PaneOptions),

    /// Zoom a pane, so that it fills its tab, or unzoom it.
    ZoomPane(ZoomOptions),

    /// Name a tab, in place of the title of its focused pane.
    SetTabTitle(SetTabTitleOptions),

    /// Create a new window in the same TankTerm process (not supported yet).
    CreateWindow(WindowOptions),

    /// Update the TankTerm configuration (not supported yet).
    Config(IpcConfig),

    /// Read runtime TankTerm configuration (not supported yet).
    GetConfig(IpcGetConfig),
}

/// Parameters to the `list` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct ListOptions {
    /// How to print the panes.
    #[clap(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}

/// How `tankTerm msg list` prints.
#[cfg(unix)]
#[derive(clap::ValueEnum, Serialize, Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// A table, one pane per line.
    #[default]
    Table,
    Json,
}

/// What a new tab or pane runs, and where.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct ProgramOptions {
    /// Start the program in this directory [default: as for a new tab or pane].
    #[clap(long, value_hint = ValueHint::DirPath)]
    pub cwd: Option<PathBuf>,

    /// Program and arguments to run [default: the shell].
    #[clap(trailing_var_arg = true, allow_hyphen_values = true, value_name = "PROGRAM")]
    pub command: Vec<String>,
}

/// Parameters to the `spawn` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct SpawnOptions {
    /// Open the tab from this profile.
    #[clap(long, conflicts_with = "command")]
    pub profile: Option<String>,

    #[clap(flatten)]
    pub program: ProgramOptions,
}

/// Parameters to the `split-pane` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct SplitOptions {
    /// Pane to split [default: $TANKTERM_PANE_ID, or the focused pane].
    #[clap(long, env = "TANKTERM_PANE_ID")]
    pub pane_id: Option<u64>,

    /// The same as `--right`.
    #[clap(long, conflicts_with_all = ["left", "right", "top", "bottom"])]
    pub horizontal: bool,

    /// Put the new pane left of the pane.
    #[clap(long, conflicts_with_all = ["right", "top", "bottom"])]
    pub left: bool,

    /// Put the new pane right of the pane.
    #[clap(long, conflicts_with_all = ["top", "bottom"])]
    pub right: bool,

    /// Put the new pane above the pane.
    #[clap(long, conflicts_with = "bottom")]
    pub top: bool,

    /// Put the new pane below the pane (the default).
    #[clap(long)]
    pub bottom: bool,

    /// Share of the space the new pane gets, in percent [default: 50].
    #[clap(long, value_parser = clap::value_parser!(u8).range(1..100))]
    pub percent: Option<u8>,

    #[clap(flatten)]
    pub program: ProgramOptions,
}

/// Parameters to the `send-text` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct SendTextOptions {
    /// Pane to send the text to [default: $TANKTERM_PANE_ID, or the focused pane].
    #[clap(long, env = "TANKTERM_PANE_ID")]
    pub pane_id: Option<u64>,

    /// Send the text as it is, instead of pasting it.
    #[clap(long)]
    pub no_paste: bool,

    /// Text to send [default: read from standard input].
    pub text: Option<String>,
}

/// Parameters to the `get-text` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct GetTextOptions {
    /// Pane to read [default: $TANKTERM_PANE_ID, or the focused pane].
    #[clap(long, env = "TANKTERM_PANE_ID")]
    pub pane_id: Option<u64>,

    /// First line to print: 0 is the top of the screen, negative lines are scrollback
    /// [default: 0].
    #[clap(long, allow_hyphen_values = true)]
    pub start_line: Option<i32>,

    /// Last line to print [default: the bottom of the screen].
    #[clap(long, allow_hyphen_values = true)]
    pub end_line: Option<i32>,
}

/// Parameters of IPC subcommands that act on a pane.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct PaneOptions {
    /// The pane [default: $TANKTERM_PANE_ID, or the focused pane].
    #[clap(long, env = "TANKTERM_PANE_ID")]
    pub pane_id: Option<u64>,
}

/// Parameters to the `activate-tab` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[clap(group(
    clap::ArgGroup::new("tab").required(true).args(["tab_id", "tab_index", "tab_relative"])
))]
pub struct ActivateTabOptions {
    /// The tab with this ID.
    #[clap(long, conflicts_with_all = ["tab_index", "tab_relative"])]
    pub tab_id: Option<u64>,

    /// The tab at this position, counting from 0; negative positions count from the last tab,
    /// which is -1.
    #[clap(long, allow_hyphen_values = true, conflicts_with = "tab_relative")]
    pub tab_index: Option<isize>,

    /// The tab this many tabs right of the shown one, or left of it for negative numbers,
    /// wrapping around at the ends.
    #[clap(long, allow_hyphen_values = true)]
    pub tab_relative: Option<isize>,

    /// Stop at the first or last tab instead of wrapping around.
    #[clap(long, conflicts_with_all = ["tab_id", "tab_index"])]
    pub no_wrap: bool,
}

/// Parameters to the `zoom-pane` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct ZoomOptions {
    /// The pane [default: $TANKTERM_PANE_ID, or the focused pane].
    #[clap(long, env = "TANKTERM_PANE_ID")]
    pub pane_id: Option<u64>,

    /// Zoom the pane (the default).
    #[clap(long, conflicts_with_all = ["unzoom", "toggle"])]
    pub zoom: bool,

    /// Unzoom the pane's tab.
    #[clap(long, conflicts_with = "toggle")]
    pub unzoom: bool,

    /// Zoom the pane, or unzoom its tab if it is zoomed.
    #[clap(long)]
    pub toggle: bool,
}

/// Parameters to the `set-tab-title` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct SetTabTitleOptions {
    /// A pane of the tab [default: $TANKTERM_PANE_ID, or the focused pane].
    #[clap(long, env = "TANKTERM_PANE_ID")]
    pub pane_id: Option<u64>,

    /// The tab with this ID, in place of a pane's tab.
    #[clap(long)]
    pub tab_id: Option<u64>,

    /// The new title; an empty title goes back to the title of the tab's focused pane.
    pub title: String,
}

/// Migrate the configuration file.
#[derive(Args, Clone, Debug)]
pub struct MigrateOptions {
    /// Path to the configuration file.
    #[clap(short, long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Only output TOML config to STDOUT.
    #[clap(short, long)]
    pub dry_run: bool,

    /// Do not recurse over imports.
    #[clap(short = 'i', long)]
    pub skip_imports: bool,

    /// Do not move renamed fields to their new location.
    #[clap(long)]
    pub skip_renames: bool,

    #[clap(short, long)]
    /// Do not output to STDOUT.
    pub silent: bool,
}

/// Subset of options that we pass to 'create-window' IPC subcommand.
#[derive(Serialize, Deserialize, Args, Default, Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    /// Terminal options which can be passed via IPC.
    #[clap(flatten)]
    pub terminal_options: TerminalOptions,

    #[clap(flatten)]
    /// Window options which could be passed via IPC.
    pub window_identity: WindowIdentity,

    #[clap(skip)]
    #[cfg(target_os = "macos")]
    /// The window tabbing identifier to use when building a window.
    pub window_tabbing_id: Option<String>,

    #[clap(skip)]
    #[cfg(not(any(target_os = "macos", windows)))]
    /// `ActivationToken` that we pass to winit.
    pub activation_token: Option<String>,

    /// Override configuration file options [example: 'cursor.style="Beam"'].
    #[clap(short = 'o', long, num_args = 1..)]
    option: Vec<String>,
}

impl WindowOptions {
    /// Get the parsed set of CLI config overrides.
    pub fn config_overrides(&self) -> ParsedOptions {
        ParsedOptions::from_options(&self.option)
    }
}

/// Parameters to the `config` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct IpcConfig {
    /// Configuration file options [example: 'cursor.style="Beam"'].
    #[clap(required = true, value_name = "CONFIG_OPTIONS")]
    pub options: Vec<String>,

    /// Window ID for the new config.
    ///
    /// Use `-1` to apply this change to all windows.
    #[clap(short, long, allow_hyphen_values = true, env = "TANKTERM_WINDOW_ID")]
    pub window_id: Option<i128>,

    /// Clear all runtime configuration changes.
    #[clap(short, long, conflicts_with = "options")]
    pub reset: bool,
}

/// Parameters to the `get-config` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct IpcGetConfig {
    /// Window ID for the config request.
    ///
    /// Use `-1` to get the global config.
    #[clap(short, long, allow_hyphen_values = true, env = "TANKTERM_WINDOW_ID")]
    pub window_id: Option<i128>,
}

/// Parsed CLI config overrides.
#[derive(Debug, Default)]
pub struct ParsedOptions {
    config_options: Vec<(String, Value)>,
}

impl ParsedOptions {
    /// Parse CLI config overrides.
    pub fn from_options(options: &[String]) -> Self {
        let mut config_options = Vec::new();

        for option in options {
            let parsed = match toml::from_str(option) {
                Ok(parsed) => parsed,
                Err(err) => {
                    eprintln!("Ignoring invalid CLI option '{option}': {err}");
                    continue;
                },
            };
            config_options.push((option.clone(), parsed));
        }

        Self { config_options }
    }

    /// Apply CLI config overrides, removing broken ones.
    ///
    /// ANSI colors and light/dark pairs in overrides resolve against the configuration's colors.
    pub fn override_config(&mut self, config: &mut UiConfig) {
        ColorContext::from_colors(&config.colors).scope(|| self.override_config_in_context(config));

        // Overrides can change styles and the colors they fall back to.
        config.style.resolve(&config.colors);

        // Reduced motion wins over anything the overrides set.
        config.apply_accessibility(accessible_env());
    }

    fn override_config_in_context(&mut self, config: &mut UiConfig) {
        let mut i = 0;
        while i < self.config_options.len() {
            let (option, parsed) = &self.config_options[i];
            match config.replace(parsed.clone()) {
                Err(err) => {
                    error!(
                        target: LOG_TARGET_IPC_CONFIG,
                        "Unable to override option '{option}': {err}"
                    );
                    self.config_options.swap_remove(i);
                },
                Ok(_) => i += 1,
            }
        }
    }
}

impl Deref for ParsedOptions {
    type Target = Vec<(String, Value)>;

    fn deref(&self) -> &Self::Target {
        &self.config_options
    }
}

impl DerefMut for ParsedOptions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.config_options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    use std::fs::File;
    #[cfg(target_os = "linux")]
    use std::io::Read;

    #[cfg(target_os = "linux")]
    use clap::CommandFactory;
    #[cfg(target_os = "linux")]
    use clap_complete::Shell;
    use toml::Table;

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let mut config = UiConfig::default();
        let old_dynamic_title = config.window.dynamic_title;

        Options::default().override_config(&mut config);

        assert_eq!(old_dynamic_title, config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_not_overridden_by_config() {
        let mut config = UiConfig::default();

        config.window.identity.title = "foo".to_owned();
        Options::default().override_config(&mut config);

        assert!(config.window.dynamic_title);
    }

    #[test]
    fn valid_option_as_value() {
        // Test with a single field.
        let value: Value = toml::from_str("field=true").unwrap();

        let mut table = Table::new();
        table.insert(String::from("field"), Value::Boolean(true));

        assert_eq!(value, Value::Table(table));

        // Test with nested fields
        let value: Value = toml::from_str("parent.field=true").unwrap();

        let mut parent_table = Table::new();
        parent_table.insert(String::from("field"), Value::Boolean(true));
        let mut table = Table::new();
        table.insert(String::from("parent"), Value::Table(parent_table));

        assert_eq!(value, Value::Table(table));
    }

    #[test]
    fn invalid_option_as_value() {
        let value = toml::from_str::<Value>("}");
        assert!(value.is_err());
    }

    #[test]
    fn float_option_as_value() {
        let value: Value = toml::from_str("float=3.4").unwrap();

        let mut expected = Table::new();
        expected.insert(String::from("float"), Value::Float(3.4));

        assert_eq!(value, Value::Table(expected));
    }

    #[test]
    fn parse_instance_class() {
        let class = parse_class("one").unwrap();
        assert_eq!(class.general, "one");
        assert_eq!(class.instance, "one");
    }

    #[test]
    fn parse_general_class() {
        let class = parse_class("one,two").unwrap();
        assert_eq!(class.general, "one");
        assert_eq!(class.instance, "two");
    }

    #[test]
    fn parse_invalid_class() {
        let class = parse_class("one,two,three");
        assert!(class.is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn completions() {
        let mut clap = Options::command();

        for (shell, file) in &[
            (Shell::Bash, "tankTerm.bash"),
            (Shell::Fish, "tankTerm.fish"),
            (Shell::Zsh, "_tankTerm"),
        ] {
            let mut generated = Vec::new();
            clap_complete::generate(*shell, &mut clap, "tankTerm", &mut generated);
            let generated = String::from_utf8_lossy(&generated);

            let mut completion = String::new();
            let mut file = File::open(format!("extra/completions/{file}")).unwrap();
            file.read_to_string(&mut completion).unwrap();

            assert_eq!(generated, completion);
        }

        // NOTE: Use this to generate new completions.
        //
        // let mut file = File::create("extra/completions/tankTerm.bash").unwrap();
        // clap_complete::generate(Shell::Bash, &mut clap, "tankTerm", &mut file);
        // let mut file = File::create("extra/completions/tankTerm.fish").unwrap();
        // clap_complete::generate(Shell::Fish, &mut clap, "tankTerm", &mut file);
        // let mut file = File::create("extra/completions/_tankTerm").unwrap();
        // clap_complete::generate(Shell::Zsh, &mut clap, "tankTerm", &mut file);
    }
}
