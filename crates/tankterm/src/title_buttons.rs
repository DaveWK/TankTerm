//! The window buttons of a tab bar that is the title bar, and where the desktop puts them.

use crate::config::window::WindowButtons;

/// A window button on the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleButton {
    Minimize,
    Maximize,
    Close,
}

use TitleButton::{Close, Maximize, Minimize};

/// The window buttons at each end of the tab bar, in the order they are drawn from left to
/// right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitleButtons {
    pub left: Vec<TitleButton>,
    pub right: Vec<TitleButton>,
}

/// All three at the right end, with the close button last.
impl Default for TitleButtons {
    fn default() -> Self {
        Self { left: Vec::new(), right: vec![Minimize, Maximize, Close] }
    }
}

impl TitleButtons {
    /// The buttons `window.buttons` asks for, or `None` if the tab bar isn't the title bar.
    /// `desktop` gives the buttons for `"Desktop"`.
    pub fn new(buttons: WindowButtons, desktop: impl FnOnce() -> Self) -> Option<Self> {
        match buttons {
            WindowButtons::Desktop => Some(desktop()),
            WindowButtons::Right => Some(Self::default()),
            WindowButtons::Left => {
                Some(Self { left: vec![Close, Maximize, Minimize], right: Vec::new() })
            },
            WindowButtons::None => None,
        }
    }

    /// The buttons where the desktop puts them.
    #[cfg(target_os = "macos")]
    pub fn desktop() -> Self {
        Self { left: vec![Close, Minimize, Maximize], right: Vec::new() }
    }

