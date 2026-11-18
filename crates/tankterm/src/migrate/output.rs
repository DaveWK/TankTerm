//! What a migration produces: TankTerm configuration documents and a report.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::config::{KeyBinding, UiConfig};

/// What a migration converted and what it skipped.
#[derive(Debug, Default)]
pub struct Report {
    /// Source keys and the TankTerm keys they became.
    pub converted: Vec<(String, String)>,
    /// Source keys and why they weren't converted.
    pub skipped: Vec<(String, String)>,
}

impl Report {
    pub fn convert(&mut self, from: impl Into<String>, to: impl Into<String>) {
        self.converted.push((from.into(), to.into()));
    }

    pub fn skip(&mut self, from: impl Into<String>, reason: impl Into<String>) {
        self.skipped.push((from.into(), reason.into()));
    }

    /// Add the entries of another report, with a prefix like the name of an imported file.
    pub fn append(&mut self, prefix: &str, other: Report) {
        let prefixed = |(from, to): (String, String)| (format!("{prefix}{from}"), to);
        self.converted.extend(other.converted.into_iter().map(prefixed));
        self.skipped.extend(other.skipped.into_iter().map(prefixed));
    }

    /// The report as text, in two aligned lists.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for (title, entries, separator) in
            [("Converted", &self.converted, "-> "), ("Skipped", &self.skipped, "")]
        {
            if entries.is_empty() {
                continue;
            }
            let width = entries.iter().map(|(from, _)| from.chars().count()).max().unwrap_or(0);
            text.push_str(&format!("{title}:\n"));
            for (from, to) in entries {
                text.push_str(&format!("  {from:width$}  {separator}{to}\n"));
            }
        }
        text
    }
}

/// A TankTerm file a migration writes.
#[derive(Debug)]
pub struct OutputFile {
    pub path: PathBuf,
    pub contents: String,
}

/// Everything a migration writes, and its report.
#[derive(Debug, Default)]
pub struct Plan {
    pub files: Vec<OutputFile>,
    pub report: Report,
}

impl Plan {
    /// A plan of a single file written from `doc`.
    pub fn single(source: &str, source_path: &Path, target: &Path, doc: Doc) -> Self {
        let contents = with_header(source, source_path, &doc.doc.to_string());
        Self { files: vec![OutputFile { path: target.to_owned(), contents }], report: doc.report }
    }
}

/// The text of a converted file, below a comment saying where it came from.
pub fn with_header(source: &str, source_path: &Path, body: &str) -> String {
    let body = body.trim_start_matches('\n');
    format!(
        "# Converted from {source}'s {} by `tankTerm migrate`.\n\n{body}",
        source_path.display()
    )
}

/// A TankTerm configuration document under construction, with its report.
#[derive(Default)]
pub struct Doc {
    pub doc: DocumentMut,
    pub report: Report,
}

impl Doc {
    /// Set the TankTerm key at the dotted path `to`, converted from the source key `from`.
    pub fn set(&mut self, from: &str, to: &str, value: impl Into<Value>) {
        let path: Vec<&str> = to.split('.').collect();
        self.set_at(from, &path, value);
    }

    /// Like [`Self::set`], with the path as a list of keys, which may contain dots.
    pub fn set_at(&mut self, from: &str, path: &[&str], value: impl Into<Value>) {
        let (last, parents) = path.split_last().expect("empty TOML path");
        let table = table_at(&mut self.doc, parents);
        table.insert(last, Item::Value(value.into()));
        self.report.convert(from, path.join("."));
    }

    /// Whether the TankTerm key at the dotted path is set.
    pub fn is_set(&self, to: &str) -> bool {
        let mut item = self.doc.as_item();
        for key in to.split('.') {
            match item.get(key) {
                Some(next) => item = next,
                None => return false,
            }
        }
        true
    }

    /// Record a source key that isn't converted.
    pub fn skip(&mut self, from: impl Into<String>, reason: impl Into<String>) {
        self.report.skip(from, reason);
    }

