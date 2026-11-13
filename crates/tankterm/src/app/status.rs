//! Status segments at the end of the tab bar: what each shows, and the clock and batteries they
//! show, which are looked at with the ticks.

use std::borrow::Cow;
use std::path::Path;

use jiff::Zoned;
use jiff::fmt::strtime;
use log::warn;

use super::App;
use crate::config::UiConfig;
use crate::config::style::TabLayer;
use crate::config::tabs::SegmentKind;
use crate::logging::LOG_TARGET_CONFIG;
use crate::template::Template;

/// A configured status segment, ready to be drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub kind: SegmentKind,
    /// The clock's strftime format.
    format: String,
    /// The placeholders of the other segments.
    template: Template,
    /// Changes to `[style.tab_status]`.
    pub layer: TabLayer,
}

/// The status segments of a configuration. Placeholders a segment doesn't know and clock
/// formats that can't be used are reported.
pub fn segments(config: &UiConfig) -> Vec<Segment> {
    let segments = config.tabs.status.iter().map(|segment| {
        let kind = segment.segment;
        let format = segment.format.clone().unwrap_or_else(|| kind.default_format().to_owned());
        let template = Template::parse(&format);

        if kind == SegmentKind::Clock {
            if let Err(err) = strtime::format(format.as_str(), &Zoned::now()) {
                warn!(target: LOG_TARGET_CONFIG, "Hiding clock with format {format:?}: {err}");
            }
        } else {
            for field in template.fields().filter(|field| !kind.fields().contains(field)) {
                warn!(target: LOG_TARGET_CONFIG, "Unknown placeholder {{{field}}} in {kind:?} segment");
            }
        }

        let layer = config.style.tab_layer(&segment.style, &config.colors);
        Segment { kind, format, template, layer }
    });
    segments.collect()
}

/// The charge of the batteries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    /// As the system says it: `Charging`, `Discharging`, `Full`, `Not charging`.
    pub status: String,
}

impl App {
    /// What a segment shows, or `None` while it has nothing to show.
    pub(super) fn segment_text(&self, segment: &Segment) -> Option<String> {
        let template = &segment.template;
        let kind = segment.kind;

        match segment.kind {
            SegmentKind::KeyTable => {
                let table = self.key_tables.last()?;
                template.render(|name| {
                    known(kind, (name == "name").then(|| table.name.to_string().into()), name)
                })
            },
            SegmentKind::Clock => strtime::format(segment.format.as_str(), self.now.as_ref()?).ok(),
            SegmentKind::Cwd => {
                let directory = self.focused_shell()?.working_directory.as_deref()?;
                template.render(|name| {
                    let value = match name {
                        "path" => Some(display_path(directory).into()),
                        "basename" => Some(basename(directory).into()),
                        _ => None,
                    };
                    known(kind, value, name)
                })
            },
            SegmentKind::Battery => {
                let battery = self.battery.as_ref()?;
                template.render(|name| {
                    let value = match name {
                        "percent" => Some(battery.percent.to_string().into()),
                        "status" => Some(battery.status.as_str().into()),
                        _ => None,
                    };
                    known(kind, value, name)
                })
            },
        }
    }

    /// Whether a segment shows something that changes without a message of its own, so the
    /// ticks have to keep coming.
    pub(super) fn status_ticks(&self) -> bool {
        let ticking = [SegmentKind::Clock, SegmentKind::Cwd, SegmentKind::Battery];
        self.options.status.iter().any(|segment| ticking.contains(&segment.kind))
    }

    /// Whether a segment of this kind is configured.
    pub(super) fn shows(&self, kind: SegmentKind) -> bool {
        self.options.status.iter().any(|segment| segment.kind == kind)
    }

    /// Look at the clock and the batteries again, for the segments that show them.
    pub(super) fn sample_status(&mut self) {
        self.now = self.shows(SegmentKind::Clock).then(Zoned::now);
        self.battery = if self.shows(SegmentKind::Battery) { battery() } else { None };
    }
}

/// The value of a placeholder. Placeholders the segment doesn't know stay as they are written.
fn known<'a>(kind: SegmentKind, value: Option<Cow<'a, str>>, name: &str) -> Option<Cow<'a, str>> {
    match value {
        Some(value) => Some(value),
        None if !kind.fields().contains(&name) => Some(format!("{{{name}}}").into()),
        None => None,
    }
}

/// A directory with the home directory written as `~`.
pub(super) fn display_path(directory: &Path) -> String {
    let home = home::home_dir();
    match home.as_deref().and_then(|home| directory.strip_prefix(home).ok()) {
        Some(relative) if relative.as_os_str().is_empty() => "~".into(),
        Some(relative) => Path::new("~").join(relative).display().to_string(),
        None => directory.display().to_string(),
    }
}

