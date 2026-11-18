//! Status segments at the ends of the tab bar: what each shows, and the clock, batteries and
//! load they show, which are looked at with the ticks.

use std::borrow::Cow;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use jiff::Zoned;
use jiff::fmt::strtime;
use log::warn;

use super::App;
use super::status_command::StatusCommand;
use crate::config::UiConfig;
use crate::config::style::TabLayer;
use crate::config::tabs::{SegmentKind, Side};
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
    /// The end of the tab bar the segment is at.
    pub side: Side,
    /// The program of a `Command` segment.
    pub command: Option<StatusCommand>,
    /// The position of the segment among the status segments.
    index: usize,
    /// Changes to `[style.tab_status]`.
    pub layer: TabLayer,
}

/// The status segments of a configuration. Placeholders a segment doesn't know and clock
/// formats that can't be used are reported.
pub fn segments(config: &UiConfig) -> Vec<Segment> {
    let segments = config.tabs.status.iter().enumerate().map(|(index, segment)| {
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

        // Looking up the domain can take a while, so it starts right away, in the background.
        if kind == SegmentKind::Host && template.fields().any(|field| field == "fqdn") {
            resolve_fqdn();
        }

        let command = match (kind, &segment.command) {
            (SegmentKind::Command, Some(program)) => {
                if segment.interval < 1 {
                    warn!(target: LOG_TARGET_CONFIG, "Running status command every second, not 0");
                }
                Some(StatusCommand {
                    program: program.program().to_owned(),
                    args: program.args().to_vec(),
                    interval: Duration::from_secs(segment.interval.max(1)),
                })
            },
            (SegmentKind::Command, None) => {
                warn!(target: LOG_TARGET_CONFIG, "Hiding Command segment without a command");
                None
            },
            (_, Some(_)) => {
                warn!(target: LOG_TARGET_CONFIG, "Ignoring the command of a {kind:?} segment");
                None
            },
            (_, None) => None,
        };

        let layer = config.style.tab_layer(&segment.style, &config.colors);
        Segment { kind, format, template, side: segment.side, command, index, layer }
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
    /// What a segment shows, or `None` while it has nothing to show, which is also when its
    /// text is empty.
    pub(super) fn segment_text(&self, segment: &Segment) -> Option<String> {
        self.segment_value(segment).filter(|text| !text.is_empty())
    }

    fn segment_value(&self, segment: &Segment) -> Option<String> {
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
            SegmentKind::Zoom => {
                self.tabs.get(self.active)?.panes.maximized()?;
                template.render(|name| known(kind, None, name))
            },
            SegmentKind::Text => template.render(|name| known(kind, None, name)),
            SegmentKind::Count => {
                let tab = self.tabs.get(self.active)?;
                template.render(|name| {
                    let value = match name {
                        "tabs" => Some(self.tabs.len().to_string().into()),
                        "panes" => Some(tab.panes.len().to_string().into()),
                        "tab" => Some((self.active + 1).to_string().into()),
                        _ => None,
                    };
                    known(kind, value, name)
                })
            },
            SegmentKind::Host => template.render(|name| {
                let value = match name {
                    "user" => user_name(),
                    "host" => host_name(),
                    "fqdn" => fqdn(),
                    _ => None,
                };
                known(kind, value.map(Cow::from), name)
            }),
            SegmentKind::Process => {
                let shell = self.focused_shell().filter(|shell| shell.busy)?;
                let process = shell.process.as_deref()?;
                template.render(|name| known(kind, (name == "name").then(|| process.into()), name))
            },
            SegmentKind::Load => {
                let load = self.load?;
                template.render(|name| {
                    let value = match name {
                        "load1" => Some(load[0]),
                        "load5" => Some(load[1]),
                        "load15" => Some(load[2]),
                        _ => None,
                    };
                    known(kind, value.map(|value| format!("{value:.2}").into()), name)
                })
            },
            // The output is a value; it is never read as a format.
            SegmentKind::Command => {
                let output = self.commands.output(segment.index)?;
                template.render(|name| known(kind, (name == "output").then(|| output.into()), name))
            },
        }
    }

    /// Whether a segment shows something that changes without a message of its own, so the
    /// ticks have to keep coming.
    pub(super) fn status_ticks(&self) -> bool {
        let ticking = [
            SegmentKind::Clock,
            SegmentKind::Cwd,
            SegmentKind::Battery,
            SegmentKind::Process,
            SegmentKind::Load,
            SegmentKind::Command,
        ];
        self.options.status.iter().any(|segment| ticking.contains(&segment.kind))
    }

    /// Whether a segment of this kind is configured.
    pub(super) fn shows(&self, kind: SegmentKind) -> bool {
        self.options.status.iter().any(|segment| segment.kind == kind)
    }

    /// Look at the clock, the batteries and the load again, for the segments that show them.
    pub(super) fn sample_status(&mut self) {
        self.now = self.shows(SegmentKind::Clock).then(Zoned::now);
        self.battery = if self.shows(SegmentKind::Battery) { battery() } else { None };
        self.load = if self.shows(SegmentKind::Load) { load_average() } else { None };
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

/// The name of the user TankTerm runs as, from the environment. It is read once.
fn user_name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    let variables: &[&str] = if cfg!(windows) { &["USERNAME"] } else { &["USER", "LOGNAME"] };
    let name = || variables.iter().find_map(|name| std::env::var(name).ok());
    NAME.get_or_init(|| name().filter(|name| !name.is_empty())).as_deref()
}

/// This computer's host name without its domain.
fn host_name() -> Option<&'static str> {
    let name = tankterm_terminal::term::shell_integration::local_host_name()?;
    name.split('.').next().filter(|name| !name.is_empty())
}

/// The fully qualified domain name of the computer, once [`resolve_fqdn`] has found it; until
/// then and without one, the host name as the system gives it.
fn fqdn() -> Option<&'static str> {
    FQDN.get()
        .and_then(Option::as_deref)
        .or_else(tankterm_terminal::term::shell_integration::local_host_name)
}