    /// Add a `[[profiles]]` table.
    pub fn push_profile(&mut self, from: &str, profile: Table) {
        let name = profile.get("name").and_then(|name| name.as_str()).unwrap_or("").to_owned();
        let profiles =
            self.doc.entry("profiles").or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
        if let Some(profiles) = profiles.as_array_of_tables_mut() {
            profiles.push(profile);
        }
        self.report.convert(from, format!("profiles \"{name}\""));
    }

    /// Add a key binding to the array at `path`, like `keyboard.bindings`.
    pub fn push_binding(&mut self, from: &str, path: &[&str], binding: InlineTable) {
        let (last, parents) = path.split_last().expect("empty TOML path");
        let table = table_at(&mut self.doc, parents);
        let array = table.entry(last).or_insert_with(|| {
            let mut array = Array::new();
            array.set_trailing("\n");
            array.set_trailing_comma(true);
            Item::Value(Value::Array(array))
        });

        let description = binding_description(&binding);
        let mut value = Value::InlineTable(binding);
        value.decor_mut().set_prefix("\n    ");
        if let Some(array) = array.as_array_mut() {
            array.push_formatted(value);
        }
        self.report.convert(from, format!("{} {description}", path.join(".")));
    }
}

/// The table at `path`, created where it's missing.
fn table_at<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut table = doc.as_table_mut();
    for key in path {
        let item = table.entry(key).or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        table = item.as_table_mut().expect("TOML path through a value");
    }
    table
}

/// A short description of a binding, like `Control|Shift+T: CreateNewTab`.
pub fn binding_description(binding: &InlineTable) -> String {
    let get = |key: &str| binding.get(key).and_then(|value| value.as_str()).unwrap_or("");
    let mut keys = String::new();
    if !get("mode").is_empty() {
        keys.push_str(&format!("{} ", get("mode")));
    }
    if !get("mods").is_empty() {
        keys.push_str(&format!("{}+", get("mods")));
    }
    keys.push_str(get("key"));

    let what = ["action", "chars", "table"]
        .iter()
        .find_map(|key| {
            binding.get(key).map(|value| match value.as_str() {
                Some(text) if *key == "chars" => format!("{text:?}"),
                Some(text) => text.to_owned(),
                None => value.to_string().trim().to_owned(),
            })
        })
        .or_else(|| binding.get("command").map(|_| "command".into()))
        .unwrap_or_default();
    format!("{keys}: {what}")
}

/// A color as TankTerm writes it, `#rrggbb`, from the forms other terminals use.
///
/// `#rgb`, `#rrggbb`, `#rrggbbaa` (the alpha is dropped), `rrggbb`, `rgb(r, g, b)`,
/// `rgba(r, g, b, a)`, `rgb:rr/gg/bb`, a few X11/CSS names, and CharmTone names are understood.
pub fn color(value: &str) -> Option<String> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let hex = |digits: &str| digits.chars().all(|c| c.is_ascii_hexdigit());

    let rgb = if let Some(digits) = lower.strip_prefix('#').or_else(|| lower.strip_prefix("0x")) {
        match digits.len() {
            3 | 4 if hex(digits) => digits.chars().take(3).flat_map(|c| [c, c]).collect(),
            6 | 8 if hex(digits) => digits[..6].to_owned(),
            _ => return None,
        }
    } else if lower.len() == 6 && hex(&lower) {
        lower.clone()
    } else if let Some(args) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|args| args.strip_suffix(')'))
    {
        let parts: Vec<_> = args.split(',').map(str::trim).collect();
        if parts.len() < 3 {
            return None;
        }
        let mut rgb = String::new();
        for part in &parts[..3] {
            rgb.push_str(&format!("{:02x}", part.parse::<u8>().ok()?));
        }
        rgb
    } else if let Some(channels) = lower.strip_prefix("rgb:") {
        let mut rgb = String::new();
        for channel in channels.split('/') {
            if channel.is_empty() || channel.len() > 4 || !hex(channel) {
                return None;
            }
            let full = u32::from_str_radix(channel, 16).ok()?;
            let max = (1u32 << (4 * channel.len())) - 1;
            rgb.push_str(&format!("{:02x}", full * 255 / max));
        }
        if rgb.len() != 6 {
            return None;
        }
        rgb
    } else if let Some(rgb) = named_color(&lower) {
        rgb.to_owned()
    } else {
        return crate::color::charmtone::color(&lower).map(|rgb| rgb.to_string());
    };

    Some(format!("#{rgb}"))
}

