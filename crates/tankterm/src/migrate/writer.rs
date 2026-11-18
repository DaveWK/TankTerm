//! Writing the planned files: asking before replacing, backing up, and writing atomically.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use tempfile::NamedTempFile;

use crate::cli::MigrateOptions;
use crate::migrate::{Plan, read_answer};

/// Write the plan's files, or print them for a dry run, and print the report.
///
/// Existing files are only replaced after a yes on the terminal (`input`) or with `--force`,
/// and each is copied to a backup first.
pub fn write(
    plan: &Plan,
    options: &MigrateOptions,
    input: Option<&mut dyn BufRead>,
) -> Result<(), String> {
    let existing: Vec<&Path> =
        plan.files.iter().map(|file| file.path.as_path()).filter(|path| path.exists()).collect();

    if options.dry_run {
        if !options.silent {
            for file in &plan.files {
                let path = file.path.display();
                println!("\nv-----Start TOML for {path}-----v\n\n{}", file.contents);
                println!("^-----End TOML for {path}-----^\n");
            }
            print!("{}", plan.report.to_text());
            for path in &existing {
                println!(
                    "Would replace {}, after a backup to {}",
                    path.display(),
                    backup_path(path).display()
                );
            }
            println!("Dry run: nothing was written.");
        }
        return Ok(());
    }

    if !existing.is_empty() && !options.force {
        let list: Vec<_> = existing.iter().map(|path| format!("  {}", path.display())).collect();
        let Some(input) = input else {
            return Err(format!(
                "these files exist already:\n{}\nRun again with --force to replace them; each is \
                 backed up to <name>.bak first. Nothing was written.",
                list.join("\n")
            ));
        };

        eprintln!("These files exist already:\n{}", list.join("\n"));
        eprint!("Replace them? Each is backed up to <name>.bak first. [y/N] ");
        let _ = io::stderr().flush();
        let answer = read_answer(input)?;
        if !matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err("nothing was written".into());
        }
    }

    for path in &existing {
        let backup = backup_path(path);
        fs::copy(path, &backup).map_err(|err| {
            format!("can't back up {} to {}: {err}", path.display(), backup.display())
        })?;
        if !options.silent {
            println!("Backed up {} to {}", path.display(), backup.display());
        }
    }

    for file in &plan.files {
        write_atomically(&file.path, &file.contents)
            .map_err(|err| format!("can't write {}: {err}", file.path.display()))?;
        if !options.silent {
            println!("Wrote {}", file.path.display());
        }
    }

    if !options.silent {
        print!("{}", plan.report.to_text());
    }

    Ok(())
}

/// Where the backup of `path` goes: `<name>.bak`, or `<name>.bak.<n>` when that exists.
fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".bak");
    let first = path.with_file_name(&name);

    let mut backup = first.clone();
    let mut n = 1;
    while backup.exists() {
        let mut numbered = name.clone();
        numbered.push(format!(".{n}"));
        backup = path.with_file_name(numbered);
        n += 1;
    }
    backup
}

/// Replace or create `path` with `contents`, without leaving a half-written file.
fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent directory"))?;
    fs::create_dir_all(dir)?;
    let tmp = NamedTempFile::new_in(dir)?;
    fs::write(tmp.path(), contents)?;
    tmp.persist(path).map_err(|err| err.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    use crate::migrate::output::OutputFile;

    fn options(force: bool) -> MigrateOptions {
        MigrateOptions {
            from: None,
            config_file: None,
            force,
            dry_run: false,
            skip_imports: false,
            silent: true,
        }
    }

    fn plan(dir: &Path) -> Plan {
        Plan {
            files: vec![
                OutputFile { path: dir.join("tankterm.toml"), contents: "new\n".into() },
                OutputFile { path: dir.join("themes/a.toml"), contents: "theme\n".into() },
            ],
            ..Plan::default()
        }
    }

    #[test]
    fn writes_new_files() {
        let dir = tempfile::tempdir().unwrap();
        write(&plan(dir.path()), &options(false), None).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("themes/a.toml")).unwrap(), "theme\n");
    }

    #[test]
    fn refuses_to_replace_without_a_terminal() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tankterm.toml"), "old\n").unwrap();

        let err = write(&plan(dir.path()), &options(false), None).unwrap_err();
        assert!(err.contains("--force"), "{err}");
        assert_eq!(fs::read_to_string(dir.path().join("tankterm.toml")).unwrap(), "old\n");
        assert!(!dir.path().join("themes/a.toml").exists());
    }

    #[test]
    fn asks_before_replacing() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("tankterm.toml");
        fs::write(&config, "old\n").unwrap();

        let mut no = Cursor::new("\n");
        assert!(write(&plan(dir.path()), &options(false), Some(&mut no)).is_err());
        assert_eq!(fs::read_to_string(&config).unwrap(), "old\n");

        let mut yes = Cursor::new("y\n");
        write(&plan(dir.path()), &options(false), Some(&mut yes)).unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), "new\n");
        assert_eq!(fs::read_to_string(dir.path().join("tankterm.toml.bak")).unwrap(), "old\n");
    }

    #[test]
    fn force_keeps_earlier_backups() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("tankterm.toml");
        fs::write(&config, "first\n").unwrap();
        fs::write(dir.path().join("tankterm.toml.bak"), "older\n").unwrap();

        write(&plan(dir.path()), &options(true), None).unwrap();

        assert_eq!(fs::read_to_string(&config).unwrap(), "new\n");
        assert_eq!(fs::read_to_string(dir.path().join("tankterm.toml.bak")).unwrap(), "older\n");
        assert_eq!(fs::read_to_string(dir.path().join("tankterm.toml.bak.1")).unwrap(), "first\n");
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut options = options(false);
        options.dry_run = true;
        write(&plan(dir.path()), &options, None).unwrap();
        assert!(!dir.path().join("tankterm.toml").exists());
    }
}