static FQDN: OnceLock<Option<String>> = OnceLock::new();

/// Look up the fully qualified domain name in the background, the first time this is called.
fn resolve_fqdn() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let lookup = || {
            FQDN.get_or_init(|| {
                let host = tankterm_terminal::term::shell_integration::local_host_name()?;
                lookup_fqdn(host)
            });
        };
        if let Err(err) = std::thread::Builder::new().name("fqdn".into()).spawn(lookup) {
            warn!("Unable to look up the domain name: {err}");
        }
    });
}

/// The canonical name the resolver gives `host`, which has its domain. A name that has a
/// domain already is kept.
#[cfg(unix)]
fn lookup_fqdn(host: &str) -> Option<String> {
    use std::ffi::{CStr, CString};
    use std::ptr;

    if host.contains('.') {
        return Some(host.to_owned());
    }

    let name = CString::new(host).ok()?;
    // SAFETY: `addrinfo` is a plain C struct, for which all zeros is a valid value.
    let mut hints: libc::addrinfo = unsafe { std::mem::zeroed() };
    hints.ai_flags = libc::AI_CANONNAME;
    hints.ai_family = libc::AF_UNSPEC;
    let mut found = ptr::null_mut();

    // SAFETY: The name is a C string, the hints are initialized, and `found` is only read when
    // the call succeeds, and then freed once.
    unsafe {
        if libc::getaddrinfo(name.as_ptr(), ptr::null(), &hints, &mut found) != 0 || found.is_null()
        {
            return None;
        }
        let canonical = (*found).ai_canonname;
        let fqdn = (!canonical.is_null())
            .then(|| CStr::from_ptr(canonical).to_string_lossy().into_owned())
            .filter(|fqdn| !fqdn.is_empty());
        libc::freeaddrinfo(found);
        fqdn
    }
}

/// The host name with the domain Windows gives the user, if it is in one.
#[cfg(not(unix))]
fn lookup_fqdn(host: &str) -> Option<String> {
    match std::env::var("USERDNSDOMAIN") {
        Ok(domain) if !domain.is_empty() && !host.contains('.') => {
            Some(format!("{host}.{}", domain.to_lowercase()))
        },
        _ => Some(host.to_owned()),
    }
}

/// The load average over 1, 5 and 15 minutes.
#[cfg(unix)]
fn load_average() -> Option<[f64; 3]> {
    let mut load = [0.; 3];
    // SAFETY: The buffer is valid for writes of three values.
    let count = unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
    (count == 3).then_some(load)
}

#[cfg(not(unix))]
fn load_average() -> Option<[f64; 3]> {
    None
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

    #[cfg(unix)]
    #[test]
    fn load_and_host_are_known() {
        let load = load_average().unwrap();
        assert!(load.iter().all(|load| *load >= 0.), "{load:?}");

        let host = host_name().unwrap();
        assert!(!host.is_empty() && !host.contains('.'));
        // Without a lookup, or before it is done, the domain name is the host name.
        assert!(fqdn().is_some());
        assert_eq!(lookup_fqdn("build.example.org").as_deref(), Some("build.example.org"));
    }
}
