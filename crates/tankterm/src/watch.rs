//! Watching the configuration files for changes.
//!
//! The files are looked at twice a second from a thread of its own. Next to file system
//! notifications this takes up to a second to notice a change, but it needs nothing from the
//! platform and follows files that editors replace instead of writing to.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use std::{fs, mem, thread};

use iced::Subscription;
use iced::futures::StreamExt;
use iced::futures::channel::mpsc;
use iced::futures::stream::BoxStream;

/// How often the files are looked at.
const INTERVAL: Duration = Duration::from_millis(500);

/// What tells one version of a file from the next: its size and when it was last written.
/// `None` for a file that can't be read.
type Stamp = Option<(u64, SystemTime)>;

/// Tells when files change.
#[derive(Debug)]
pub struct Watcher {
    /// The watched files; the first one is the configuration file itself.
    paths: Vec<PathBuf>,
    /// The files as they were last seen.
    stamps: Vec<Stamp>,
    /// Whether the files changed and haven't been reported yet.
    changed: bool,
}

impl Watcher {
    /// Watch files from how they are now.
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let stamps = stamps(&paths);
        Self { paths, stamps, changed: false }
    }

    /// Look at the files, returning whether they changed.
    ///
    /// A change is only reported once the files are the same for two looks in a row, since
    /// editors save in steps: they truncate and write, or write a new file and move it over
    /// the old one. No change is reported while the first file is missing, which is how the
    /// middle of such a save looks, and what removing the configuration looks like.
    pub fn look(&mut self) -> bool {
        let stamps = stamps(&self.paths);
        if stamps != self.stamps {
            self.stamps = stamps;
            self.changed = true;
            return false;
        }

        let readable = self.stamps.first().is_some_and(Option::is_some);
        readable && mem::take(&mut self.changed)
    }
}

/// How the files at `paths` are right now. Symbolic links are followed.
fn stamps(paths: &[PathBuf]) -> Vec<Stamp> {
    let stamp = |path: &PathBuf| {
        let metadata = fs::metadata(path).ok()?;
        Some((metadata.len(), metadata.modified().ok()?))
    };
    paths.iter().map(stamp).collect()
}

/// An item whenever one of the files at `paths` changes; the first one is the configuration
/// file itself.
///
/// The same paths are the same subscription, so the files are only watched anew when the
/// list changes.
pub fn changes(paths: Vec<PathBuf>) -> Subscription<()> {
    Subscription::run_with(paths, |paths| changes_every(paths.clone(), INTERVAL))
}

/// An item whenever a look at the files every `interval` finds them changed.
fn changes_every(paths: Vec<PathBuf>, interval: Duration) -> BoxStream<'static, ()> {
    let (mut sender, receiver) = mpsc::channel(1);
    let mut watcher = Watcher::new(paths);

    thread::spawn(move || {
        loop {
            thread::sleep(interval);

            // Nobody is listening once the files are no longer watched.
            if sender.is_closed() {
                break;
            }
            // A full channel already holds a change the application hasn't gotten to yet.
            if watcher.look() && sender.try_send(()).is_err_and(|err| err.is_disconnected()) {
                break;
            }
        }
    });

    receiver.boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    /// Write a file so that it can be told from what it was before: with another size.
    fn write(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
    }

    #[test]
    fn changes_are_reported_once_they_have_settled() {
        let directory = tempfile::tempdir().unwrap();
        let (config, import) = (directory.path().join("config"), directory.path().join("import"));
        write(&config, "a");
        write(&import, "a");

        let mut watcher = Watcher::new(vec![config.clone(), import.clone()]);
        assert!(!watcher.look());

        // The look that finds a change waits for the next one to find the same.
        write(&config, "ab");
        assert!(!watcher.look());
        assert!(watcher.look());
        assert!(!watcher.look());

        // A file that is written again in between isn't done changing.
        write(&import, "ab");
        assert!(!watcher.look());
        write(&import, "abc");
        assert!(!watcher.look());
        assert!(watcher.look());
        assert!(!watcher.look());
    }

    #[test]
    fn replaced_files_are_followed() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config");
        write(&config, "a");
        let mut watcher = Watcher::new(vec![config.clone()]);

        // Editors write a new file and move it over the old one.
        let new = directory.path().join("config.new");
        write(&new, "ab");
        fs::rename(&new, &config).unwrap();
        assert!(!watcher.look());
        assert!(watcher.look());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_followed() {
        let directory = tempfile::tempdir().unwrap();
        let (target, link) = (directory.path().join("target"), directory.path().join("link"));
        write(&target, "a");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let mut watcher = Watcher::new(vec![link]);

        write(&target, "ab");
        assert!(!watcher.look());
        assert!(watcher.look());
    }

    #[test]
    fn a_missing_configuration_file_is_waited_for() {
        let directory = tempfile::tempdir().unwrap();
        let (config, import) = (directory.path().join("config"), directory.path().join("import"));
        write(&config, "a");
        let mut watcher = Watcher::new(vec![config.clone(), import.clone()]);

        // The file is gone for as long as a save takes, or for good.
        fs::remove_file(&config).unwrap();
        for _ in 0..3 {
            assert!(!watcher.look());
        }

        write(&config, "ab");
        assert!(!watcher.look());
        assert!(watcher.look());

        // An import that didn't exist is a change when it shows up, and when it goes away.
        write(&import, "a");
        assert!(!watcher.look());
        assert!(watcher.look());
        fs::remove_file(&import).unwrap();
        assert!(!watcher.look());
        assert!(watcher.look());
    }

    #[test]
    fn watching_nothing_reports_nothing() {
        let mut watcher = Watcher::new(Vec::new());
        assert!(!watcher.look());
        assert!(!watcher.look());
    }

    #[test]
    fn changes_arrive_from_the_watching_thread() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("config");
        write(&config, "a");

        let mut changes = changes_every(vec![config.clone()], Duration::from_millis(5));
        write(&config, "ab");
        assert_eq!(iced::futures::executor::block_on(changes.next()), Some(()));

        // The thread ends with the stream; a later change has nobody to go to.
        drop(changes);
        write(&config, "abc");
    }
}
