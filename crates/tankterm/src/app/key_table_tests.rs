//! Key tables, choosing panes by their labels, rotating panes, the tab navigator and renaming
//! tabs, through the real `App`.

use std::time::{Duration, Instant};

use iced::Size;
use iced::keyboard::key::Named;
use iced::widget::pane_grid::Axis;
use iced_test::simulator::Simulator;

use super::tab_tests::{app_with, focused, has_text, perform};
use super::*;
use crate::config::UiConfig;
use crate::session::ordered_panes;

/// A tab split into three panes side by side, terminals 0, 1 and 2, with the last one focused.
fn three_panes() -> App {
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Vertical)]);
    assert_eq!(terminals(&app), [0, 1, 2]);

    app
}

/// Terminal IDs of the panes of the shown tab, in layout order.
fn terminals(app: &App) -> Vec<u64> {
    let tab = &app.tabs[app.active];
    ordered_panes(tab.panes.layout()).iter().map(|pane| *tab.panes.get(*pane).unwrap()).collect()
}

/// The actions the keys typed into the application's view ask for.
fn typed(app: &App, keys: &[iced::keyboard::Key]) -> Vec<AppAction> {
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    for key in keys {
        ui.tap_key(key.clone());
    }

    let actions = ui.into_messages().filter_map(|message| match message {
        Message::Action(action) => Some(action),
        _ => None,
    });
    actions.collect()
}

fn character(key: &str) -> iced::keyboard::Key {
    iced::keyboard::Key::Character(key.into())
}

#[test]
fn panes_are_focused_by_their_labels() {
    let mut app = three_panes();
    perform(&mut app, &[AppAction::ToggleZoom, AppAction::PaneSelect(PaneSelectMode::Focus)]);

    // Every pane shows its label, in layout order, and none is zoomed.
    assert!(app.tabs[0].panes.maximized().is_none());
    assert!(has_text(&app, "A") && has_text(&app, "S") && has_text(&app, "D"));

    // The labels take the keyboard; nothing else typed meanwhile reaches a terminal.
    let actions = typed(&app, &[character("1"), character("s")]);
    assert_eq!(actions, [AppAction::PaneSelectChoose(1)]);

    perform(&mut app, &actions);
    assert!(app.pane_select.is_none());
    assert_eq!(focused(&app), 1);
    assert!(!has_text(&app, "S"));
}

#[test]
fn panes_are_swapped_by_their_labels() {
    let mut app = three_panes();
    perform(&mut app, &[
        AppAction::PaneSelect(PaneSelectMode::Swap),
        AppAction::PaneSelectChoose(0),
    ]);

    // The focused terminal moved to the first place and kept the focus.
    assert_eq!(terminals(&app), [2, 1, 0]);
    assert_eq!(focused(&app), 2);
}

#[test]
fn choosing_a_pane_can_be_cancelled() {
    let mut app = three_panes();
    perform(&mut app, &[AppAction::PaneSelect(PaneSelectMode::Focus)]);
    assert_eq!(typed(&app, &[Named::Escape.into()]), [AppAction::PaneSelectCancel]);

    perform(&mut app, &[AppAction::PaneSelectCancel, AppAction::PaneSelectChoose(0)]);
    assert_eq!((app.pane_select.is_none(), focused(&app)), (true, 2));

    // A letter that labels no pane leaves the labels shown.
    perform(&mut app, &[
        AppAction::PaneSelect(PaneSelectMode::Focus),
        AppAction::PaneSelectChoose(3),
    ]);
    assert!(app.pane_select.is_some());

    // Showing another tab, as a click on the tab bar does, ends the choice.
    perform(&mut app, &[AppAction::NewTab]);
    assert!(app.pane_select.is_none());
    assert_eq!(typed(&app, &[character("a")]), []);

    // A tab with one pane has nothing to choose from.
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::PaneSelect(PaneSelectMode::Focus)]);
    assert!(app.pane_select.is_none());
}

#[test]
fn panes_rotate_with_the_focus() {
    let mut app = three_panes();

    perform(&mut app, &[AppAction::RotatePanes(true)]);
    assert_eq!(terminals(&app), [2, 0, 1]);
    assert_eq!(focused(&app), 2);

    perform(&mut app, &[AppAction::RotatePanes(false), AppAction::RotatePanes(false)]);
    assert_eq!(terminals(&app), [1, 2, 0]);
    assert_eq!(focused(&app), 2);
}

