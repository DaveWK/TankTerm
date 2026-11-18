//! `tankTerm ssh`: run ssh with a `TERM` the server has a terminfo entry for.
//!
//! Hosts listed in `ssh.probe` are checked once, over the connection the session then reuses,
//! for the entries in `ssh.order`. The answer is cached per resolved user, host and port. Other
//! hosts get `ssh.fallback`. Nothing is ever installed on a server.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::cli::SshOptions;
use crate::config::ssh::Ssh;

/// The ssh program, looked up in `$PATH`.
const SSH: &str = "ssh";

/// ssh options that take a value, from the getopt string of OpenSSH's `ssh.c`.
const WITH_VALUE: &str = "BbcDEeFIiJLlmOoPpQRSWw";

/// Options that don't open a session, such as `-G` and `-V`; ssh gets these unchanged.
const NO_SESSION: &str = "GOQV";

/// Options that rule out a probe: no remote shell (`-N`, `-W`), or connection sharing the user
/// manages (`-M`, `-S`).
const NO_PROBE: &str = "MNSW";

/// Options left out of the probe: terminal, background, forwarding and subsystem options.
const NOT_FOR_PROBE: &str = "DfLNnRsTtw";

/// System terminfo directories. `~/.terminfo` and `$TERMINFO` are left out, since root can't see
/// them after `sudo`.
const TERMINFO_DIRS: &[&str] = &[
    "/etc/terminfo",
    "/lib/terminfo",
    "/usr/share/terminfo",
    "/usr/lib/terminfo",
    "/usr/share/lib/terminfo",
    "/usr/local/share/terminfo",
];

/// First and last line the probe prints.
const PROBE_START: &str = "tankterm-probe";
const PROBE_END: &str = "tankterm-probe-end";

/// Seconds the shared connection stays open after its last session, so the session can attach.
const CONTROL_PERSIST: &str = "30";

/// The entry Debian and Ubuntu ship in `ncurses-term`.
const NCURSES_TERM_ENTRY: &str = "alacritty";

/// Run ssh with `options`, choosing `TERM` as `config` says.
pub fn run(config: &Ssh, options: &SshOptions) -> ! {
    let state = State { cache: cache_path(), control_dir: control_dir() };
    let session = match prepare(config, options, &state) {
        Ok(session) => session,
        Err(status) => process::exit(status),
    };

    let mut ssh = Command::new(SSH);
    ssh.args(&session.control).args(&options.args);
    if let Some(term) = &session.term {
        ssh.env("TERM", term);
    }

    let err = ssh.exec();
    eprintln!("tankTerm ssh: unable to run {SSH}: {err}");
    process::exit(255);
}

/// Where the cache and the shared connections are kept.
struct State {
    cache: Option<PathBuf>,
    control_dir: Option<PathBuf>,
}

/// How to start the session.
#[derive(Debug, Default, PartialEq, Eq)]
struct Session {
    /// `TERM` to send, or `None` to leave it as it is.
    term: Option<String>,
    /// Options put before the user's, to reuse the probe's connection.
    control: Vec<String>,
}

impl Session {
    fn with_term(term: &str) -> Self {
        Self { term: Some(term.to_owned()), control: Vec::new() }
    }
}

