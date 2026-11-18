//! Shell integration for the shells TankTerm starts: bash, zsh and fish load the scripts in
//! `extra/shell-integration` without users sourcing them.
//!
//! Each shell is reached through a variable it reads its startup from: bash in POSIX mode reads
//! `ENV`, zsh reads its files from `ZDOTDIR` and fish its configuration snippets from
//! `XDG_DATA_DIRS`. The scripts these point at put the variables back and read the user's startup
//! files as the shell would have. The approach is Ghostty's.
//!
//! The scripts are written to the runtime directory once per process.

// How the arguments and variables of bash, zsh and fish are changed is ported from Ghostty's
// `src/termio/shell_integration.zig`, under this license:
//
// MIT License
//
// Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::ffi::OsStr;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::{env, fs, io, process};

use iced_tterm::settings::BackendSettings;
use log::warn;

/// The directory the scripts are in, for the shells and for users to source them by hand.
pub const DIRECTORY_ENV: &str = "TANKTERM_SHELL_INTEGRATION_DIR";

/// The scripts, by their path in the shell integration directory.
const SCRIPTS: &[(&str, &str)] = &[
    ("tankterm.bash", include_str!("../../../extra/shell-integration/tankterm.bash")),
    ("tankterm.zsh", include_str!("../../../extra/shell-integration/tankterm.zsh")),
    ("tankterm.fish", include_str!("../../../extra/shell-integration/tankterm.fish")),
    (
        "bash/tankterm-inject.bash",
        include_str!("../../../extra/shell-integration/bash/tankterm-inject.bash"),
    ),
    ("zsh/.zshenv", include_str!("../../../extra/shell-integration/zsh/.zshenv")),
    (
        "fish/vendor_conf.d/tankterm.fish",
        include_str!("../../../extra/shell-integration/fish/vendor_conf.d/tankterm.fish"),
    ),
];

/// What fish reads its configuration snippets from without `XDG_DATA_DIRS`.
const DEFAULT_XDG_DATA_DIRS: &str = "/usr/local/share:/usr/share";

/// The shells that get the integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// The shell a program is, by its file name.
    fn of(program: &str) -> Option<Self> {
        let name = Path::new(program).file_name().and_then(OsStr::to_str)?;
        // A login shell's name can start with a dash.
        match name.strip_prefix('-').unwrap_or(name) {
            "bash" => Some(Self::Bash),
            "zsh" => Some(Self::Zsh),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }
}

/// How to start a shell with the integration: its new arguments, and the variables to set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Injection {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// How to start `program` with `args` so that it loads the shell integration in `directory`, if
/// it is bash, zsh or fish and the arguments allow it.
///
/// `env` looks up the variables the shell will start with.
pub fn inject(
    program: &str,
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    directory: &Path,
) -> Option<Injection> {
    let shell = Shell::of(program)?;
    let mut injection = match shell {
        Shell::Bash => bash(program, args, env, directory)?,
        Shell::Zsh => zsh(args, env, directory)?,
        Shell::Fish => fish(args, env, directory)?,
    };

    let directory = directory.to_string_lossy().into_owned();
    injection.env.push((DIRECTORY_ENV.into(), directory));
    Some(injection)
}