#[test]
fn tab_navigator_shows_a_tab() {
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab, AppAction::NewTab, AppAction::RenameTab]);
    let _ = app.update(Message::PaletteQuery("logs".into()));
    let _ = app.update(Message::PaletteSubmit);

    perform(&mut app, &[AppAction::ShowTabNavigator]);
    assert!(has_text(&app, "1: test") && has_text(&app, "3: logs"));
    // The shown tab is selected to begin with.
    let _ = app.update(Message::PaletteSubmit);
    assert_eq!(app.active, 2);

    perform(&mut app, &[AppAction::ShowTabNavigator]);
    let _ = app.update(Message::PaletteQuery("1".into()));
    let _ = app.update(Message::PaletteSubmit);
    assert_eq!((app.active, app.palette.is_none()), (0, true));
}

#[test]
fn tabs_are_renamed_and_remembered() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("session.json");

    let mut app = super::tab_tests::app_from("", Hotkeys::default(), Some(file.clone()), |_| ());
    perform(&mut app, &[AppAction::RenameTab]);
    let _ = app.update(Message::PaletteQuery("  build ".into()));
    let _ = app.update(Message::PaletteSubmit);
    assert_eq!(app.tabs[0].title.as_deref(), Some("build"));
    assert_eq!(app.title(), "build");

    // The prompt starts with the title, and an empty one gives the pane's title back.
    perform(&mut app, &[AppAction::NewTab, AppAction::RenameTab]);
    let _ = app.update(Message::PaletteSubmit);
    assert_eq!(app.tabs[1].title, None);
    perform(&mut app, &[AppAction::SelectTab(0), AppAction::RenameTab]);
    assert!(has_text(&app, "build"));
    perform(&mut app, &[AppAction::PaletteClose]);
    drop(app);

    let app = super::tab_tests::app_from("", Hotkeys::default(), Some(file), |_| ());
    assert_eq!(app.tabs[0].title.as_deref(), Some("build"));
    assert_eq!(app.tabs[1].title, None);
}

/// Hotkeys with a `resize` table that stays until it is left and a one shot `once` table that
/// times out.
fn table_hotkeys() -> Hotkeys {
    let config: UiConfig = toml::from_str(
        r#"
        [keyboard.tables.resize]
        one_shot = false
        bindings = [
            { key = "ArrowLeft", action = "ResizePaneLeft" },
            { key = "Escape", action = "PopKeyTable" },
        ]

        [keyboard.tables.once]
        timeout = 60000
        bindings = [{ key = "N", action = "SelectNextTab" }]
        "#,
    )
    .unwrap();

    Hotkeys::from_config(&config)
}

#[test]
fn key_tables_are_activated_and_left() {
    let mut app = app_with("", table_hotkeys());
    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);

    perform(&mut app, &[AppAction::ActivateKeyTable("resize".into())]);
    // The tab bar shows the table.
    assert!(has_text(&app, "resize"));
    let actions = typed(&app, &[Named::ArrowLeft.into(), Named::ArrowLeft.into()]);
    let left = AppAction::ResizePane(iced::widget::pane_grid::Direction::Left);
    assert_eq!(actions, [left.clone(), left]);

    perform(&mut app, &[AppAction::ActivateKeyTable("once".into())]);
    assert_eq!(typed(&app, &[character("n")]), [AppAction::PopKeyTable, AppAction::NextTab]);
    assert!(app.key_tables[1].deadline.is_some());

    perform(&mut app, &[AppAction::PopKeyTable]);
    assert_eq!(typed(&app, &[Named::Escape.into()]), [AppAction::PopKeyTable]);
    perform(&mut app, &[AppAction::PopKeyTable]);
    assert!(app.key_tables.is_empty() && !has_text(&app, "resize"));

    // Tables that don't exist aren't activated.
    perform(&mut app, &[AppAction::ActivateKeyTable("missing".into())]);
    assert!(app.key_tables.is_empty());

    perform(&mut app, &[
        AppAction::ActivateKeyTable("resize".into()),
        AppAction::ActivateKeyTable("once".into()),
        AppAction::ClearKeyTables,
    ]);
    assert!(app.key_tables.is_empty());
}

#[test]
fn key_tables_time_out() {
    let mut app = app_with("", table_hotkeys());
    perform(&mut app, &[
        AppAction::ActivateKeyTable("resize".into()),
        AppAction::ActivateKeyTable("once".into()),
    ]);

    app.key_tables[1].deadline = Some(Instant::now() - Duration::from_millis(1));
    let _ = app.update(Message::Tick);
    assert_eq!(app.key_tables.len(), 1);
    assert_eq!(&*app.key_tables[0].name, "resize");
}