    /// The buttons where the desktop puts them: KDE's and GNOME's button layout, and the
    /// default elsewhere.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn desktop() -> Self {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        Self::for_desktop(&desktop, unix::kwinrc, unix::gnome_layout)
    }

    /// The buttons where the desktop puts them.
    #[cfg(not(unix))]
    pub fn desktop() -> Self {
        Self::default()
    }

    /// The buttons where `desktop`, a value of `XDG_CURRENT_DESKTOP`, puts them. KDE's are
    /// read from the contents of its `kwinrc` files, the user's first, and GNOME's from its
    /// `button-layout` setting.
    #[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
    fn for_desktop(
        desktop: &str,
        kwinrc: impl FnOnce() -> Vec<String>,
        gnome_layout: impl FnOnce() -> Option<String>,
    ) -> Self {
        let mut desktops = desktop.split(':');
        if desktops.clone().any(|desktop| desktop.eq_ignore_ascii_case("KDE")) {
            let files = kwinrc();
            let value = |key: &str, default: &str| {
                let mut values = files.iter().filter_map(|file| kde_value(file, key));
                values.next().unwrap_or_else(|| default.to_owned())
            };
            // KWin's defaults.
            return Self::kde(&value("ButtonsOnLeft", "MS"), &value("ButtonsOnRight", "HIAX"));
        }

        if desktops.any(|desktop| desktop.eq_ignore_ascii_case("GNOME"))
            && let Some(layout) = gnome_layout()
        {
            return Self::gnome(&layout);
        }

        Self::default()
    }

    /// The buttons of KWin's `ButtonsOnLeft` and `ButtonsOnRight`, where `I` is minimize,
    /// `A` maximize and `X` close. Its other buttons aren't drawn.
    fn kde(left: &str, right: &str) -> Self {
        let button = |code| match code {
            'I' => Some(Minimize),
            'A' => Some(Maximize),
            'X' => Some(Close),
            _ => None,
        };
        Self::from_sides(left.chars().filter_map(button), right.chars().filter_map(button))
    }

    /// The buttons of GNOME's `button-layout`, like `'appmenu:minimize,maximize,close'`: the
    /// left end's buttons before the colon, the right end's after it.
    fn gnome(layout: &str) -> Self {
        let layout = layout.trim().trim_matches(|c| c == '\'' || c == '"');
        let (left, right) = layout.split_once(':').unwrap_or((layout, ""));
        let buttons = |side: &str| {
            let names = side.split(',').map(str::trim);
            names
                .filter_map(|name| match name {
                    "minimize" => Some(Minimize),
                    "maximize" => Some(Maximize),
                    "close" => Some(Close),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        Self::from_sides(buttons(left), buttons(right))
    }

    /// Each button once, where it first appears.
    fn from_sides(
        left: impl IntoIterator<Item = TitleButton>,
        right: impl IntoIterator<Item = TitleButton>,
    ) -> Self {
        let mut seen = Vec::new();
        let mut once = |button: &TitleButton| {
            let new = !seen.contains(button);
            seen.push(*button);
            new
        };
        let left = left.into_iter().filter(&mut once).collect();
        let right = right.into_iter().filter(&mut once).collect();
        Self { left, right }
    }
}

/// The value of `key` in the `[org.kde.kdecoration2]` group of a KDE config file.
///
/// Keys can be marked immutable with `[$i]`, which is ignored here.
fn kde_value(file: &str, key: &str) -> Option<String> {
    let mut in_group = false;
    for line in file.lines().map(str::trim) {
        if line.starts_with('[') && !line.contains('=') {
            in_group = line == "[org.kde.kdecoration2]";
            continue;
        }
        if !in_group {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else { continue };
        let name = name.split('[').next().unwrap_or_default().trim();
        if name == key {
            return Some(value.trim().to_owned());
        }
    }
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
mod unix {
    use std::env;
    use std::path::PathBuf;
    use std::process::Command;

    /// The contents of the `kwinrc` files that exist, in the order KDE reads values from them:
    /// the user's, the defaults of the user's global theme, then the system's.
    pub fn kwinrc() -> Vec<String> {
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
        let config_dirs = env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".into());

        let user = config_home
            .iter()
            .flat_map(|home| [home.join("kwinrc"), home.join("kdedefaults/kwinrc")]);
        let system = config_dirs.split(':').map(|dir| PathBuf::from(dir).join("kwinrc"));
        user.chain(system).filter_map(|path| std::fs::read_to_string(path).ok()).collect()
    }

    /// GNOME's `button-layout` setting, as `gsettings` prints it.
    pub fn gnome_layout() -> Option<String> {
        let output = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
            .output()
            .ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buttons(left: &[TitleButton], right: &[TitleButton]) -> TitleButtons {
        TitleButtons { left: left.to_vec(), right: right.to_vec() }
    }

    fn unused<T>() -> T {
        panic!("not asked for on this desktop")
    }

    #[test]
    fn configured_sides() {
        let desktop = || buttons(&[Close], &[]);
        assert_eq!(TitleButtons::new(WindowButtons::Desktop, desktop), Some(desktop()));
        assert_eq!(
            TitleButtons::new(WindowButtons::Right, unused),
            Some(buttons(&[], &[Minimize, Maximize, Close]))
        );
        assert_eq!(
            TitleButtons::new(WindowButtons::Left, unused),
            Some(buttons(&[Close, Maximize, Minimize], &[]))
        );
        assert_eq!(TitleButtons::new(WindowButtons::None, unused), None);
    }

    #[test]
    fn kde_layout() {
        let kwinrc = "[Desktops]\nNumber=4\n\n[org.kde.kdecoration2]\nButtonsOnLeft=XS\\
                      nButtonsOnRight[$i]=HIA\nlibrary=org.kde.breeze\n";
        let read = || vec![kwinrc.to_owned()];
        assert_eq!(
            TitleButtons::for_desktop("KDE", read, unused),
            buttons(&[Close], &[Minimize, Maximize])
        );

        // Values missing from the user's file come from the next one, then from KWin's
        // defaults.
        let user = "[org.kde.kdecoration2]\nButtonsOnLeft=IAX\n";
        let system = "[org.kde.kdecoration2]\nButtonsOnLeft=X\nButtonsOnRight=MI\n";
        let read = || vec![user.to_owned(), system.to_owned()];
        assert_eq!(
            TitleButtons::for_desktop("KDE", read, unused),
            buttons(&[Minimize, Maximize, Close], &[])
        );
        assert_eq!(
            TitleButtons::for_desktop("KDE", Vec::new, unused),
            buttons(&[], &[Minimize, Maximize, Close])
        );

        // The key has to be in KWin's decoration group.
        let other = "[Windows]\nButtonsOnLeft=X\n";
        assert_eq!(
            TitleButtons::for_desktop("KDE", || vec![other.to_owned()], unused),
            buttons(&[], &[Minimize, Maximize, Close])
        );
    }

    #[test]
    fn gnome_layout() {
        let layout = |value: &'static str| {
            TitleButtons::for_desktop("ubuntu:GNOME", unused, || Some(value.to_owned()))
        };
        assert_eq!(layout("'appmenu:close'\n"), buttons(&[], &[Close]));
        assert_eq!(layout("'close,minimize:appmenu'"), buttons(&[Close, Minimize], &[]));
        assert_eq!(
            layout("'appmenu:minimize,maximize,close'"),
            buttons(&[], &[Minimize, Maximize, Close])
        );
        // Each button is drawn once.
        assert_eq!(layout("'close:close,maximize'"), buttons(&[Close], &[Maximize]));

        // Without `gsettings`, the buttons are on the right.
        assert_eq!(
            TitleButtons::for_desktop("GNOME", unused, || None),
            buttons(&[], &[Minimize, Maximize, Close])
        );
    }

    #[test]
    fn other_desktops() {
        assert_eq!(TitleButtons::for_desktop("XFCE", unused, unused), TitleButtons::default());
        assert_eq!(TitleButtons::for_desktop("", unused, unused), TitleButtons::default());
    }
}
