//! Alacritty: TOML files and legacy YAML files, with their imports.
//!
//! TankTerm's options are Alacritty's, so a file is copied with Alacritty's historical renames
//! applied and the options TankTerm doesn't have taken out.

use std::collections::HashMap;
use std::mem;
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
#[cfg(test)]
use toml_edit::Value;
use toml_edit::{DocumentMut, InlineTable, Item, Value as TomlValue};

use crate::config::{self, KeyBinding, MouseBinding};
use crate::migrate::dirs;
use crate::migrate::output::{
    OutputFile, Plan, Report, binding_description, toml_inline, with_header,
};

/// Alacritty's configuration files, in the order it looks for them: TOML before YAML.
pub fn locations() -> Vec<PathBuf> {
    let mut locations = Vec::new();
    for suffix in ["toml", "yml"] {
        let file = format!("alacritty.{suffix}");

        #[cfg(not(windows))]
        {
            if let Some(config_home) = dirs::config_home() {
                locations.push(config_home.join("alacritty").join(&file));
                locations.push(config_home.join(&file));
            }
            for dir in dirs::config_dirs() {
                locations.push(dir.join("alacritty").join(&file));
            }
            if let Some(home) = dirs::home() {
                locations.push(home.join(".config").join("alacritty").join(&file));
                locations.push(home.join(format!(".{file}")));
            }
            locations.push(PathBuf::from("/etc/alacritty").join(&file));
        }

        #[cfg(windows)]
        if let Some(app_data) = dirs::app_data() {
            locations.push(app_data.join("alacritty").join(&file));
        }
    }
    locations
}

/// Plan the conversion of the Alacritty file at `path` into `target`, with its imports.
pub fn plan(path: &Path, target: &Path, skip_imports: bool) -> Result<Plan, String> {
    let root_dir = path.parent().unwrap_or(Path::new("/")).to_owned();
    let target_dir = target.parent().unwrap_or(Path::new("/")).to_owned();
    let path = &lexical(path);
    let mut planner = Planner {
        root_dir,
        target_dir,
        skip_imports,
        plan: Plan::default(),
        converted: HashMap::new(),
    };
    planner.convert(path, target.to_owned(), "", config::IMPORT_RECURSION_LIMIT)?;
    Ok(planner.plan)
}

struct Planner {
    /// The directory of the main Alacritty file, which imports are mirrored from.
    root_dir: PathBuf,
    /// TankTerm's configuration directory.
    target_dir: PathBuf,
    skip_imports: bool,
    plan: Plan,
    /// Converted source files and where they went.
    converted: HashMap<PathBuf, PathBuf>,
}

impl Planner {
    /// Convert `path` into `target`, and the files it imports.
    fn convert(
        &mut self,
        path: &Path,
        target: PathBuf,
        prefix: &str,
        recursion_limit: usize,
    ) -> Result<(), String> {
        self.converted.insert(path.to_owned(), target.clone());

        let mut report = Report::default();
        let mut document = read(path)?;
        migrate_toml(&mut document, &mut report)?;
        prune(&mut document, &mut report);
        prune_bindings(&mut document, &mut report);
        self.imports(path, &target, &mut document, &mut report, recursion_limit)?;
        copied(&document, &mut report);

        let contents = with_header("Alacritty", path, &document.to_string());
        self.plan.files.push(OutputFile { path: target, contents });
        self.plan.report.append(prefix, report);
        Ok(())
    }

