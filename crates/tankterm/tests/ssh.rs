//! Tests for `tankTerm ssh`.
//!
//! These run the `tankTerm` binary with a fake `ssh` first in `PATH`. The fake writes its `TERM`
//! and arguments to a log, one line per run, answers `ssh -G` and prints a set probe output. No
//! connection is made.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

const FAKE_SSH: &str = r#"#!/bin/sh
line="TERM=$TERM"
for arg in "$@"; do line="$line|$arg"; last=$arg; done
echo "$line" >> "$FAKE_SSH_LOG"
case "$1" in
    -G) printf 'user dave\nhostname %s\nport 22\n' "${last#*@}"; exit 0 ;;
esac
case "$last" in
    *tankterm-probe*) cat "$FAKE_SSH_PROBE"; exit "${FAKE_SSH_STATUS:-0}" ;;
esac
exit 0
"#;

const FEDORA: &str = "tankterm-probe\nfound a/alacritty\nfound x/xterm-256color\nfound \
                      x/xterm\nos fedora\ntankterm-probe-end\n";
const MINIMAL_DEBIAN: &str = "Welcome!\ntankterm-probe\nfound x/xterm-256color\nfound x/xterm\nos \
                              debian\ntankterm-probe-end\n";

/// An isolated `HOME`, state and runtime directory, with the fake `ssh`.
struct Env {
    root: TempDir,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for dir in ["bin", "home", "state", "runtime"] {
            fs::create_dir(root.path().join(dir)).unwrap();
        }
        fs::set_permissions(root.path().join("runtime"), fs::Permissions::from_mode(0o700))
            .unwrap();

        let ssh = root.path().join("bin/ssh");
        fs::write(&ssh, FAKE_SSH).unwrap();
        fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();

        fs::write(root.path().join("tankterm.toml"), "[ssh]\nprobe = [\"*.lan\", \"!nas.lan\"]\n")
            .unwrap();

