//! `tankTerm migrate`: convert another terminal's configuration into TankTerm's.
//!
//! Every source turns its files into TankTerm configuration files and a report of what it
//! converted and skipped. The source files are never changed; the results go to TankTerm's
//! configuration directory through [`writer`].

use std::fmt::{self, Display, Formatter};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::cli::MigrateOptions;
use crate::config;

mod alacritty;
mod crush;
mod ghostty;
mod lua;
mod output;
mod tabby;
mod wezterm;
mod writer;

pub use output::Plan;

/// A terminal whose configuration `tankTerm migrate` converts.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    #[value(name = "alacritty")]
    Alacritty,
    #[value(name = "tabby")]
    Tabby,
    #[value(name = "wezterm")]
    WezTerm,
    #[value(name = "ghostty")]
    Ghostty,
    #[value(name = "crush")]
    Crush,
}

impl Display for Source {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Alacritty => "Alacritty",
            Self::Tabby => "Tabby",
            Self::WezTerm => "WezTerm",
            Self::Ghostty => "Ghostty",
            Self::Crush => "Crush",
        })
    }
}

impl Source {
    const ALL: [Source; 5] =
        [Self::Alacritty, Self::Tabby, Self::WezTerm, Self::Ghostty, Self::Crush];

    /// Where the source keeps its configuration, in the order it reads them.
    fn locations(self) -> Vec<PathBuf> {
        match self {
            Self::Alacritty => alacritty::locations(),
            Self::Tabby => tabby::locations(),
            Self::WezTerm => wezterm::locations(),
            Self::Ghostty => ghostty::locations(),
            Self::Crush => crush::locations(),
        }
    }

    /// The configuration file the source reads, if it has one.
    fn find(self) -> Option<PathBuf> {
        self.locations().into_iter().find(|path| path.is_file())
    }

    /// Guess the source of a configuration file from its name and contents.
    fn guess(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let parent = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if name.trim_start_matches('.').starts_with("alacritty") || parent == "alacritty" {
            return Some(Self::Alacritty);
        }
        if name.ends_with(".lua") || parent == "wezterm" {
            return Some(Self::WezTerm);
        }
        if parent == "tabby" {
            return Some(Self::Tabby);
        }
        if name.ends_with(".ghostty") || parent == "ghostty" {
            return Some(Self::Ghostty);
        }
        if name.ends_with(".json") {
            return Some(Self::Crush);
        }

        // Fall back to the contents.
        let contents = fs::read_to_string(path).ok()?;
        if contents.contains("wezterm") {
            return Some(Self::WezTerm);
        }
        if name.ends_with(".yaml") && contents.contains("hotkeys:") {
            return Some(Self::Tabby);
        }
        if name.ends_with(".toml") || name.ends_with(".yml") || name.ends_with(".yaml") {
            return Some(Self::Alacritty);
        }
        let ghostty_like = contents.lines().map(str::trim).any(|line| {
            line.starts_with("font-family")
                || line.starts_with("keybind")
                || line.starts_with("theme")
        });
        ghostty_like.then_some(Self::Ghostty)
    }

    /// Plan the TankTerm files for the source's configuration at `path`.
    fn plan(self, path: &Path, target: &Path, options: &MigrateOptions) -> Result<Plan, String> {
        let detected = options.config_file.is_none();
        match self {
            Self::Alacritty => alacritty::plan(path, target, options.skip_imports),
            Self::Tabby => tabby::plan(path, target),
            Self::WezTerm => wezterm::plan(path, target),
            Self::Ghostty => ghostty::plan(path, target, detected),
            Self::Crush => crush::plan(path, target, detected),
        }
    }
}

/// Handle `tankTerm migrate`.
pub fn migrate(options: MigrateOptions) {
    let stdin = io::stdin();
    let mut input = stdin.is_terminal().then(|| stdin.lock());
    let input = input.as_mut().map(|input| input as &mut dyn BufRead);

    if let Err(err) = run(&options, input) {
        eprintln!("tankTerm migrate: {err}");
        std::process::exit(1);
    }
}