    /// Convert the imports of `document` and point its import entries at the results.
    fn imports(
        &mut self,
        path: &Path,
        target: &Path,
        document: &mut DocumentMut,
        report: &mut Report,
        recursion_limit: usize,
    ) -> Result<(), String> {
        let Some(imports) = document
            .get_mut("general")
            .and_then(|general| general.get_mut("import"))
            .and_then(Item::as_array_mut)
        else {
            return Ok(());
        };

        let entries: Vec<String> =
            imports.iter().filter_map(|import| import.as_str().map(ToOwned::to_owned)).collect();
        let mut new_entries = Vec::new();
        for entry in entries {
            let source = lexical(&config::normalize_import(path, &entry));
            let original = source.to_string_lossy().into_owned();

            if self.skip_imports {
                report.skip(
                    format!("general.import {entry}"),
                    "--skip-imports: kept the original file",
                );
                new_entries.push(original);
                continue;
            }
            if !source.is_file() {
                report.skip(format!("general.import {entry}"), "file not found; kept the path");
                new_entries.push(original);
                continue;
            }
            if recursion_limit == 0 {
                return Err("exceeded the maximum depth of configuration imports".into());
            }

            let import_target = match self.converted.get(&source) {
                Some(import_target) => import_target.clone(),
                None => {
                    let import_target = self.import_target(&source);
                    let prefix = format!("{}: ", import_target.display());
                    self.convert(&source, import_target.clone(), &prefix, recursion_limit - 1)?;
                    import_target
                },
            };

            report.convert(format!("general.import {entry}"), import_target.display().to_string());
            let target_dir = target.parent().unwrap_or(Path::new("/"));
            let relative = import_target.strip_prefix(target_dir).unwrap_or(&import_target);
            new_entries.push(relative.to_string_lossy().into_owned());
        }

        let imports = document["general"]["import"].as_array_mut().expect("import array");
        imports.clear();
        for entry in new_entries {
            imports.push(entry);
        }
        Ok(())
    }

    /// Where a converted import goes: its place relative to the main Alacritty file, or
    /// `imports/<name>.toml` for files outside its directory.
    fn import_target(&self, source: &Path) -> PathBuf {
        let mut target = match source.strip_prefix(&self.root_dir) {
            Ok(relative) => self.target_dir.join(relative),
            Err(_) => self.target_dir.join("imports").join(source.file_name().unwrap_or_default()),
        };
        target.set_extension("toml");

        // Two imports with the same name from different places.
        let taken = |path: &Path| self.converted.values().any(|target| target == path);
        let stem = target.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let mut n = 2;
        while taken(&target) {
            target.set_file_name(format!("{stem}-{n}.toml"));
            n += 1;
        }
        target
    }
}

/// The path without `.` and `..` components, resolved without looking at the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            Component::ParentDir => {
                if !result.pop() {
                    result.push(component);
                }
            },
            _ => result.push(component),
        }
    }
    result
}

/// Read a TOML or YAML file as a TOML document.
fn read(path: &Path) -> Result<DocumentMut, String> {
    let is_yaml = path.extension().is_some_and(|ext| ext == "yml" || ext == "yaml");
    let toml = if is_yaml {
        let value = config::deserialize_config(path, false)
            .map_err(|err| format!("{}: YAML parsing error: {err}", path.display()))?;
        toml::to_string(&value)
            .map_err(|err| format!("{}: conversion error: {err}", path.display()))?
    } else {
        std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?
    };

    let toml = toml.trim_start_matches('\u{FEFF}');
    toml.parse::<DocumentMut>()
        .map_err(|err| format!("{}: TOML parsing error: {err}", path.display()))
}

/// Apply Alacritty's renames of options.
fn migrate_toml(document: &mut DocumentMut, report: &mut Report) -> Result<(), String> {
    let moves: &[(&[&str], &[&str])] = &[
        (&["draw_bold_text_with_bright_colors"], &["colors", "draw_bold_text_with_bright_colors"]),
        (&["key_bindings"], &["keyboard", "bindings"]),
        (&["mouse_bindings"], &["mouse", "bindings"]),
        (&["live_config_reload"], &["general", "live_config_reload"]),
        (&["working_directory"], &["general", "working_directory"]),
        (&["ipc_socket"], &["general", "ipc_socket"]),
        (&["import"], &["general", "import"]),
        (&["shell"], &["terminal", "shell"]),
        (&["background_opacity"], &["window", "opacity"]),
        (&["visual_bell"], &["bell"]),
        (&["window", "gtk_theme_variant"], &["window", "decorations_theme_variant"]),
    ];

    for (origin, target) in moves {
        if move_value(document, origin, target)? {
            report.convert(origin.join("."), target.join("."));
        }
    }
    Ok(())
}