/// bash in POSIX mode reads `ENV`, and the script there the startup files.
///
/// Shells that don't read their startup files aren't changed: those that run a command or a
/// script, and those in POSIX mode already.
fn bash(
    program: &str,
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    directory: &Path,
) -> Option<Injection> {
    // macOS comes with bash 3.2, too old for the scripts.
    if cfg!(target_os = "macos") && program == "/bin/bash" {
        return None;
    }

    // Long options have to come before short ones, so `--posix` goes first.
    let mut injected = vec![String::from("--posix")];
    // What the startup script has to know: the options that leave out startup files.
    let mut inject = String::from("1");
    let mut rcfile = None;
    // With `-s` the arguments after the options are the shell's, not a script.
    let mut from_stdin = false;

    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--posix" => return None,
            "--norc" | "--noprofile" => inject.push_str(&format!(" {arg}")),
            "--rcfile" | "--init-file" => rcfile = Some(args.next()?.clone()),
            "-" | "--" => {
                let rest: Vec<_> = args.by_ref().cloned().collect();
                if !rest.is_empty() && !from_stdin {
                    return None;
                }
                injected.push(arg.clone());
                injected.extend(rest);
            },
            long if long.starts_with("--") => injected.push(arg.clone()),
            short if short.len() > 1 && short.starts_with(['-', '+']) => {
                let letters = &short[1..];
                if short.starts_with('-') && letters.contains('c') {
                    return None;
                }
                from_stdin |= short.starts_with('-') && letters.contains('s');
                injected.push(arg.clone());

                // `-o` and `-O` take the name of an option.
                if letters.ends_with(['o', 'O']) {
                    let option = args.next()?;
                    if short.starts_with('-') && option == "posix" {
                        return None;
                    }
                    injected.push(option.clone());
                }
            },
            _ if from_stdin => injected.push(arg.clone()),
            // A script.
            _ => return None,
        }
    }

    let script = directory.join("bash/tankterm-inject.bash");
    let mut variables = vec![
        ("TANKTERM_BASH_INJECT".into(), inject),
        ("ENV".into(), script.to_string_lossy().into_owned()),
    ];
    if let Some(rcfile) = rcfile {
        variables.push(("TANKTERM_BASH_RCFILE".into(), rcfile));
    }
    if let Some(saved) = env("ENV") {
        variables.push(("TANKTERM_BASH_ENV".into(), saved));
    }
    // POSIX mode keeps the history in ~/.sh_history.
    if let (None, Some(home)) = (env("HISTFILE"), env("HOME")) {
        variables.push(("HISTFILE".into(), format!("{home}/.bash_history")));
        variables.push(("TANKTERM_BASH_UNEXPORT_HISTFILE".into(), "1".into()));
    }

    Some(Injection { args: injected, env: variables })
}

/// zsh reads its startup files from `ZDOTDIR`, where the `.zshenv` puts it back.
///
/// Shells that read no startup files aren't changed.
fn zsh(
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    directory: &Path,
) -> Option<Injection> {
    // Option names ignore case, `_` and `-`.
    let name = |option: &str| option.to_lowercase().replace(['_', '-'], "");
    let mut options = args.iter();
    while let Some(arg) = options.next() {
        if arg == "--" || arg == "-" {
            break;
        } else if let Some(long) = arg.strip_prefix("--") {
            if name(long) == "norcs" {
                return None;
            }
        } else if arg.len() > 1 && arg.starts_with(['-', '+']) {
            let (on, letters) = (arg.starts_with('-'), &arg[1..]);
            if on && letters.contains('f') {
                return None;
            }
            if letters.ends_with('o') {
                let option = name(options.next()?);
                if (on && option == "norcs") || (!on && option == "rcs") {
                    return None;
                }
            }
        } else {
            // A script, and its arguments.
            break;
        }
    }

    let mut variables = vec![("ZDOTDIR".into(), directory.join("zsh").to_string_lossy().into())];
    if let Some(saved) = env("ZDOTDIR") {
        variables.push(("TANKTERM_ZSH_ZDOTDIR".into(), saved));
    }

    Some(Injection { args: args.to_vec(), env: variables })
}

/// fish reads the configuration snippets in `fish/vendor_conf.d` of the directories in
/// `XDG_DATA_DIRS`, where the snippet takes the directory out again.
///
/// Shells that read no configuration aren't changed.
fn fish(
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    directory: &Path,
) -> Option<Injection> {
    for arg in args {
        if arg == "--" || !arg.starts_with('-') {
            break;
        }
        let short = !arg.starts_with("--") && arg[1..].contains('N');
        if short || arg == "--no-config" {
            return None;
        }
    }

    let directory = directory.to_string_lossy();
    let mut variables = Vec::new();
    let dirs = match env("XDG_DATA_DIRS").filter(|dirs| !dirs.is_empty()) {
        Some(dirs) => dirs,
        None => {
            variables.push(("TANKTERM_FISH_UNSET_XDG_DATA_DIRS".into(), "1".into()));
            DEFAULT_XDG_DATA_DIRS.into()
        },
    };
    variables.push(("XDG_DATA_DIRS".into(), format!("{directory}:{dirs}")));

    Some(Injection { args: args.to_vec(), env: variables })
}