/// Choose the `TERM` for the session, probing the host if it opted in and isn't cached.
///
/// Returns the exit status if the probe couldn't connect.
fn prepare(config: &Ssh, options: &SshOptions, state: &State) -> Result<Session, i32> {
    let order = valid_names(&config.order);
    let fallback = if valid_name(&config.fallback) {
        config.fallback.clone()
    } else {
        eprintln!("tankTerm ssh: ignoring invalid ssh.fallback {:?}", config.fallback);
        Ssh::default().fallback
    };

    // Arguments ssh would reject, or that don't start a session, go to ssh unchanged.
    let Some(args) = SshArgs::parse(&options.args) else { return Ok(Session::default()) };
    let Some(destination) = &args.destination else { return Ok(Session::default()) };
    if args.has_any(NO_SESSION) {
        return Ok(Session::default());
    }

    if options.no_probe {
        return Ok(Session::with_term(&fallback));
    }
    let Some(resolved) = resolve(&args, destination) else {
        return Ok(Session::with_term(&fallback));
    };
    let alias = destination_host(destination);
    let opted_in =
        options.probe || options.reprobe || wants_probe(&config.probe, &alias, &resolved);
    if !opted_in {
        return Ok(Session::with_term(&fallback));
    }

    let key = resolved.key();
    let cached = state.cache.as_deref().map(Cache::load).unwrap_or_default();
    let cached = cached.hosts.get(&key).and_then(|entry| entry.term(&order, &fallback));
    match cached {
        Some(term) if !options.reprobe => return Ok(Session::with_term(&term)),
        _ if args.has_any(NO_PROBE) => return Ok(Session::with_term(&fallback)),
        _ => (),
    }

    let (probe_control, control) = control_options(&resolved, state.control_dir.as_deref());
    let output = Command::new(SSH)
        .args(&probe_control)
        .args(probe_options())
        .args(args.options_for_probe())
        .arg(destination)
        .arg(probe_command(&order, ""))
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output();
    // Without ssh, running the session reports the error.
    let Ok(output) = output else { return Ok(Session::with_term(&fallback)) };

    let Some(probe) = Probe::parse(&String::from_utf8_lossy(&output.stdout)) else {
        // ssh exits with 255 when it couldn't connect, and has said why; a second attempt
        // would fail the same way.
        if output.status.code().is_none_or(|status| status == 255) {
            return Err(255);
        }
        eprintln!(
            "tankTerm ssh: unable to check the terminfo entries of {alias}; using {fallback}"
        );
        return Ok(Session { term: Some(fallback), control });
    };

    let entry = Entry {
        found: order.iter().filter(|name| probe.found.contains(*name)).cloned().collect(),
        checked: order.clone(),
    };
    let term = entry.term(&order, &fallback).unwrap_or(fallback);

    if order.iter().any(|name| name == NCURSES_TERM_ENTRY)
        && !probe.found.contains(NCURSES_TERM_ENTRY)
        && probe.is_debian()
    {
        eprintln!(
            "tankTerm ssh: {alias} has no {NCURSES_TERM_ENTRY} terminfo entry, so it gets {term}. \
             Installing ncurses-term there adds it."
        );
    }

    if let Some(path) = &state.cache {
        // Read the file again, in case another session wrote to it while this one probed.
        let mut cache = Cache::load(path);
        cache.hosts.insert(key, entry);
        if let Err(err) = cache.save(path) {
            eprintln!("tankTerm ssh: unable to save {}: {err}", path.display());
        }
    }

    Ok(Session { term: Some(term), control })
}

/// Whether a terminfo name is safe to put in the probe and in `TERM`.
fn valid_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

/// The valid names of `names`, reporting the others.
fn valid_names(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| {
            let valid = valid_name(name);
            if !valid {
                eprintln!("tankTerm ssh: ignoring invalid terminfo name {name:?} in ssh.order");
            }
            valid
        })
        .cloned()
        .collect()
}

/// An ssh option and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SshOption {
    flag: char,
    value: Option<String>,
}

/// The parts of an ssh command line `tankTerm ssh` needs.
#[derive(Debug, Default, PartialEq, Eq)]
struct SshArgs {
    options: Vec<SshOption>,
    destination: Option<String>,
}

impl SshArgs {
    /// Parse ssh arguments like ssh does: options, the destination, more options, and the
    /// command. `--` ends the options.
    ///
    /// Returns `None` if an option lacks its value.
    fn parse(args: &[String]) -> Option<Self> {
        let mut options = Vec::new();
        let (index, ended) = parse_options(args, 0, &mut options)?;
        let destination = args.get(index).cloned();
        if destination.is_some() && !ended {
            parse_options(args, index + 1, &mut options)?;
        }

        Some(Self { options, destination })
    }