fn run(options: &MigrateOptions, mut input: Option<&mut dyn BufRead>) -> Result<(), String> {
    let (source, path) =
        choose_source(options, input.as_mut().map(|input| &mut **input as &mut dyn BufRead))?;

    let target = config::default_config_path()
        .ok_or("can't find TankTerm's configuration directory; is $HOME set?")?;

    let plan = source.plan(&path, &target, options)?;

    if !options.silent {
        println!("Converting {source}'s {}", path.display());

        // The new file is read before any other TankTerm configuration file.
        let current = config::installed_config("toml").or_else(|| config::installed_config("yml"));
        if let Some(current) = current {
            println!("Current TankTerm configuration: {}", current.display());
            if current != target {
                println!(
                    "TankTerm reads {} first, so it takes the place of that file",
                    target.display()
                );
            }
        }
    }

    if plan.report.converted.is_empty() && plan.report.skipped.is_empty() {
        if !options.silent {
            println!("Nothing to convert; nothing was written.");
        }
        return Ok(());
    }

    writer::write(&plan, options, input)
}

/// The source and file to convert: from the options, or the one found in the standard locations.
fn choose_source(
    options: &MigrateOptions,
    input: Option<&mut dyn BufRead>,
) -> Result<(Source, PathBuf), String> {
    if let Some(path) = &options.config_file {
        if !path.is_file() {
            return Err(format!("{} doesn't exist", path.display()));
        }
        let source = options.from.or_else(|| Source::guess(path)).ok_or_else(|| {
            format!("can't tell which terminal {} is from; name it with --from", path.display())
        })?;
        let path = std::path::absolute(path).unwrap_or_else(|_| path.clone());
        return Ok((source, path));
    }

    if let Some(source) = options.from {
        let path = source.find().ok_or_else(|| {
            let locations = source.locations();
            let locations: Vec<_> =
                locations.iter().map(|path| format!("  {}", path.display())).collect();
            format!(
                "no {source} configuration found; looked in:\n{}\nName the file with --config-file",
                locations.join("\n")
            )
        })?;
        return Ok((source, path));
    }

    let mut found = found_sources();
    match found.len() {
        0 => Err("No configuration to convert found. Name a file with --config-file, or a \
                  terminal with --from to see where it was looked for."
            .into()),
        1 => Ok(found.remove(0)),
        _ => {
            let list: Vec<_> = found
                .iter()
                .enumerate()
                .map(|(i, (source, path))| format!("  {}) {source}: {}", i + 1, path.display()))
                .collect();
            let Some(input) = input else {
                return Err(format!(
                    "found several configurations:\n{}\nPick one with --from",
                    list.join("\n")
                ));
            };

            eprintln!("Found several configurations:\n{}", list.join("\n"));
            eprint!("Which one should be converted? [1-{}] ", found.len());
            let _ = io::stderr().flush();
            let answer = read_answer(input)?;
            match answer.parse::<usize>() {
                Ok(i) if (1..=found.len()).contains(&i) => Ok(found.remove(i - 1)),
                _ => Err("nothing chosen; nothing was written".into()),
            }
        },
    }
}

/// Every source that has a configuration file in its standard location.
fn found_sources() -> Vec<(Source, PathBuf)> {
    Source::ALL.iter().filter_map(|&source| Some((source, source.find()?))).collect()
}

/// A line suggesting `tankTerm migrate`, when other terminals' configurations exist.
///
/// This is logged when TankTerm starts without a configuration file of its own.
pub fn first_start_hint() -> Option<String> {
    hint(&found_sources())
}