/// The basic X11 and CSS color names.
fn named_color(name: &str) -> Option<&'static str> {
    Some(match name.replace([' ', '_', '-'], "").as_str() {
        "black" => "000000",
        "white" => "ffffff",
        "red" => "ff0000",
        "green" => "00ff00",
        "blue" => "0000ff",
        "yellow" => "ffff00",
        "cyan" | "aqua" => "00ffff",
        "magenta" | "fuchsia" => "ff00ff",
        "gray" | "grey" => "bebebe",
        "darkgray" | "darkgrey" => "a9a9a9",
        "lightgray" | "lightgrey" => "d3d3d3",
        "orange" => "ffa500",
        "purple" => "a020f0",
        "navy" => "000080",
        "maroon" => "b03060",
        "silver" => "c0c0c0",
        "teal" => "008080",
        "olive" => "808000",
        "lime" => "32cd32",
        _ => return None,
    })
}

/// The names of the 16 ANSI colors, as `[colors.normal]` and `[colors.bright]` keys.
pub const ANSI_NAMES: [&str; 8] =
    ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/// The TankTerm key of ANSI color `index` (0 to 15).
pub fn ansi_key(index: usize) -> String {
    let table = if index < 8 { "normal" } else { "bright" };
    format!("colors.{table}.{}", ANSI_NAMES[index % 8])
}

/// Modifier keys of a binding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
}

impl Mods {
    /// Set a modifier by one of its many names; false for a name that isn't a modifier.
    pub fn add(&mut self, name: &str) -> bool {
        match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "⌃" => self.control = true,
            "shift" | "⇧" => self.shift = true,
            "alt" | "opt" | "option" | "meta" | "⌥" => self.alt = true,
            "super" | "cmd" | "command" | "win" | "⌘" => self.command = true,
            _ => return false,
        }
        true
    }

    /// The modifiers as TankTerm writes them, like `Control|Shift`.
    pub fn to_tankterm(self) -> String {
        let names = [
            (self.control, "Control"),
            (self.shift, "Shift"),
            (self.alt, "Alt"),
            (self.command, "Super"),
        ];
        let names: Vec<_> = names.iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect();
        names.join("|")
    }
}

/// What a binding does in TankTerm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Action(&'static str),
    Chars(String),
    Table(String),
}

/// A key binding for `[keyboard] bindings`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    /// The key, as TankTerm names it.
    pub key: String,
    pub mods: Mods,
    /// Only after the leader key.
    pub leader: bool,
    pub target: Target,
}

impl Binding {
    pub fn to_inline_table(&self) -> InlineTable {
        let mut table = InlineTable::new();
        table.insert("key", self.key.as_str().into());
        let mods = self.mods.to_tankterm();
        if !mods.is_empty() {
            table.insert("mods", mods.into());
        }
        if self.leader {
            table.insert("mode", "Leader".into());
        }
        match &self.target {
            Target::Action(action) => table.insert("action", (*action).into()),
            Target::Chars(chars) => table.insert("chars", chars.as_str().into()),
            Target::Table(name) => table.insert("table", name.as_str().into()),
        };
        table
    }
}

/// The bindings TankTerm has without a configuration, with a preset.
pub struct Defaults(Vec<KeyBinding>);

impl Defaults {
    pub fn new(preset: Option<&str>) -> Self {
        let config = match preset {
            Some(preset) => format!("[keyboard]\npreset = \"{preset}\"\n"),
            None => String::new(),
        };
        let config: UiConfig = toml::from_str(&config).unwrap_or_default();
        Self(config.app_bindings_for(cfg!(target_os = "macos")))
    }

