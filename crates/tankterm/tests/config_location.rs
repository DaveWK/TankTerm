//! Tests for default config file discovery.
//!
//! These run the `tankTerm` binary in a subprocess with an isolated environment, so `HOME` and the
//! XDG variables can be pointed at temporary directories without affecting other tests. The
//! chosen file is observed through `tankTerm migrate --dry-run`, which prints the path of the
//! config it found without modifying anything.

#![cfg(not(windows))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// Isolated `HOME` and XDG directories for one test.
struct Env {
    _root: TempDir,
    home: PathBuf,
    xdg_config_home: PathBuf,
    xdg_config_dirs: PathBuf,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let xdg_config_home = root.path().join("xdg");
        let xdg_config_dirs = root.path().join("xdg-dirs");
        for dir in [&home, &xdg_config_home, &xdg_config_dirs] {
            fs::create_dir_all(dir).unwrap();
        }
        Self { _root: root, home, xdg_config_home, xdg_config_dirs }
    }

    /// Write a minimal config file at `path`, creating parent directories.
    fn write_config(&self, path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "[window]\ndynamic_padding = true\n").unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tankTerm"))
            .args(args)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg_config_home)
            .env("XDG_CONFIG_DIRS", &self.xdg_config_dirs)
            .output()
            .unwrap()
    }

    /// Path of the config file `tankTerm` picks by default.
    fn found_config(&self) -> Option<PathBuf> {
        let output = self.run(&["migrate", "--dry-run"]);
        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout.lines().find_map(|line| line.strip_prefix("Successfully migrated "))?;
        Some(PathBuf::from(line.trim_matches('"')))
    }
}

/// A system-wide config would be found instead of "no config", so tests expecting none skip when
/// one exists.
fn has_system_config() -> bool {
    Path::new("/etc/tankterm").exists()
}

#[test]
fn tankterm_xdg_config() {
    let env = Env::new();
    let config = env.xdg_config_home.join("tankterm/tankterm.toml");
    env.write_config(&config);

    assert_eq!(env.found_config(), Some(config));
}

#[test]
fn alacritty_configs_are_not_used() {
    if has_system_config() {
        return;
    }

    let env = Env::new();
    env.write_config(&env.xdg_config_home.join("alacritty/alacritty.toml"));
    env.write_config(&env.home.join(".alacritty.toml"));

    assert_eq!(env.found_config(), None);
}

#[test]
fn tankterm_locations_keep_upstream_order() {
    let env = Env::new();
    let xdg = env.xdg_config_home.join("tankterm/tankterm.toml");
    env.write_config(&xdg);
    env.write_config(&env.home.join(".tankterm.toml"));

    assert_eq!(env.found_config(), Some(xdg));
}

#[test]
fn no_config_found() {
    if has_system_config() {
        return;
    }

    let env = Env::new();
    let output = env.run(&["migrate", "--dry-run"]);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No configuration file found"));
}

#[test]
fn command_name_is_tankterm() {
    let output = Env::new().run(&["--version"]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert!(output.status.success());
    assert!(stdout.starts_with("tankTerm "), "unexpected version output: {stdout:?}");
}
