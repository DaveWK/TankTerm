//! Remembering the tabs and split panes of the last window.
//!
//! Only the layout is stored: how many tabs there were, how each was split and which pane had
//! focus. The terminals are started fresh when the layout is restored.

use std::path::{Path, PathBuf};
use std::{fs, io};

use iced::widget::pane_grid::{Axis, Configuration, Node, Pane};
use serde::{Deserialize, Serialize};

/// Most panes restored from a session file, so a damaged file can't start endless shells.
const MAX_PANES: usize = 64;

/// The tabs of a window.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Session {
    /// Position of the tab that was shown.
    pub active: usize,
    pub tabs: Vec<SavedTab>,
}

/// The split layout of a tab.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SavedTab {
    pub layout: SavedNode,
    /// Position of the focused pane, counting panes from left to right and top to bottom.
    pub focus: usize,
    /// Title given to the tab, if it was given one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A pane, or a split holding two more nodes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SavedNode {
    Pane {
        /// Directory the pane's shell was in, where that is known.
        #[serde(default)]
        working_directory: Option<PathBuf>,
    },
    Split {
        /// Whether the divider is vertical, with the nodes side by side.
        side_by_side: bool,
        /// Share of the split taken by the first node.
        ratio: f32,
        a: Box<SavedNode>,
        b: Box<SavedNode>,
    },
}

impl SavedNode {
    /// Record a pane layout, asking `working_directory` for the directory of every pane.
    pub fn from_layout(node: &Node, working_directory: &impl Fn(Pane) -> Option<PathBuf>) -> Self {
        match node {
            Node::Pane(pane) => Self::Pane { working_directory: working_directory(*pane) },
            Node::Split { axis, ratio, a, b, .. } => Self::Split {
                side_by_side: *axis == Axis::Vertical,
                ratio: *ratio,
                a: Box::new(Self::from_layout(a, working_directory)),
                b: Box::new(Self::from_layout(b, working_directory)),
            },
        }
    }

    /// Number of panes in this layout.
    pub fn panes(&self) -> usize {
        match self {
            Self::Pane { .. } => 1,
            Self::Split { a, b, .. } => a.panes() + b.panes(),
        }
    }

    /// Build the pane layout, creating the content of every pane with `pane`, which is given
    /// the pane's working directory.
    ///
    /// This stops with the first error of `pane`.
    pub fn configuration<T, E>(
        &self,
        pane: &mut impl FnMut(Option<&Path>) -> Result<T, E>,
    ) -> Result<Configuration<T>, E> {
        Ok(match self {
            Self::Pane { working_directory } => {
                Configuration::Pane(pane(working_directory.as_deref())?)
            },
            Self::Split { side_by_side, ratio, a, b } => Configuration::Split {
                axis: if *side_by_side { Axis::Vertical } else { Axis::Horizontal },
                // Keep both sides visible, whatever the file says.
                ratio: if ratio.is_finite() { ratio.clamp(0.1, 0.9) } else { 0.5 },
                a: Box::new(a.configuration(pane)?),
                b: Box::new(b.configuration(pane)?),
            },
        })
    }
}

impl Session {
    /// Read a session file.
    ///
    /// Returns `None` if there is no usable session: no file, a damaged file, no tabs or an
    /// unreasonable number of panes.
    pub fn load(path: &Path) -> Option<Self> {
        let session: Self = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;

        let panes: usize = session.tabs.iter().map(|tab| tab.layout.panes()).sum();
        (1..=MAX_PANES).contains(&panes).then_some(session)
    }

    /// Write the session file, replacing the previous one in a single step.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }

        let temporary = path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_string(self)?)?;
        fs::rename(temporary, path)
    }
}

/// The panes of a layout from left to right and top to bottom.
pub fn ordered_panes(node: &Node) -> Vec<Pane> {
    match node {
        Node::Pane(pane) => vec![*pane],
        Node::Split { a, b, .. } => {
            let mut panes = ordered_panes(a);
            panes.extend(ordered_panes(b));
            panes
        },
    }
}