    /// Whether any of `flags` was given.
    fn has_any(&self, flags: &str) -> bool {
        self.options.iter().any(|option| flags.contains(option.flag))
    }

    /// The options as separate arguments, leaving out `skip`.
    fn arguments(&self, skip: &str) -> Vec<String> {
        let mut arguments = Vec::new();
        for option in self.options.iter().filter(|option| !skip.contains(option.flag)) {
            arguments.push(format!("-{}", option.flag));
            arguments.extend(option.value.clone());
        }
        arguments
    }

    /// The user's options that also apply to the probe.
    fn options_for_probe(&self) -> Vec<String> {
        self.arguments(NOT_FOR_PROBE)
    }
}

/// Parse options from `start` on, until an argument that isn't one.
///
/// Returns where the options stopped, and whether they stopped at `--`.
fn parse_options(
    args: &[String],
    start: usize,
    options: &mut Vec<SshOption>,
) -> Option<(usize, bool)> {
    let mut index = start;
    while let Some(arg) = args.get(index) {
        if arg == "--" {
            return Some((index + 1, true));
        }
        let Some(flags) = arg.strip_prefix('-').filter(|flags| !flags.is_empty()) else {
            break;
        };
        index += 1;

        for (position, flag) in flags.char_indices() {
            if !WITH_VALUE.contains(flag) {
                options.push(SshOption { flag, value: None });
                continue;
            }

            // The value is the rest of the argument, or the next one.
            let rest = &flags[position + flag.len_utf8()..];
            let value = if rest.is_empty() {
                index += 1;
                args.get(index - 1)?.clone()
            } else {
                rest.to_owned()
            };
            options.push(SshOption { flag, value: Some(value) });
            break;
        }
    }

    Some((index, false))
}

/// The host of a destination as typed: `[user@]host` or `ssh://[user@]host[:port]`.
fn destination_host(destination: &str) -> String {
    let host = match destination.strip_prefix("ssh://") {
        Some(uri) => {
            let authority = uri.split('/').next().unwrap_or_default();
            let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
            match host.strip_prefix('[') {
                Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
                None => host.split(':').next().unwrap_or_default(),
            }
        },
        None => destination.rsplit_once('@').map_or(destination, |(_, host)| host),
    };
    host.to_lowercase()
}

/// What ssh makes of the destination after reading its configuration.
#[derive(Debug, Default, PartialEq, Eq)]
struct Resolved {
    user: String,
    hostname: String,
    port: String,
    /// Whether the configuration sets a `ControlPath`.
    control_path: bool,
    /// Whether the configuration sets a `ControlPersist`.
    control_persist: bool,
}

impl Resolved {
    /// Read the output of `ssh -G`.
    fn parse(output: &str) -> Option<Self> {
        let mut resolved = Self::default();
        for line in output.lines() {
            let Some((key, value)) = line.trim().split_once(' ') else { continue };
            let value = value.trim();
            match key.to_ascii_lowercase().as_str() {
                "user" => resolved.user = value.to_owned(),
                "hostname" => resolved.hostname = value.to_lowercase(),
                "port" => resolved.port = value.to_owned(),
                "controlpath" => resolved.control_path = value != "none",
                "controlpersist" => resolved.control_persist = value != "no",
                _ => (),
            }
        }

        let complete = [&resolved.user, &resolved.hostname, &resolved.port];
        complete.iter().all(|value| !value.is_empty()).then_some(resolved)
    }

    /// The cache key: `user@host:port`.
    fn key(&self) -> String {
        format!("{}@{}:{}", self.user, self.hostname, self.port)
    }
}

/// Ask `ssh -G` how it resolves the destination.
fn resolve(args: &SshArgs, destination: &str) -> Option<Resolved> {
    let output = Command::new(SSH)
        .arg("-G")
        .args(args.arguments(""))
        .arg(destination)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| Resolved::parse(&String::from_utf8_lossy(&output.stdout)))?
}