    /// Whether the binding is one TankTerm has anyway, outside vi mode and search.
    pub fn contains(&self, binding: &Binding) -> bool {
        let value = toml::Value::try_from(toml_inline(&binding.to_inline_table()));
        let Some(binding) = value.ok().and_then(|value| KeyBinding::deserialize(value).ok()) else {
            return false;
        };
        self.0.iter().any(|default| {
            default.trigger == binding.trigger
                && default.mods == binding.mods
                && default.action == binding.action
                && default.mode == binding.mode
        })
    }
}

/// An inline table as a `toml` value.
pub fn toml_inline(table: &InlineTable) -> toml::Table {
    toml::from_str(&format!("b = {table}"))
        .ok()
        .and_then(|mut outer: toml::Table| outer.remove("b"))
        .and_then(|value| value.as_table().cloned())
        .unwrap_or_default()
}

/// Write converted bindings, leaving out those TankTerm has anyway with `preset`.
pub fn push_bindings(doc: &mut Doc, preset: Option<&str>, bindings: Vec<(String, Binding)>) {
    let defaults = Defaults::new(preset);
    let mut unchanged = 0;
    for (from, binding) in bindings {
        if defaults.contains(&binding) {
            unchanged += 1;
            continue;
        }
        doc.push_binding(&from, &["keyboard", "bindings"], binding.to_inline_table());
    }
    if unchanged > 0 {
        let preset = preset.map(|preset| format!(" with the {preset} preset")).unwrap_or_default();
        doc.report.convert(format!("{unchanged} key bindings"), format!("TankTerm's own{preset}"));
    }
}

/// A shell program and its arguments as a TankTerm `shell` value.
pub fn program(program: &str, args: &[String]) -> Value {
    if args.is_empty() {
        return program.into();
    }
    let mut table = InlineTable::new();
    table.insert("program", program.into());
    table.insert("args", Value::Array(args.iter().map(String::as_str).collect()));
    Value::InlineTable(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(color("#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(color("#39FF14").as_deref(), Some("#39ff14"));
        assert_eq!(color("#88888888").as_deref(), Some("#888888"));
        assert_eq!(color("282a36").as_deref(), Some("#282a36"));
        assert_eq!(color("rgba(1, 2, 255, 0.5)").as_deref(), Some("#0102ff"));
        assert_eq!(color("rgb:ff/80/00").as_deref(), Some("#ff8000"));
        assert_eq!(color("rgb:ffff/0/0").as_deref(), Some("#ff0000"));
        assert_eq!(color("Black").as_deref(), Some("#000000"));
        assert_eq!(color("Charple").as_deref(), Some("#6b50ff"));
        assert_eq!(color("nope"), None);
        assert_eq!(color("#12345"), None);
    }

    #[test]
    fn set_and_report() {
        let mut doc = Doc::default();
        doc.set("a", "colors.primary.background", "#000000");
        doc.set("b", "font.size", 12.0);
        assert!(doc.is_set("font.size"));
        assert!(!doc.is_set("font.normal"));
        assert_eq!(
            doc.doc.to_string(),
            "[colors.primary]\nbackground = \"#000000\"\n\n[font]\nsize = 12.0\n"
        );
        assert_eq!(doc.report.converted[0], ("a".into(), "colors.primary.background".into()));
    }

    #[test]
    fn bindings_leave_out_defaults() {
        let new_tab = Binding {
            key: "T".into(),
            mods: Mods { control: true, shift: true, ..Mods::default() },
            leader: false,
            target: Target::Action("CreateNewTab"),
        };
        let other = Binding { key: "Y".into(), ..new_tab.clone() };

        let mut doc = Doc::default();
        push_bindings(&mut doc, Some("Tabby"), vec![("a".into(), new_tab), ("b".into(), other)]);

        assert_eq!(
            doc.doc.to_string(),
            "[keyboard]\nbindings = [\n    { key = \"Y\", mods = \"Control|Shift\", action = \
             \"CreateNewTab\" },\n]\n"
        );
        let config: UiConfig = toml::from_str(&doc.doc.to_string()).unwrap();
        assert!(config.key_bindings().iter().any(|binding| binding.action
            == crate::config::Action::CreateNewTab
            && binding.mods
                == (winit::keyboard::ModifiersState::CONTROL
                    | winit::keyboard::ModifiersState::SHIFT)));
    }
}