/// The hint for the configurations found.
fn hint(found: &[(Source, PathBuf)]) -> Option<String> {
    if found.is_empty() {
        return None;
    }

    let mut found: Vec<_> =
        found.iter().map(|(source, path)| format!("{source} ({})", path.display())).collect();
    let last = found.pop()?;
    let list = match found.is_empty() {
        true => last,
        false => format!("{} or {last}", found.join(", ")),
    };
    Some(format!("`tankTerm migrate` can convert the configuration of {list} into TankTerm's"))
}

/// Read one trimmed line of the answer to a question.
fn read_answer(input: &mut dyn BufRead) -> Result<String, String> {
    let mut answer = String::new();
    input.read_line(&mut answer).map_err(|err| format!("can't read the answer: {err}"))?;
    Ok(answer.trim().to_owned())
}

/// Standard directories, from the environment.
mod dirs {
    use super::*;

    /// The home directory.
    pub fn home() -> Option<PathBuf> {
        home::home_dir()
    }

    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    #[cfg_attr(windows, allow(dead_code))]
    pub fn config_home() -> Option<PathBuf> {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| Some(home()?.join(".config")))
    }

    /// `$XDG_CONFIG_DIRS`, or `/etc/xdg`.
    #[cfg_attr(windows, allow(dead_code))]
    pub fn config_dirs() -> Vec<PathBuf> {
        match env::var("XDG_CONFIG_DIRS") {
            Ok(dirs) if !dirs.is_empty() => {
                dirs.split(':').map(PathBuf::from).filter(|path| path.is_absolute()).collect()
            },
            _ => vec![PathBuf::from("/etc/xdg")],
        }
    }

    /// `%APPDATA%` on Windows.
    #[cfg(windows)]
    pub fn app_data() -> Option<PathBuf> {
        env::var_os("APPDATA").map(PathBuf::from)
    }

    /// `~/Library/Application Support` on macOS.
    #[cfg(target_os = "macos")]
    pub fn app_support() -> Option<PathBuf> {
        Some(home()?.join("Library").join("Application Support"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_from_names() {
        let guess = |path: &str| Source::guess(Path::new(path));
        assert_eq!(guess("/x/alacritty/alacritty.toml"), Some(Source::Alacritty));
        assert_eq!(guess("/x/.alacritty.yml"), Some(Source::Alacritty));
        assert_eq!(guess("/x/alacritty.yml"), Some(Source::Alacritty));
        assert_eq!(guess("/x/.wezterm.lua"), Some(Source::WezTerm));
        assert_eq!(guess("/x/tabby/config.yaml"), Some(Source::Tabby));
        assert_eq!(guess("/x/ghostty/config"), Some(Source::Ghostty));
        assert_eq!(guess("/x/my.ghostty"), Some(Source::Ghostty));
        assert_eq!(guess("/x/crush/themes/mine.json"), Some(Source::Crush));
    }

    #[test]
    fn guess_from_contents() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, contents: &str| {
            let path = dir.path().join(name);
            fs::write(&path, contents).unwrap();
            Source::guess(&path)
        };

        assert_eq!(
            write("a.yaml", "terminal:\n  font: X\nhotkeys:\n  copy: []\n"),
            Some(Source::Tabby)
        );
        assert_eq!(write("b", "font-family = X\n"), Some(Source::Ghostty));
        assert_eq!(write("c", "local wezterm = require 'wezterm'\n"), Some(Source::WezTerm));
        assert_eq!(write("d.toml", "[window]\nopacity = 0.5\n"), Some(Source::Alacritty));
        assert_eq!(write("e", "nothing"), None);
    }

    #[test]
    fn first_start_hint_names_what_was_found() {
        assert_eq!(hint(&[]), None);
        let found = [
            (Source::Tabby, PathBuf::from("/h/.config/tabby/config.yaml")),
            (Source::Ghostty, PathBuf::from("/h/.config/ghostty/config")),
        ];
        assert_eq!(
            hint(&found).unwrap(),
            "`tankTerm migrate` can convert the configuration of Tabby \
             (/h/.config/tabby/config.yaml) or Ghostty (/h/.config/ghostty/config) into TankTerm's"
        );
    }
}