/// Whether `ssh.probe` lists the host.
///
/// Patterns without `@` match the host as typed or the host name ssh resolves it to; patterns
/// with `@` match `user@host` with either. A matching pattern starting with `!` rules the host
/// out.
fn wants_probe(patterns: &[String], alias: &str, resolved: &Resolved) -> bool {
    let hosts = [alias.to_owned(), resolved.hostname.clone()];
    let users = hosts.clone().map(|host| format!("{}@{host}", resolved.user));

    let mut wanted = false;
    for pattern in patterns {
        let (negated, pattern) = match pattern.strip_prefix('!') {
            Some(pattern) => (true, pattern),
            None => (false, pattern.as_str()),
        };
        let pattern = pattern.to_lowercase();
        let candidates = if pattern.contains('@') { &users } else { &hosts };

        if candidates.iter().any(|candidate| glob(&pattern, &candidate.to_lowercase())) {
            if negated {
                return false;
            }
            wanted = true;
        }
    }

    wanted
}

/// Match `text` against a pattern with `*` (any characters) and `?` (one character).
fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    let (mut p, mut t) = (0, 0);
    // Position after the last `*`, and the text position it was tried at.
    let mut star = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p + 1, t));
                p += 1;
            },
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            },
            _ => match star {
                // Let the last `*` take one more character.
                Some((after, tried)) => {
                    p = after;
                    t = tried + 1;
                    star = Some((after, tried + 1));
                },
                None => return false,
            },
        }
    }

    pattern[p..].iter().all(|&c| c == '*')
}

/// ssh options for the probe and for the session that reuses its connection.
///
/// The configuration's own `ControlPath` is used if it has one.
fn control_options(resolved: &Resolved, control_dir: Option<&Path>) -> (Vec<String>, Vec<String>) {
    let mut probe = vec!["-o".to_owned(), "ControlMaster=auto".to_owned()];
    let mut session = Vec::new();

    if !resolved.control_path {
        let Some(dir) = control_dir else { return (Vec::new(), Vec::new()) };
        let path = format!("ControlPath={}", dir.join("ssh-%C").display());
        probe.extend(["-o".to_owned(), path.clone()]);
        session.extend(["-o".to_owned(), path]);
    }
    if !resolved.control_persist {
        probe.extend(["-o".to_owned(), format!("ControlPersist={CONTROL_PERSIST}")]);
    }

    (probe, session)
}

/// Options that keep the probe to running its command.
fn probe_options() -> Vec<String> {
    let options =
        ["RequestTTY=no", "RemoteCommand=none", "ClearAllForwardings=yes", "PermitLocalCommand=no"];
    options.iter().flat_map(|option| ["-o".to_owned(), (*option).to_owned()]).collect()
}

/// The command the probe runs: `probe_script` in single quotes, for the login shell to pass to
/// `sh`.
fn probe_command(names: &[String], root: &str) -> String {
    format!("sh -c '{}'", probe_script(names, root))
}

/// The shell script that lists which of `names` the system terminfo directories have, and
/// prints the `ID` and `ID_LIKE` of `/etc/os-release`.
///
/// It is one line without single quotes, backslashes or `!`, so it can be quoted for any login
/// shell. Entries are looked for in both layouts: `a/alacritty` and macOS' `61/alacritty`.
fn probe_script(names: &[String], root: &str) -> String {
    let dirs: Vec<String> = TERMINFO_DIRS.iter().map(|dir| format!("{root}{dir}")).collect();
    let files: Vec<String> = names
        .iter()
        .filter_map(|name| {
            let first = name.chars().next()?;
            Some(format!("{first}/{name} {:x}/{name}", first as u32))
        })
        .collect();

    format!(
        "echo {PROBE_START}; for d in {dirs}; do for f in {files}; do if test -f \"$d/$f\"; then \
         echo \"found $f\"; fi; done; done; for o in {root}/etc/os-release \
         {root}/usr/lib/os-release; do if test -r \"$o\"; then (. \"$o\"; echo \"os $ID \
         $ID_LIKE\"); break; fi; done; echo {PROBE_END}",
        dirs = dirs.join(" "),
        files = files.join(" "),
    )
}