/// Start the shell of `backend` with the shell integration, if it is bash, zsh or fish.
pub fn apply(backend: &mut BackendSettings) {
    if Shell::of(&backend.program).is_none() {
        return;
    }

    if let Some(directory) = directory() {
        apply_from(backend, directory);
    }
}

/// [`apply`] with the scripts in `directory`.
fn apply_from(backend: &mut BackendSettings, directory: &Path) {
    let configured = &backend.env;
    let env = |name: &str| configured.get(name).cloned().or_else(|| env::var(name).ok());
    let Some(injection) = inject(&backend.program, &backend.args, &env, directory) else {
        return;
    };

    backend.args = injection.args;
    backend.env.extend(injection.env);
}

/// The directory with the scripts, written there by the first call, if they could be.
fn directory() -> Option<&'static Path> {
    static DIRECTORY: OnceLock<Option<PathBuf>> = OnceLock::new();
    let directory = DIRECTORY.get_or_init(|| {
        let directory = base_directory()?.join("shell-integration").join(env!("CARGO_PKG_VERSION"));
        match write_scripts(&directory) {
            Ok(()) => Some(directory),
            Err(err) => {
                warn!("Unable to write the shell integration to {}: {err}", directory.display());
                None
            },
        }
    });

    directory.as_deref()
}

/// TankTerm's directory in the runtime directory, or else one in the temporary directory that
/// only this user can write to.
fn base_directory() -> Option<PathBuf> {
    #[cfg(not(target_os = "macos"))]
    if let Ok(runtime) = xdg::BaseDirectories::new().get_runtime_directory() {
        return Some(runtime.join("tankterm"));
    }

    // SAFETY: `getuid` has no preconditions.
    let user = unsafe { libc::getuid() };
    let directory = env::temp_dir().join(format!("tankterm-{user}"));
    // Other users can create files in the temporary directory, so the directory is only used if
    // this user made it and nobody else can write to it.
    let _ = fs::DirBuilder::new().mode(0o700).create(&directory);
    let metadata = fs::symlink_metadata(&directory).ok()?;
    let private = metadata.is_dir() && metadata.uid() == user && metadata.mode() & 0o022 == 0;
    if !private {
        warn!(
            "Not writing the shell integration to {}: others can write to it",
            directory.display()
        );
        return None;
    }

    Some(directory)
}

