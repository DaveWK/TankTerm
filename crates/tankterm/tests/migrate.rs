//! Tests for `tankTerm migrate`, run as a subprocess with `HOME` and the XDG variables pointed
//! at temporary directories. Standard input isn't a terminal here, so nothing is asked.

#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

struct Env {
    _root: TempDir,
    home: PathBuf,
    xdg_config_home: PathBuf,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let xdg_config_home = root.path().join("xdg");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&xdg_config_home).unwrap();
        Self { _root: root, home, xdg_config_home }
    }

    fn write(&self, path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn target(&self) -> PathBuf {
        self.xdg_config_home.join("tankterm/tankterm.toml")
    }

    fn tabby(&self) -> PathBuf {
        self.xdg_config_home.join("tabby/config.yaml")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_tankTerm"))
            .arg("migrate")
            .args(args)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg_config_home)
            .env("XDG_CONFIG_DIRS", self.home.join("no-dirs"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn converts_the_one_config_found() {
    let env = Env::new();
    env.write(&env.tabby(), "terminal:\n  fontSize: 16\n  ligatures: true\n");

    let output = env.run(&[]);

    assert!(output.status.success(), "{}", stderr(&output));
    let written = fs::read_to_string(env.target()).unwrap();
    assert!(written.contains("size = 12.0"), "{written}");
    let stdout = stdout(&output);
    assert!(stdout.contains("Converting Tabby's"), "{stdout}");
    assert!(stdout.contains("Skipped:\n  terminal.ligatures"), "{stdout}");
    // The source is never changed.
    assert_eq!(
        fs::read_to_string(env.tabby()).unwrap(),
        "terminal:\n  fontSize: 16\n  ligatures: true\n"
    );
}

#[test]
fn several_configs_need_from_without_a_terminal() {
    let env = Env::new();
    env.write(&env.tabby(), "terminal:\n  fontSize: 16\n");
    env.write(&env.home.join(".wezterm.lua"), "return { font_size = 10 }\n");

    let output = env.run(&[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--from"), "{}", stderr(&output));
    assert!(!env.target().exists());

    let output = env.run(&["--from", "wezterm"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::read_to_string(env.target()).unwrap().contains("size = 10.0"));
}

#[test]
fn existing_files_need_force_without_a_terminal() {
    let env = Env::new();
    env.write(&env.tabby(), "terminal:\n  fontSize: 16\n");
    env.write(&env.target(), "# mine\n");

    let output = env.run(&[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--force"), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(env.target()).unwrap(), "# mine\n");

    let output = env.run(&["--force"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::read_to_string(env.target()).unwrap().contains("size = 12.0"));
    assert_eq!(fs::read_to_string(env.target().with_extension("toml.bak")).unwrap(), "# mine\n");
}

#[test]
fn dry_run_writes_nothing() {
    let env = Env::new();
    let source = env.home.join("alacritty.yml");
    env.write(&source, "font:\n  size: 9\ndebug:\n  render_timer: true\n");
    env.write(&env.target(), "# mine\n");

    let output = env.run(&["--dry-run", "-c", source.to_str().unwrap()]);

    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("size = 9"), "{stdout}");
    assert!(stdout.contains("debug.render_timer"), "{stdout}");
    assert!(stdout.contains("Would replace"), "{stdout}");
    assert_eq!(fs::read_to_string(env.target()).unwrap(), "# mine\n");
}

#[test]
fn nothing_to_convert() {
    let env = Env::new();

    let output = env.run(&[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("No configuration to convert found"), "{}", stderr(&output));

    let output = env.run(&["--from", "ghostty"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("ghostty/config.ghostty"), "{}", stderr(&output));
}

#[test]
fn alacritty_imports_are_converted_next_to_the_config() {
    let env = Env::new();
    let alacritty = env.xdg_config_home.join("alacritty");
    env.write(
        &alacritty.join("alacritty.toml"),
        "[general]\nimport = [\"themes/dark.toml\"]\n\n[font]\nsize = 11\n",
    );
    env.write(
        &alacritty.join("themes/dark.toml"),
        "[colors.primary]\nbackground = \"#000000\"\n\n[debug]\nprefer_egl = true\n",
    );
    let theme = env.xdg_config_home.join("tankterm/themes/dark.toml");
    env.write(&theme, "# mine\n");

    let output = env.run(&["--force"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let config = fs::read_to_string(env.target()).unwrap();
    assert!(config.contains("import = [\"themes/dark.toml\"]"), "{config}");
    let converted = fs::read_to_string(&theme).unwrap();
    assert!(converted.contains("background = \"#000000\""), "{converted}");
    assert!(!converted.contains("prefer_egl"), "{converted}");
    assert_eq!(fs::read_to_string(theme.with_extension("toml.bak")).unwrap(), "# mine\n");
    assert!(stdout(&output).contains("themes/dark.toml: debug.prefer_egl"), "{}", stdout(&output));
}

#[test]
fn crush_theme_from_the_data_file() {
    let env = Env::new();
    let crush = env.xdg_config_home.join("crush");
    env.write(&crush.join("crush.json"), r#"{ "options": { "tui": { "active_theme": "old" } } }"#);
    // Crush's Themes dialog saves the theme here, and this file wins.
    env.write(
        &env.home.join(".local/share/crush/crush.json"),
        r#"{ "options": { "tui": { "active_theme": "new" } } }"#,
    );
    env.write(
        &crush.join("themes/new.json"),
        r##"{ "bg_base": "#123456", "primary": "#ff0000" }"##,
    );

    let output = env.run(&["--from", "crush", "--dry-run"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("theme = \"charm\""), "{stdout}");
    assert!(stdout.contains("background = \"#123456\""), "{stdout}");
    assert!(stdout.contains("new.primary"), "{stdout}");
}

#[test]
fn an_older_tankterm_config_is_named() {
    let env = Env::new();
    env.write(&env.tabby(), "terminal:\n  fontSize: 16\n");
    env.write(&env.home.join(".tankterm.toml"), "# mine\n");

    let output = env.run(&["--dry-run"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains(".tankterm.toml"), "{stdout}");
    assert!(stdout.contains("takes the place of that file"), "{stdout}");
}