/// What the probe found.
#[derive(Debug, Default, PartialEq, Eq)]
struct Probe {
    /// Terminfo names with an entry.
    found: BTreeSet<String>,
    /// `ID` and `ID_LIKE` of the server's OS.
    os: Vec<String>,
}

impl Probe {
    /// Read the probe's output, which login scripts may have added lines to.
    ///
    /// Returns `None` if the probe didn't run to its end.
    fn parse(output: &str) -> Option<Self> {
        let mut lines = output.lines().map(str::trim);
        // A login script may have printed text without a line break before the probe's.
        lines.find(|line| line.ends_with(PROBE_START))?;

        let mut probe = Self::default();
        for line in lines {
            if line == PROBE_END {
                return Some(probe);
            } else if let Some(file) = line.strip_prefix("found ") {
                let name = file.rsplit('/').next().unwrap_or_default();
                probe.found.insert(name.to_owned());
            } else if let Some(ids) = line.strip_prefix("os ") {
                probe.os = ids.split_whitespace().map(str::to_lowercase).collect();
            }
        }

        None
    }

    /// Whether the server runs Debian, Ubuntu or a distribution based on them.
    fn is_debian(&self) -> bool {
        self.os.iter().any(|id| id == "debian" || id == "ubuntu")
    }
}

/// Probe results by `user@host:port`.
#[derive(Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
struct Cache {
    #[serde(default)]
    hosts: BTreeMap<String, Entry>,
}

/// The probe result for one host.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// Names the probe looked for.
    checked: Vec<String>,
    /// Names it found.
    found: Vec<String>,
}

impl Entry {
    /// The first of `order` the host has, or `fallback` if it has none.
    ///
    /// Returns `None` if the probe didn't look for every name in `order`.
    fn term(&self, order: &[String], fallback: &str) -> Option<String> {
        if !order.iter().all(|name| self.checked.contains(name)) {
            return None;
        }
        let found = order.iter().find(|name| self.found.contains(name));
        Some(found.map_or(fallback, String::as_str).to_owned())
    }
}

impl Cache {
    /// Read the cache; a missing or damaged file is an empty cache.
    fn load(path: &Path) -> Self {
        let cache = fs::read_to_string(path).ok();
        cache.and_then(|cache| serde_json::from_str(&cache).ok()).unwrap_or_default()
    }

    /// Write the cache, replacing the previous file in a single step.
    fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }

        let temporary = path.with_extension(format!("{}.tmp", process::id()));
        fs::write(&temporary, serde_json::to_string_pretty(self)?)?;
        fs::rename(temporary, path)
    }
}

/// Where the cache is stored: `$XDG_STATE_HOME/tankterm/ssh-terms.json`.
fn cache_path() -> Option<PathBuf> {
    xdg::BaseDirectories::with_prefix("tankterm").get_state_file("ssh-terms.json")
}