/// Write the scripts to `directory`, replacing each file in one step so that a shell that starts
/// meanwhile reads a whole one.
fn write_scripts(directory: &Path) -> io::Result<()> {
    for (name, content) in SCRIPTS {
        let path = directory.join(name);
        if fs::read(&path).is_ok_and(|current| current == content.as_bytes()) {
            continue;
        }

        let parent = path.parent().unwrap_or(directory);
        fs::create_dir_all(parent)?;
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let temporary = parent.join(format!(".{file_name}.{}.tmp", process::id()));
        fs::write(&temporary, content)?;
        fs::rename(&temporary, &path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::io::{ErrorKind, Read, Write};
    use std::thread;
    use std::time::{Duration, Instant};

    use tankterm_terminal::event::WindowSize;
    use tankterm_terminal::tty::{self, Pty};
    use tempfile::TempDir;

    use super::*;

    const DIRECTORY: &str = "/run/tankterm";

    /// A shell's arguments and the variables it starts with, sorted, as `inject` changed them.
    type Started = Option<(Vec<String>, Vec<String>)>;

    /// A command line, the variables it starts with, and how `inject` changes it.
    type Case<'a> = (&'a [&'a str], &'a [(&'a str, &'a str)], Started);

    /// What `inject` does with a shell started with `command` and `env`, with the variables
    /// sorted.
    fn injected(command: &[&str], env: &[(&str, &str)]) -> Started {
        let env: HashMap<_, _> = env.iter().map(|(name, value)| (*name, *value)).collect();
        let lookup = |name: &str| env.get(name).map(|value| (*value).to_owned());
        let args: Vec<String> = command[1..].iter().map(|arg| (*arg).to_owned()).collect();
        let injection = inject(command[0], &args, &lookup, Path::new(DIRECTORY))?;

        let variables: BTreeMap<_, _> = injection.env.into_iter().collect();
        let variables = variables.into_iter().map(|(name, value)| format!("{name}={value}"));
        Some((injection.args, variables.collect()))
    }

    /// The arguments and sorted variables of an injection, as `injected` gives them.
    fn injection(args: &[&str], env: &[&str]) -> Started {
        let strings = |list: &[&str]| list.iter().map(|item| (*item).to_owned()).collect();
        let mut env: Vec<String> = strings(env);
        env.push(format!("{DIRECTORY_ENV}={DIRECTORY}"));
        env.sort();
        Some((strings(args), env))
    }

    #[test]
    fn bash_starts_in_posix_mode_with_env() {
        let env = "ENV=/run/tankterm/bash/tankterm-inject.bash";
        let cases: &[Case<'_>] = &[
            (&["bash"], &[], injection(&["--posix"], &[env, "TANKTERM_BASH_INJECT=1"])),
            (&["-bash"], &[], injection(&["--posix"], &[env, "TANKTERM_BASH_INJECT=1"])),
            (
                &["/usr/bin/bash", "-l"],
                &[("HOME", "/home/me")],
                injection(&["--posix", "-l"], &[
                    env,
                    "HISTFILE=/home/me/.bash_history",
                    "TANKTERM_BASH_INJECT=1",
                    "TANKTERM_BASH_UNEXPORT_HISTFILE=1",
                ]),
            ),
            (
                &["bash", "--norc", "--noprofile", "-i"],
                &[],
                injection(&["--posix", "-i"], &[env, "TANKTERM_BASH_INJECT=1 --norc --noprofile"]),
            ),
            (
                &["bash", "--login", "--rcfile", "/etc/rc", "-i"],
                &[],
                injection(&["--posix", "--login", "-i"], &[
                    env,
                    "TANKTERM_BASH_INJECT=1",
                    "TANKTERM_BASH_RCFILE=/etc/rc",
                ]),
            ),
            (
                &["bash"],
                &[("ENV", "/etc/env"), ("HISTFILE", "/h"), ("HOME", "/home/me")],
                injection(&["--posix"], &[
                    env,
                    "TANKTERM_BASH_ENV=/etc/env",
                    "TANKTERM_BASH_INJECT=1",
                ]),
            ),
            // Options with a value, and arguments for a shell that reads its commands.
            (
                &["bash", "-o", "vi", "+O", "checkwinsize"],
                &[],
                injection(&["--posix", "-o", "vi", "+O", "checkwinsize"], &[
                    env,
                    "TANKTERM_BASH_INJECT=1",
                ]),
            ),
            (
                &["bash", "-s", "one", "--", "two"],
                &[],
                injection(&["--posix", "-s", "one", "--", "two"], &[env, "TANKTERM_BASH_INJECT=1"]),
            ),
        ];
        for (command, env, expected) in cases {
            assert_eq!(&injected(command, env), expected, "{command:?}");
        }
    }

    #[test]
    fn bash_that_reads_no_startup_files_is_left_alone() {
        let commands: &[&[&str]] = &[
            &["bash", "-c", "ls"],
            &["bash", "-lc", "ls"],
            &["bash", "--posix"],
            &["bash", "-o", "posix"],
            &["bash", "script.sh"],
            &["bash", "-l", "--", "script.sh"],
            &["bash", "--rcfile"],
        ];
        for command in commands {
            assert_eq!(injected(command, &[]), None, "{command:?}");
        }
    }

    #[test]
    fn zsh_reads_its_startup_files_from_the_integration() {
        let zdotdir = "ZDOTDIR=/run/tankterm/zsh";
        let cases: &[Case<'_>] = &[
            (&["zsh"], &[], injection(&[], &[zdotdir])),
            (
                &["/bin/zsh", "-l"],
                &[("ZDOTDIR", "/home/me/.zsh")],
                injection(&["-l"], &[zdotdir, "TANKTERM_ZSH_ZDOTDIR=/home/me/.zsh"]),
            ),
            // `-f` is an argument of the script there.
            (&["zsh", "script.zsh", "-f"], &[], injection(&["script.zsh", "-f"], &[zdotdir])),
            (&["zsh", "+o", "norcs"], &[], injection(&["+o", "norcs"], &[zdotdir])),
        ];
        for (command, env, expected) in cases {
            assert_eq!(&injected(command, env), expected, "{command:?}");
        }

        let commands: &[&[&str]] = &[
            &["zsh", "-f"],
            &["zsh", "-lf"],
            &["zsh", "--no-rcs"],
            &["zsh", "--NO_RCS"],
            &["zsh", "-o", "no_rcs"],
            &["zsh", "+o", "rcs"],
        ];
        for command in commands {
            assert_eq!(injected(command, &[]), None, "{command:?}");
        }
    }

    #[test]
    fn fish_finds_the_integration_in_its_data_directories() {
        let cases: &[Case<'_>] = &[
            (
                &["fish"],
                &[],
                injection(&[], &[
                    "TANKTERM_FISH_UNSET_XDG_DATA_DIRS=1",
                    "XDG_DATA_DIRS=/run/tankterm:/usr/local/share:/usr/share",
                ]),
            ),
            (
                &["/usr/bin/fish", "-l"],
                &[("XDG_DATA_DIRS", "/a:/b")],
                injection(&["-l"], &["XDG_DATA_DIRS=/run/tankterm:/a:/b"]),
            ),
        ];
        for (command, env, expected) in cases {
            assert_eq!(&injected(command, env), expected, "{command:?}");
        }

        for command in [&["fish", "-N"][..], &["fish", "-iN"], &["fish", "--no-config"]] {
            assert_eq!(injected(command, &[]), None, "{command:?}");
        }
    }

    #[test]
    fn other_programs_are_left_alone() {
        for program in ["sh", "/bin/dash", "cat", "/usr/bin/bashful", "nu"] {
            assert_eq!(injected(&[program], &[]), None, "{program}");
        }
    }

    #[test]
    fn scripts_are_written_and_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("zsh/.zshenv");
        write_scripts(directory.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), SCRIPTS[4].1);

        fs::write(&path, "stale").unwrap();
        write_scripts(directory.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), SCRIPTS[4].1);

        // Nothing else is left behind.
        let names: Vec<_> = fs::read_dir(directory.path().join("zsh"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [".zshenv"]);
    }

    /// Whether `program` is on the `PATH`; tests of shells that aren't installed are skipped.
    fn installed(program: &str) -> bool {
        let found = env::var_os("PATH").is_some_and(|path| {
            env::split_paths(&path).any(|directory| directory.join(program).is_file())
        });
        if !found {
            eprintln!("{program} isn't installed; skipped");
        }
        found
    }

    /// An interactive shell started the way TankTerm starts it, in a PTY of its own, with a
    /// home directory of its own.
    struct Session {
        pty: Pty,
        output: Vec<u8>,
        home: TempDir,
        _scripts: TempDir,
    }

    impl Session {
        /// Start `program` with `args`; `files` are written to the home directory first.
        fn start(
            program: &str,
            args: &[&str],
            env: &[(&str, &str)],
            files: &[(&str, &str)],
        ) -> Self {
            let home = tempfile::tempdir().unwrap();
            for (name, content) in files {
                let path = home.path().join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, content).unwrap();
            }
            let scripts = tempfile::tempdir().unwrap();
            write_scripts(scripts.path()).unwrap();

            let home_path = home.path().to_str().unwrap();
            let mut env: HashMap<String, String> = env
                .iter()
                .map(|(name, value)| ((*name).into(), value.replace("~", home_path)))
                .collect();
            env.insert("HOME".into(), home_path.into());
            env.insert("TERM".into(), "xterm-256color".into());
            let mut backend = BackendSettings {
                program: program.into(),
                args: args.iter().map(|arg| arg.replace("~", home_path)).collect(),
                env,
                working_directory: Some(home.path().into()),
                hold: false,
            };
            apply_from(&mut backend, scripts.path());
            assert!(backend.env.contains_key(DIRECTORY_ENV), "{program} {args:?} wasn't injected");

            let options = tty::Options {
                shell: Some(tty::Shell::new(backend.program, backend.args)),
                working_directory: backend.working_directory,
                drain_on_exit: false,
                env: backend.env,
            };
            let size = WindowSize { num_lines: 24, num_cols: 120, cell_width: 8, cell_height: 16 };
            let pty = tty::new(&options, size, 0).unwrap();
            Self { pty, output: Vec::new(), home, _scripts: scripts }
        }

        /// The home directory.
        fn home(&self) -> &str {
            self.home.path().to_str().unwrap()
        }

        /// Type a line.
        fn send(&mut self, line: &str) {
            let mut file = self.pty.file();
            file.write_all(format!("{line}\r").as_bytes()).unwrap();
        }

        /// Wait for the shell to print `text`, and forget what it printed up to there.
        fn expect(&mut self, text: &str) {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut buffer = [0; 4096];
            let mut answered = 0;
            loop {
                if let Some(found) = find(&self.output, text.as_bytes()) {
                    self.output.drain(..found + text.len());
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "never printed {text:?}; printed:\n{}",
                    String::from_utf8_lossy(&self.output)
                );

                match self.pty.file().read(&mut buffer) {
                    Ok(0) => panic!("the shell exited:\n{}", String::from_utf8_lossy(&self.output)),
                    Ok(read) => self.output.extend_from_slice(&buffer[..read]),
                    Err(err) if err.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    },
                    // The shell exited, on Linux.
                    Err(err) => panic!("{err}:\n{}", String::from_utf8_lossy(&self.output)),
                }

                // Shells that ask what the terminal can do get the answer of a VT220.
                let questions = count(&self.output, b"\x1b[c") + count(&self.output, b"\x1b[0c");
                while answered < questions {
                    self.pty.file().write_all(b"\x1b[?62;22c").unwrap();
                    answered += 1;
                }
            }
        }
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|window| window == needle)
    }

    fn count(haystack: &[u8], needle: &[u8]) -> usize {
        haystack.windows(needle.len()).filter(|window| *window == needle).count()
    }

    const PROMPT_START: &str = "\x1b]133;A";

    /// The user's startup files ran, and nothing of the injection is left in the shell.
    #[test]
    fn bash_reads_the_users_bashrc() {
        if !installed("bash") {
            return;
        }

        let bashrc = "PS1='$ '\ntankterm_test_rc=ran\n";
        let mut bash =
            Session::start("bash", &[], &[("ENV", "/etc/tankterm-env")], &[(".bashrc", bashrc)]);
        bash.expect(PROMPT_START);
        bash.send(
            r#"[[ -o posix ]] && p=on || p=off; echo "rc=$tankterm_test_rc posix=$p env=$ENV""#,
        );
        bash.expect("rc=ran posix=off env=/etc/tankterm-env");
        bash.send(
            r#"echo "left=${TANKTERM_BASH_INJECT-}${TANKTERM_BASH_ENV-}${TANKTERM_BASH_RCFILE-}.""#,
        );
        bash.expect("left=.");

        // The history goes where bash keeps it outside POSIX mode, and stays bash's own.
        if env::var_os("HISTFILE").is_none() {
            let home = bash.home().to_owned();
            bash.send(r#"echo "history=$HISTFILE exported=$(export -p | grep -c HISTFILE)""#);
            bash.expect(&format!("history={home}/.bash_history exported=0"));
        }

        bash.send("false");
        bash.expect("\x1b]133;D;1");
        bash.expect(PROMPT_START);
    }

    #[test]
    fn bash_login_shell_reads_the_profile() {
        if !installed("bash") {
            return;
        }

        let files = [
            (".bash_profile", "PS1='$ '\ntankterm_test_profile=ran\n"),
            (".bashrc", "tankterm_test_rc=ran\n"),
        ];
        let mut bash = Session::start("bash", &["-l"], &[], &files);
        bash.expect(PROMPT_START);
        bash.send(r#"echo "profile=$tankterm_test_profile rc=$tankterm_test_rc.""#);
        bash.expect("profile=ran rc=.");
    }

    #[test]
    fn bash_reads_the_rcfile_it_was_given() {
        if !installed("bash") {
            return;
        }

        let files = [("custom", "PS1='$ '\ntankterm_test_rc=custom\n"), (".bashrc", "exit 1\n")];
        let mut bash = Session::start("bash", &["--rcfile", "~/custom"], &[], &files);
        bash.expect(PROMPT_START);
        bash.send(r#"echo "rc=$tankterm_test_rc""#);
        bash.expect("rc=custom");
    }

    #[test]
    fn zsh_reads_the_users_files_and_gets_zdotdir_back() {
        if !installed("zsh") {
            return;
        }

        let files = [
            ("zdot/.zshenv", "tankterm_test_env=ran\n"),
            ("zdot/.zshrc", "PS1='%% '\ntankterm_test_rc=ran\n"),
        ];
        let mut zsh = Session::start("zsh", &[], &[("ZDOTDIR", "~/zdot")], &files);
        zsh.expect(PROMPT_START);
        let home = zsh.home().to_owned();
        zsh.send(r#"echo "env=$tankterm_test_env rc=$tankterm_test_rc zdotdir=$ZDOTDIR""#);
        zsh.expect(&format!("env=ran rc=ran zdotdir={home}/zdot"));

        zsh.send("false");
        zsh.expect("\x1b]133;D;1");
        zsh.expect(PROMPT_START);
    }

    #[test]
    fn zsh_without_zdotdir_reads_the_home_directory() {
        if !installed("zsh") || env::var_os("ZDOTDIR").is_some() {
            return;
        }

        let files = [(".zshrc", "PS1='%% '\ntankterm_test_rc=ran\n")];
        let mut zsh = Session::start("zsh", &[], &[], &files);
        zsh.expect(PROMPT_START);
        zsh.send(r#"echo "rc=$tankterm_test_rc zdotdir=${ZDOTDIR-unset}""#);
        zsh.expect("rc=ran zdotdir=unset");
    }

    /// fish reads its configuration, the prompt still sees the status of the command before,
    /// and `XDG_DATA_DIRS` is what it was.
    #[test]
    fn fish_reads_its_configuration_and_gets_xdg_data_dirs_back() {
        if !installed("fish") {
            return;
        }

        let config = "set -g tankterm_test_rc ran\nfunction fish_prompt\n    printf 'ps=%s> ' \
                      \"$pipestatus\"\nend\n";
        let env = [
            ("XDG_CONFIG_HOME", "~/.config"),
            ("XDG_DATA_HOME", "~/.local/share"),
            ("XDG_DATA_DIRS", "/usr/share:/tankterm-test"),
        ];
        let mut fish = Session::start("fish", &[], &env, &[(".config/fish/config.fish", config)]);
        fish.expect(PROMPT_START);
        fish.expect("ps=0> ");
        fish.send("false | true");
        fish.expect("ps=1 0> ");
        fish.send(r#"echo "rc=$tankterm_test_rc dirs=$XDG_DATA_DIRS""#);
        fish.expect("rc=ran dirs=/usr/share:/tankterm-test");
    }
}