/// TankTerm's options, by table; `None` takes any key.
const OPTIONS: &[(&str, Option<&[&str]>)] = &[
    (
        "general",
        Some(&["import", "working_directory", "live_config_reload", "ipc_socket", "accessible"]),
    ),
    ("env", None),
    ("scrolling", Some(&["history", "multiplier"])),
    (
        "cursor",
        Some(&[
            "style",
            "vi_mode_style",
            "blink_interval",
            "blink_timeout",
            "unfocused_hollow",
            "thickness",
        ]),
    ),
    ("selection", Some(&["semantic_escape_chars", "save_to_clipboard"])),
    (
        "font",
        Some(&[
            "normal",
            "bold",
            "italic",
            "bold_italic",
            "size",
            "offset",
            "glyph_offset",
            "builtin_box_drawing",
        ]),
    ),
    (
        "window",
        Some(&[
            "dimensions",
            "position",
            "padding",
            "dynamic_padding",
            "decorations",
            "buttons",
            "opacity",
            "blur",
            "startup_mode",
            "title",
            "dynamic_title",
            "class",
            "decorations_theme_variant",
            "resize_increments",
            "option_as_alt",
            "level",
        ]),
    ),
    ("tabs", None),
    ("profiles", None),
    ("mouse", Some(&["hide_when_typing", "bindings"])),
    ("debug", Some(&["log_level", "persistent_logging"])),
    ("bell", Some(&["animation", "spring", "duration", "color", "command"])),
    ("theme", None),
    (
        "colors",
        Some(&[
            "mode",
            "primary",
            "cursor",
            "vi_mode_cursor",
            "search",
            "hints",
            "line_indicator",
            "footer_bar",
            "selection",
            "normal",
            "bright",
            "dim",
            "indexed_colors",
            "transparent_background_colors",
            "draw_bold_text_with_bright_colors",
        ]),
    ),
    ("style", None),
    ("hints", Some(&["alphabet", "enabled"])),
    ("terminal", Some(&["shell", "osc52", "confirm_multiline_paste"])),
    ("keyboard", Some(&["preset", "leader", "tables", "bindings"])),
];

/// Take out the options TankTerm doesn't have.
fn prune(document: &mut DocumentMut, report: &mut Report) {
    let table = document.as_table_mut();
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_owned()).collect();
    for key in keys {
        let Some((_, known)) = OPTIONS.iter().find(|(name, _)| *name == key) else {
            table.remove(&key);
            report.skip(&key, "not a TankTerm option");
            continue;
        };
        let (Some(known), Some(inner)) =
            (known, table.get_mut(&key).and_then(Item::as_table_like_mut))
        else {
            continue;
        };

        let inner_keys: Vec<String> = inner.iter().map(|(key, _)| key.to_owned()).collect();
        for inner_key in inner_keys {
            if known.contains(&inner_key.as_str()) {
                continue;
            }
            inner.remove(&inner_key);
            let path = format!("{key}.{inner_key}");
            let reason = match path.as_str() {
                "debug.render_timer"
                | "debug.renderer"
                | "debug.print_events"
                | "debug.highlight_damage"
                | "debug.prefer_egl"
                | "debug.ref_test" => "an option of Alacritty's renderer; TankTerm draws with iced",
                "font.use_thin_strokes" => {
                    "removed; set the AppleFontSmoothing user default instead"
                },
                _ => "not a TankTerm option",
            };
            report.skip(path, reason);
        }
    }
}

/// Take out the key and mouse bindings TankTerm can't read, like those of actions it doesn't
/// have, which it would leave out with an error when it loads the file.
fn prune_bindings(document: &mut DocumentMut, report: &mut Report) {
    prune_bindings_of::<KeyBinding>(document, "keyboard", report);
    prune_bindings_of::<MouseBinding>(document, "mouse", report);
}

fn prune_bindings_of<B: DeserializeOwned>(
    document: &mut DocumentMut,
    table: &str,
    report: &mut Report,
) {
    let Some(item) = document.get_mut(table).and_then(|table| table.get_mut("bindings")) else {
        return;
    };

    // Why each binding can't be read, if it can't.
    let check = |binding: InlineTable| -> Option<(String, String)> {
        let value = toml::Value::Table(toml_inline(&binding));
        let err = B::deserialize(value).err()?;
        // Serde lists every action it knows after the name it didn't.
        let err = err.to_string();
        let reason = err.split(", expected").next().unwrap_or(&err).trim().to_owned();
        Some((binding_description(&binding), reason))
    };

    let mut index = 0;
    let mut skipped = Vec::new();
    let mut keep = |binding: InlineTable| {
        index += 1;
        match check(binding) {
            Some((description, reason)) => {
                skipped.push((format!("{table}.bindings[{index}] {description}"), reason));
                false
            },
            None => true,
        }
    };
    match item {
        Item::ArrayOfTables(bindings) => {
            bindings.retain(|binding| keep(binding.clone().into_inline_table()))
        },
        Item::Value(TomlValue::Array(bindings)) => bindings.retain(|binding| match binding {
            TomlValue::InlineTable(binding) => keep(binding.clone()),
            _ => keep(InlineTable::new()),
        }),
        _ => (),
    }

    for (from, reason) in skipped {
        report.skip(from, reason);
    }
}