/// Directory for the shared connections' sockets: `$XDG_RUNTIME_DIR/tankterm`, or
/// `/tmp/tankterm-<uid>`. `/tmp` keeps the socket paths short enough on macOS.
fn control_dir() -> Option<PathBuf> {
    let runtime = xdg::BaseDirectories::new().get_runtime_directory().ok().cloned();
    let dir = match runtime {
        Some(runtime) => runtime.join("tankterm"),
        None => PathBuf::from(format!("/tmp/tankterm-{}", unsafe { libc::getuid() })),
    };

    DirBuilder::new().recursive(true).mode(0o700).create(&dir).ok()?;
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn order() -> Vec<String> {
        Ssh::default().order
    }

    fn resolved(user: &str, hostname: &str) -> Resolved {
        Resolved {
            user: user.into(),
            hostname: hostname.into(),
            port: "22".into(),
            ..Default::default()
        }
    }

    #[test]
    fn arguments_are_parsed_like_ssh_does() {
        let args = SshArgs::parse(&strings(&[
            "-p",
            "2222",
            "-tA",
            "-oUser=x",
            "-vJjump",
            "dave@host",
            "-L",
            "1:h:2",
            "ls",
            "-l",
        ]))
        .unwrap();

        assert_eq!(args.destination.as_deref(), Some("dave@host"));
        assert_eq!(
            args.arguments(""),
            strings(&["-p", "2222", "-t", "-A", "-o", "User=x", "-v", "-J", "jump", "-L", "1:h:2"])
        );
        // The command and its options aren't ssh options.
        assert!(!args.has_any("l"));
    }

    #[test]
    fn double_dash_ends_the_options() {
        let args = SshArgs::parse(&strings(&["-4", "--", "host", "-p", "1"])).unwrap();
        assert_eq!(args.destination.as_deref(), Some("host"));
        assert!(!args.has_any("p"));

        let args = SshArgs::parse(&strings(&["host", "--", "-p", "1"])).unwrap();
        assert!(!args.has_any("p"));
    }

    #[test]
    fn arguments_without_a_destination_or_value() {
        assert_eq!(SshArgs::parse(&strings(&["-V"])).unwrap().destination, None);
        assert_eq!(SshArgs::parse(&strings(&["host", "-p"])), None);
    }

    #[test]
    fn the_probe_leaves_out_terminal_and_forwarding_options() {
        let args = SshArgs::parse(&strings(&["-tt", "-L8080:h:80", "-i", "key", "-N", "host"]));
        assert_eq!(args.unwrap().options_for_probe(), strings(&["-i", "key"]));
    }

    #[test]
    fn destination_hosts() {
        assert_eq!(destination_host("Build.LAN"), "build.lan");
        assert_eq!(destination_host("me@x@host"), "host");
        assert_eq!(destination_host("ssh://me@host:2222"), "host");
        assert_eq!(destination_host("ssh://[::1]:22/"), "::1");
    }

    #[test]
    fn ssh_g_output_is_read() {
        let output = "user dave\nhostname Build.example.com\nport 2222\ncontrolmaster \
                      auto\ncontrolpath ~/.ssh/cm-%C\ncontrolpersist no\n";
        let resolved = Resolved::parse(output).unwrap();
        assert_eq!(resolved.key(), "dave@build.example.com:2222");
        assert!(resolved.control_path && !resolved.control_persist);

        assert_eq!(Resolved::parse("user dave\nport 22\n"), None);
    }

    #[test]
    fn globs() {
        assert!(glob("*.lan", "build.lan"));
        assert!(glob("build-?", "build-1"));
        assert!(glob("*a*b*", "xxaxxbxx"));
        assert!(glob("*", ""));
        assert!(!glob("*.lan", "build.lan.example.com"));
        assert!(!glob("build-?", "build-12"));
        assert!(!glob("a*b", "acbd"));
    }

    #[test]
    fn hosts_opt_in_by_alias_hostname_or_user() {
        let patterns = strings(&["*.lan", "build-*", "pi@10.0.0.*", "!secret.lan"]);
        let wants = |alias: &str, resolved: &Resolved| wants_probe(&patterns, alias, resolved);

        // The alias as typed, or the host name it resolves to.
        assert!(wants("box", &resolved("dave", "box.lan")));
        assert!(wants("build-3", &resolved("dave", "10.1.1.3")));
        // `user@host` patterns.
        assert!(wants("pi", &resolved("pi", "10.0.0.7")));
        assert!(!wants("pi", &resolved("dave", "10.0.0.7")));
        // Excluded, and not listed.
        assert!(!wants("secret.lan", &resolved("dave", "secret.lan")));
        assert!(!wants("example.com", &resolved("dave", "example.com")));
        assert!(!wants_probe(&[], "box.lan", &resolved("dave", "box.lan")));
    }

    #[test]
    fn invalid_names_are_dropped() {
        assert!(valid_name("xterm-256color") && valid_name("alacritty"));
        assert!(!valid_name("") && !valid_name("-x") && !valid_name("a'b") && !valid_name("a b"));
        assert_eq!(valid_names(&strings(&["alacritty", "x;y"])), strings(&["alacritty"]));
    }

    #[test]
    fn control_options_share_one_connection() {
        let dir = Path::new("/run/user/1000/tankterm");
        let (probe, session) = control_options(&resolved("dave", "host"), Some(dir));
        assert_eq!(
            probe,
            strings(&[
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPath=/run/user/1000/tankterm/ssh-%C",
                "-o",
                "ControlPersist=30",
            ])
        );
        assert_eq!(session, strings(&["-o", "ControlPath=/run/user/1000/tankterm/ssh-%C"]));

        // The configuration's own `ControlPath` and `ControlPersist` are kept.
        let own = Resolved { control_path: true, control_persist: true, ..resolved("d", "h") };
        let (probe, session) = control_options(&own, Some(dir));
        assert_eq!((probe, session), (strings(&["-o", "ControlMaster=auto"]), Vec::new()));
    }

    #[test]
    fn probe_output_is_read() {
        let output = "motd linetankterm-probe\nfound a/alacritty\nfound 78/xterm\nos ubuntu \
                      debian\ntankterm-probe-end\n";
        let probe = Probe::parse(output).unwrap();
        assert_eq!(probe.found, ["alacritty", "xterm"].map(String::from).into());
        assert!(probe.is_debian());

        // Cut short.
        assert_eq!(Probe::parse("tankterm-probe\nfound a/alacritty\n"), None);
        assert_eq!(Probe::parse(""), None);
    }

    #[test]
    fn probe_script_finds_system_entries() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_str().unwrap();
        let file = |path: &str, content: &str| {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        file("usr/share/terminfo/78/xterm-256color", "");
        file("lib/terminfo/x/xterm", "");
        // `~/.terminfo` doesn't count.
        file("home/terminfo/a/alacritty", "");
        // Without `/etc/os-release`, the one in `/usr/lib` is read.
        file("usr/lib/os-release", "ID=ubuntu\nID_LIKE=\"debian\"\n");

        let script = probe_script(&order(), root_path);
        assert!(!script.contains(['\'', '\\', '!', '\n']));

        // ssh hands the command to the login shell.
        let command = probe_command(&order(), root_path);
        let output = Command::new("sh").arg("-c").arg(&command).output().unwrap();
        let probe = Probe::parse(&String::from_utf8_lossy(&output.stdout)).unwrap();
        assert_eq!(probe.found, ["xterm", "xterm-256color"].map(String::from).into());
        assert_eq!(probe.os, strings(&["ubuntu", "debian"]));
    }

    #[test]
    fn cached_entries_follow_the_order() {
        let entry = Entry { checked: order(), found: strings(&["xterm"]) };
        assert_eq!(entry.term(&order(), "xterm-256color").as_deref(), Some("xterm"));

        let nothing = Entry { checked: order(), found: Vec::new() };
        assert_eq!(nothing.term(&order(), "vt100").as_deref(), Some("vt100"));

        // A name that was never looked for needs a new probe.
        let order = strings(&["foot", "xterm"]);
        assert_eq!(entry.term(&order, "xterm-256color"), None);
    }

    #[test]
    fn the_cache_survives_saving_and_loading() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state").join("ssh-terms.json");
        assert_eq!(Cache::load(&path), Cache::default());

        let mut cache = Cache::default();
        let entry = Entry { checked: order(), found: strings(&["alacritty"]) };
        cache.hosts.insert("dave@host:22".into(), entry);
        cache.save(&path).unwrap();
        assert_eq!(Cache::load(&path), cache);

        fs::write(&path, "{ not json").unwrap();
        assert_eq!(Cache::load(&path), Cache::default());
    }
}
