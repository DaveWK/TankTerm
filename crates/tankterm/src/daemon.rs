//! Running programs in the background, detached from TankTerm.

use std::ffi::OsStr;
use std::io;
#[cfg(not(windows))]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(windows)]
use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

/// Start a program in the background, in `working_directory` if there is one.
///
/// The program gets no input and its output is dropped. It keeps running when TankTerm exits.
#[cfg(windows)]
pub fn spawn_daemon<I, S>(
    program: &str,
    args: I,
    working_directory: Option<&Path>,
) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }

    // Without a console of its own a console program opens no window.
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW).spawn().map(|_| ())
}

/// Start a program in the background, in `working_directory` if there is one.
///
/// The program gets no input and its output is dropped. It is started in a session of its
/// own by a process that exits right away, so it keeps running when TankTerm exits and
/// leaves no zombie process behind when it ends.
#[cfg(not(windows))]
pub fn spawn_daemon<I, S>(
    program: &str,
    args: I,
    working_directory: Option<&Path>,
) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }

    // SAFETY: the closure runs between fork and exec and only makes async-signal-safe calls.
    unsafe {
        command.pre_exec(|| {
            match libc::fork() {
                -1 => return Err(io::Error::last_os_error()),
                0 => (),
                _ => libc::_exit(0),
            }

            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        });
    }

    // The process waited for is the one that forks and exits.
    command.spawn()?.wait().map(|_| ())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn daemon_runs_in_the_working_directory_without_being_waited_for() {
        let directory = tempfile::tempdir().unwrap();
        let script = "sleep 0.2; pwd > done";

        let started = Instant::now();
        spawn_daemon("sh", ["-c", script], Some(directory.path())).unwrap();
        assert!(!directory.path().join("done").exists(), "the daemon was waited for");

        let deadline = started + Duration::from_secs(60);
        let done = directory.path().join("done");
        while !done.exists() {
            assert!(Instant::now() < deadline, "the daemon never ran");
            thread::sleep(Duration::from_millis(10));
        }
        // Wait for the file to be written, not only created.
        let mut written = String::new();
        while written.is_empty() && Instant::now() < deadline {
            written = std::fs::read_to_string(&done).unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            Path::new(written.trim()).canonicalize().unwrap(),
            directory.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn missing_programs_are_reported() {
        assert!(spawn_daemon("/does/not/exist/tankterm", [""; 0], None).is_err());
    }
}