/// Report the tables that are copied as they are.
fn copied(document: &DocumentMut, report: &mut Report) {
    for (key, item) in document.iter() {
        if key == "general"
            && item.get("import").is_some()
            && item.as_table_like().is_some_and(|t| t.len() == 1)
        {
            continue;
        }
        report.convert(key, format!("{key} (as it is)"));
    }
}

/// Move a TOML value from one map to another; false when there's nothing to move.
fn move_value(
    document: &mut DocumentMut,
    origin: &[&str],
    target: &[&str],
) -> Result<bool, String> {
    // Find and remove the original item.
    let (mut origin_key, mut origin_item) = (None, document.as_item_mut());
    for element in origin {
        let table = match origin_item.as_table_like_mut() {
            Some(table) => table,
            None => return Err(format!("{} isn't a table", origin.join("."))),
        };

        let (key, item) = match table.get_key_value_mut(element) {
            Some((key, item)) => (key, item),
            None => return Ok(false),
        };

        origin_key = Some(key);
        origin_item = item;

        // Ensure no empty tables are left behind.
        if let Some(table) = origin_item.as_table_mut() {
            table.set_implicit(true)
        }
    }

    let origin_key_decor =
        origin_key.map(|key| (key.leaf_decor().clone(), key.dotted_decor().clone()));
    let origin_item = mem::replace(origin_item, Item::None);

    // Create all dependencies for the new location.
    let mut target_item = document.as_item_mut();
    for (i, element) in target.iter().enumerate() {
        let table = match target_item.as_table_like_mut() {
            Some(table) => table,
            None => return Err(format!("{} isn't a table", target.join("."))),
        };

        if i + 1 == target.len() {
            table.insert(element, origin_item);
            // Move original key decorations.
            if let Some((leaf, dotted)) = origin_key_decor {
                let mut key = table.key_mut(element).unwrap();
                *key.leaf_decor_mut() = leaf;
                *key.dotted_decor_mut() = dotted;
            }

            break;
        } else {
            // Create missing parent tables.
            target_item = target_item[element].or_insert(toml_edit::table());
        }
    }

    Ok(true)
}