/// Where the session is stored: `$XDG_STATE_HOME/tankterm/session.json`.
#[cfg(not(windows))]
pub fn default_path() -> Option<PathBuf> {
    xdg::BaseDirectories::with_prefix("tankterm").get_state_file("session.json")
}

/// Where the session is stored: `%LOCALAPPDATA%\tankterm\session.json`.
#[cfg(windows)]
pub fn default_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|path| path.join("tankterm").join("session.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane() -> SavedNode {
        SavedNode::Pane { working_directory: None }
    }

    fn split(side_by_side: bool, ratio: f32, a: SavedNode, b: SavedNode) -> SavedNode {
        SavedNode::Split { side_by_side, ratio, a: Box::new(a), b: Box::new(b) }
    }

    #[test]
    fn sessions_survive_saving_and_loading() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state").join("session.json");

        let layout = split(true, 0.3, pane(), split(false, 0.5, pane(), pane()));
        let session = Session {
            active: 1,
            tabs: vec![
                SavedTab { layout: pane(), focus: 0, title: Some("build".into()) },
                SavedTab { layout, focus: 2, title: None },
            ],
        };

        assert_eq!(Session::load(&path), None);
        session.save(&path).unwrap();
        assert_eq!(Session::load(&path), Some(session));
    }

    #[test]
    fn damaged_sessions_are_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");

        fs::write(&path, "{ not json").unwrap();
        assert_eq!(Session::load(&path), None);

        Session { active: 0, tabs: Vec::new() }.save(&path).unwrap();
        assert_eq!(Session::load(&path), None);

        // A layout with more panes than anyone would open.
        let mut layout = pane();
        for _ in 0..MAX_PANES {
            layout = split(true, 0.5, layout, pane());
        }
        let tabs = vec![SavedTab { layout, focus: 0, title: None }];
        Session { active: 0, tabs }.save(&path).unwrap();
        assert_eq!(Session::load(&path), None);
    }

    #[test]
    fn layouts_are_rebuilt_with_sane_ratios() {
        let layout = split(false, f32::NAN, pane(), split(true, 7., pane(), pane()));
        let mut next = 0;
        let configuration = layout
            .configuration(&mut |_| {
                next += 1;
                Ok::<_, ()>(next)
            })
            .unwrap();

        let Configuration::Split { axis, ratio, a, b } = configuration else {
            panic!("not a split")
        };
        assert_eq!((axis, ratio), (Axis::Horizontal, 0.5));
        assert!(matches!(*a, Configuration::Pane(1)));
        let Configuration::Split { axis, ratio, a, b } = *b else { panic!("not a split") };
        assert_eq!((axis, ratio), (Axis::Vertical, 0.9));
        assert!(matches!((*a, *b), (Configuration::Pane(2), Configuration::Pane(3))));
    }

    #[test]
    fn panes_are_rebuilt_in_their_working_directories() {
        let home = SavedNode::Pane { working_directory: Some("/home".into()) };
        let layout = split(true, 0.5, home, pane());

        let mut directories = Vec::new();
        let _ = layout.configuration(&mut |directory| {
            directories.push(directory.map(Path::to_owned));
            Ok::<_, ()>(())
        });
        assert_eq!(directories, [Some("/home".into()), None]);

        // Sessions written before directories were remembered still load.
        let old: SavedNode = serde_json::from_str(r#"{"Pane":{}}"#).unwrap();
        assert_eq!(old, pane());
    }

    #[test]
    fn building_a_layout_stops_at_the_first_error() {
        let layout = split(true, 0.5, pane(), pane());
        let mut calls = 0;
        let result = layout.configuration(&mut |_| {
            calls += 1;
            Err::<(), _>("no shell")
        });

        assert_eq!((result.err(), calls), (Some("no shell"), 1));
    }
}