        let env = Self { root };
        env.set_probe(FEDORA);
        env
    }

    fn path(&self, path: &str) -> PathBuf {
        self.root.path().join(path)
    }

    fn set_probe(&self, output: &str) {
        fs::write(self.path("probe"), output).unwrap();
    }

    fn cache(&self) -> PathBuf {
        self.path("state/tankterm/ssh-terms.json")
    }

    /// Run `tankTerm ssh` with `args`, returning its output and the runs of the fake `ssh`.
    fn ssh(&self, args: &[&str], status: Option<&str>) -> (Output, Vec<String>) {
        let _ = fs::remove_file(self.path("log"));

        let mut command = Command::new(env!("CARGO_BIN_EXE_tankTerm"));
        command
            .arg("--config-file")
            .arg(self.path("tankterm.toml"))
            .arg("ssh")
            .args(args)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.path("bin").display()))
            .env("HOME", self.path("home"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .env("TERM", "xterm-kitty")
            .env("FAKE_SSH_LOG", self.path("log"))
            .env("FAKE_SSH_PROBE", self.path("probe"));
        if let Some(status) = status {
            command.env("FAKE_SSH_STATUS", status);
        }

        let output = command.output().unwrap();
        let log = fs::read_to_string(self.path("log")).unwrap_or_default();
        (output, log.lines().map(String::from).collect())
    }

    fn control_path(&self) -> String {
        format!("ControlPath={}", self.path("runtime/tankterm/ssh-%C").display())
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn unlisted_hosts_get_the_fallback() {
    let env = Env::new();

    let (output, runs) = env.ssh(&["-p", "2222", "me@example.com", "ls", "-l"], None);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(runs, [
        "TERM=xterm-kitty|-G|-p|2222|me@example.com",
        "TERM=xterm-256color|-p|2222|me@example.com|ls|-l",
    ]);

    // Excluded by a `!` pattern.
    let (_, runs) = env.ssh(&["nas.lan"], None);
    assert_eq!(runs.last().unwrap(), "TERM=xterm-256color|nas.lan");
    assert!(!env.cache().exists());
}

#[test]
fn listed_hosts_are_probed_once_over_a_shared_connection() {
    let env = Env::new();
    let control_path = env.control_path();

    let (output, runs) = env.ssh(&["-t", "box.lan", "htop"], None);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(runs.len(), 3, "{runs:?}");
    assert_eq!(runs[0], "TERM=xterm-kitty|-G|-t|box.lan");

    // The probe leaves out `-t` and starts the shared connection.
    let probe: Vec<&str> = runs[1].split('|').collect();
    assert_eq!(probe[..9], [
        "TERM=xterm-kitty",
        "-o",
        "ControlMaster=auto",
        "-o",
        &control_path,
        "-o",
        "ControlPersist=30",
        "-o",
        "RequestTTY=no",
    ]);
    assert!(!probe.contains(&"-t"));
    assert_eq!(probe[probe.len() - 2], "box.lan");
    assert!(probe[probe.len() - 1].starts_with("sh -c 'echo tankterm-probe;"));

    // The session reuses it.
    assert_eq!(runs[2], format!("TERM=alacritty|-o|{control_path}|-t|box.lan|htop"));

    let cache = fs::read_to_string(env.cache()).unwrap();
    assert!(cache.contains("\"dave@box.lan:22\""), "{cache}");

    // The second time the cached answer is used, without a probe.
    let (_, runs) = env.ssh(&["box.lan"], None);
    assert_eq!(runs, ["TERM=xterm-kitty|-G|box.lan", "TERM=alacritty|box.lan"]);

    // `--no-probe` doesn't look at the cache or the host.
    let (_, runs) = env.ssh(&["--no-probe", "box.lan"], None);
    assert_eq!(runs, ["TERM=xterm-256color|box.lan"]);
}

#[test]
fn debian_hosts_without_the_entry_get_a_hint() {
    let env = Env::new();
    env.set_probe(MINIMAL_DEBIAN);

    let (output, runs) = env.ssh(&["box.lan"], None);
    assert!(stderr(&output).contains("Installing ncurses-term there adds it"), "{output:?}");
    assert!(runs.last().unwrap().starts_with("TERM=xterm-256color|-o|"));

    // `--reprobe` checks again and replaces the cached answer.
    env.set_probe(FEDORA);
    let (output, runs) = env.ssh(&["--reprobe", "box.lan"], None);
    assert!(!stderr(&output).contains("ncurses-term"));
    assert_eq!(runs.len(), 3);
    assert!(runs[2].starts_with("TERM=alacritty|"));

    let (_, runs) = env.ssh(&["box.lan"], None);
    assert_eq!(runs.last().unwrap(), "TERM=alacritty|box.lan");
}

#[test]
fn probe_flag_checks_unlisted_hosts() {
    let env = Env::new();

    let (_, runs) = env.ssh(&["--probe", "me@example.com"], None);
    assert_eq!(runs.len(), 3);
    assert!(runs[2].starts_with("TERM=alacritty|-o|"));
    assert!(fs::read_to_string(env.cache()).unwrap().contains("\"dave@example.com:22\""));
}

#[test]
fn failed_connections_end_without_a_second_attempt() {
    let env = Env::new();
    env.set_probe("");

    let (output, runs) = env.ssh(&["box.lan"], Some("255"));
    assert_eq!(output.status.code(), Some(255));
    assert_eq!(runs.len(), 2);
    assert!(!env.cache().exists());

    // A probe that ran but printed nothing usable falls back, without caching.
    let (output, runs) = env.ssh(&["box.lan"], Some("1"));
    assert!(output.status.success());
    assert!(stderr(&output).contains("unable to check the terminfo entries of box.lan"));
    assert!(runs[2].starts_with("TERM=xterm-256color|-o|"));
    assert!(!env.cache().exists());
}

#[test]
fn commands_without_a_session_go_to_ssh_unchanged() {
    let env = Env::new();

    let (_, runs) = env.ssh(&["-V"], None);
    assert_eq!(runs, ["TERM=xterm-kitty|-V"]);

    let (_, runs) = env.ssh(&["-G", "box.lan"], None);
    assert_eq!(runs, ["TERM=xterm-kitty|-G|box.lan"]);
}

#[test]
fn ssh_arguments_are_required() {
    let env = Env::new();

    let (output, runs) = env.ssh(&[], None);
    assert!(!output.status.success());
    assert!(runs.is_empty());
}