/// The import entries of a converted document.
#[cfg(test)]
fn import_entries(contents: &str) -> Vec<String> {
    let document = contents.parse::<DocumentMut>().unwrap();
    document["general"]["import"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use crate::config::UiConfig;

    #[test]
    fn move_values() {
        let input = r#"
# This is a root_value.
#
# Use it with care.
root_value = 3

[table]
table_value = 5

[preexisting]
not_moved = 9
        "#;

        let mut document = input.parse::<DocumentMut>().unwrap();

        move_value(&mut document, &["root_value"], &["new_table", "root_value"]).unwrap();
        move_value(&mut document, &["table", "table_value"], &[
            "preexisting",
            "subtable",
            "new_name",
        ])
        .unwrap();

        let output = document.to_string();

        let expected = r#"
[preexisting]
not_moved = 9

[preexisting.subtable]
new_name = 5

[new_table]

# This is a root_value.
#
# Use it with care.
root_value = 3
        "#;

        assert_eq!(output, expected);
    }

    #[test]
    fn migrate_empty() {
        let mut document = DocumentMut::new();
        migrate_toml(&mut document, &mut Report::default()).unwrap();
        assert!(document.to_string().is_empty());
    }

    #[test]
    fn renames_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("alacritty/alacritty.toml");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(
            &source,
            "# My colors.\nlive_config_reload = false\nbackground_opacity = \
             0.8\n\n[debug]\nrender_timer = true\nlog_level = \"Info\"\n\n[mouse]\ndouble_click = \
             { threshold = 300 }\n\n[colors.primary]\nbackground = \
             \"#101010\"\n\n[[keyboard.bindings]]\nkey = \"N\"\nmods = \"Control|Shift\"\naction \
             = \"CreateNewWindow\"\n\n[[keyboard.bindings]]\nkey = \"Q\"\nmods = \
             \"Control\"\naction = \"ToggleChaos\"\n",
        )
        .unwrap();
        let target = dir.path().join("tankterm/tankterm.toml");

        let plan = plan(&source, &target, false).unwrap();

        assert_eq!(plan.files.len(), 1);
        let contents = &plan.files[0].contents;
        assert!(contents.starts_with("# Converted from Alacritty's"), "{contents}");
        assert!(contents.contains("# My colors."), "{contents}");
        assert!(!contents.contains("render_timer"), "{contents}");
        assert!(!contents.contains("double_click"), "{contents}");

        let config: UiConfig = toml::from_str(contents).unwrap();
        assert!(!config.live_config_reload());
        assert_eq!(config.window_opacity(), 0.8);
        assert_eq!(config.debug.log_level, log::LevelFilter::Info);
        assert_eq!(config.colors.primary.background, crate::color::Rgb::new(0x10, 0x10, 0x10));

        let skipped: Vec<_> = plan.report.skipped.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(skipped, [
            "debug.render_timer",
            "mouse.double_click",
            "keyboard.bindings[2] Control+Q: ToggleChaos"
        ]);
        assert!(contents.contains("CreateNewWindow"), "{contents}");
        assert!(!contents.contains("ToggleChaos"), "{contents}");
        assert!(
            plan.report.converted.contains(&("background_opacity".into(), "window.opacity".into()))
        );
    }

    #[test]
    fn imports_are_converted_next_to_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let alacritty = dir.path().join("alacritty");
        fs::create_dir_all(alacritty.join("themes")).unwrap();
        fs::write(
            alacritty.join("alacritty.yml"),
            "import:\n  - themes/dark.yml\n  - ~/does-not-exist.toml\n  - ../shared.toml\nfont:\n  size: 12\n",
        )
        .unwrap();
        fs::write(
            alacritty.join("themes/dark.yml"),
            "colors:\n  primary:\n    background: '#000000'\n",
        )
        .unwrap();
        fs::write(dir.path().join("shared.toml"), "[debug]\nprefer_egl = true\n").unwrap();
        let target = dir.path().join("tankterm/tankterm.toml");

        let plan = plan(&alacritty.join("alacritty.yml"), &target, false).unwrap();

        let paths: Vec<_> = plan.files.iter().map(|file| file.path.clone()).collect();
        let tankterm = dir.path().join("tankterm");
        assert_eq!(paths, [
            tankterm.join("themes/dark.toml"),
            tankterm.join("imports/shared.toml"),
            target.clone(),
        ]);

        let entries = import_entries(&plan.files[2].contents);
        assert_eq!(entries[0], "themes/dark.toml");
        assert!(
            entries[1].ends_with("does-not-exist.toml") && Path::new(&entries[1]).is_absolute()
        );
        assert_eq!(entries[2], "imports/shared.toml");

        assert!(plan.files[0].contents.contains("[colors.primary]"));
        assert!(!plan.files[1].contents.contains("prefer_egl"));
        assert!(
            plan.report
                .skipped
                .iter()
                .any(|(key, _)| key.ends_with("imports/shared.toml: debug.prefer_egl"))
        );
    }

    #[test]
    fn skip_imports_keeps_original_paths() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("alacritty.toml");
        fs::write(&source, "[general]\nimport = [\"theme.toml\"]\n").unwrap();
        fs::write(dir.path().join("theme.toml"), "").unwrap();

        let plan = plan(&source, &dir.path().join("tankterm/tankterm.toml"), true).unwrap();

        assert_eq!(plan.files.len(), 1);
        let entries = import_entries(&plan.files[0].contents);
        assert_eq!(entries, [dir.path().join("theme.toml").to_string_lossy().into_owned()]);
    }

    #[test]
    fn import_cycles_end() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("alacritty.toml"), "[general]\nimport = [\"a.toml\"]\n").unwrap();
        fs::write(dir.path().join("a.toml"), "[general]\nimport = [\"alacritty.toml\"]\n").unwrap();

        let plan =
            plan(&dir.path().join("alacritty.toml"), &dir.path().join("t/tankterm.toml"), false)
                .unwrap();

        assert_eq!(plan.files.len(), 2);
        assert_eq!(import_entries(&plan.files[0].contents), ["tankterm.toml"]);
    }
}