/// The last part of a directory, or `/` for the root.
pub(super) fn basename(directory: &Path) -> String {
    match directory.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => directory.display().to_string(),
    }
}

#[cfg(target_os = "linux")]
fn battery() -> Option<Battery> {
    read_batteries(Path::new("/sys/class/power_supply"))
}

#[cfg(not(target_os = "linux"))]
fn battery() -> Option<Battery> {
    None
}

/// The batteries that power the system among the power supplies in `root`, the Linux
/// `/sys/class/power_supply`. Batteries of devices like mice are left out.
///
/// With several batteries, the charge is that of all of them together, and one charging makes
/// them charging.
#[cfg(any(target_os = "linux", test))]
fn read_batteries(root: &Path) -> Option<Battery> {
    let read = |supply: &Path, name: &str| {
        let value = std::fs::read_to_string(supply.join(name)).ok()?;
        Some(value.trim().to_owned())
    };
    let number = |supply: &Path, name: &str| read(supply, name)?.parse::<u64>().ok();

    let mut supplies: Vec<_> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).collect();
    supplies.sort();
    let batteries: Vec<_> = supplies
        .into_iter()
        .filter(|supply| read(supply, "type").as_deref() == Some("Battery"))
        .filter(|supply| read(supply, "scope").as_deref() != Some("Device"))
        .collect();

    // Energy, or charge on systems that count that instead, tells the batteries' sizes apart.
    let amounts = |now: &str, full: &str| -> Option<(u64, u64)> {
        batteries.iter().try_fold((0, 0), |(sum_now, sum_full), battery| {
            Some((sum_now + number(battery, now)?, sum_full + number(battery, full)?))
        })
    };
    let percent = match amounts("energy_now", "energy_full")
        .or_else(|| amounts("charge_now", "charge_full"))
        .filter(|(_, full)| *full > 0)
    {
        Some((now, full)) => (now * 100 / full).min(100),
        None => {
            let capacities: Vec<_> =
                batteries.iter().filter_map(|battery| number(battery, "capacity")).collect();
            if capacities.is_empty() {
                return None;
            }
            (capacities.iter().sum::<u64>() / capacities.len() as u64).min(100)
        },
    };

    let statuses: Vec<_> = batteries.iter().filter_map(|battery| read(battery, "status")).collect();
    let status = match statuses.iter().find(|status| *status == "Charging") {
        Some(charging) => charging.clone(),
        None => statuses.into_iter().next().unwrap_or_else(|| "Unknown".into()),
    };

    Some(Battery { percent: percent as u8, status })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn supply(root: &Path, name: &str, files: &[(&str, &str)]) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, content) in files {
            fs::write(dir.join(file), format!("{content}\n")).unwrap();
        }
    }

    #[test]
    fn no_battery_shows_nothing() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "AC", &[("type", "Mains"), ("online", "1")]);

        assert_eq!(read_batteries(root.path()), None);
        assert_eq!(read_batteries(&root.path().join("missing")), None);
    }

    #[test]
    fn one_battery() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "AC", &[("type", "Mains")]);
        supply(root.path(), "BAT0", &[
            ("type", "Battery"),
            ("capacity", "57"),
            ("status", "Discharging"),
        ]);

        let battery = read_batteries(root.path()).unwrap();
        assert_eq!(battery, Battery { percent: 57, status: "Discharging".into() });
    }

    #[test]
    fn batteries_add_up_by_energy_and_device_batteries_are_left_out() {
        let root = tempfile::tempdir().unwrap();
        supply(root.path(), "BAT0", &[
            ("type", "Battery"),
            ("energy_now", "20000000"),
            ("energy_full", "80000000"),
            ("capacity", "25"),
            ("status", "Discharging"),
        ]);
        supply(root.path(), "BAT1", &[
            ("type", "Battery"),
            ("energy_now", "20000000"),
            ("energy_full", "20000000"),
            ("capacity", "100"),
            ("status", "Charging"),
        ]);
        // A wireless mouse.
        supply(root.path(), "hidpp_battery_0", &[
            ("type", "Battery"),
            ("scope", "Device"),
            ("capacity", "5"),
            ("status", "Discharging"),
        ]);

        let battery = read_batteries(root.path()).unwrap();
        assert_eq!(battery, Battery { percent: 40, status: "Charging".into() });
    }

    #[test]
    fn paths_shorten_the_home_directory() {
        let Some(home) = home::home_dir() else { return };

        assert_eq!(display_path(&home), "~");
        assert_eq!(display_path(&home.join("src/tankterm")), "~/src/tankterm");
        assert_eq!(display_path(Path::new("/etc")), "/etc");
        assert_eq!(basename(&home.join("src/tankterm")), "tankterm");
        assert_eq!(basename(Path::new("/")), "/");
    }
}
