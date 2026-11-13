//! Key bindings handled by the application itself: tabs and panes, copying and pasting, the font
//! size, scrolling, the window, and keys that type text or run a program.
//!
//! The terminal widget sends every key it receives to the shell. [`KeyFilter`] sits above it in
//! the widget tree and takes the keys bound to these actions out of the event stream, so they
//! never reach the shell.

use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, Modifiers};
use iced::widget::pane_grid::{Axis, Direction};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use iced_core::widget::{Operation, Tree, tree};
use iced_core::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};

use iced_tterm::TermMode;
use iced_tterm::bindings::{Binding as WidgetBinding, BindingAction, InputKind};

use crate::config::ui_config::Program;
use crate::config::{
    Action, BindingKey, BindingMode, KeyBinding, KeyLocation, LeaderKey, MouseAction, MouseBinding,
    MouseEvent, UiConfig,
};

/// Where a scrolling action moves the view of the focused terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTo {
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    LineUp,
    LineDown,
    Top,
    Bottom,
}

/// Actions of the application: for tabs and panes, the focused terminal and the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    /// Select a tab by its position, starting at zero.
    SelectTab(usize),
    LastTab,
    MoveTabLeft,
    MoveTabRight,
    /// Split the focused pane; the new pane is placed after it on this axis.
    Split(Axis),
    ClosePane,
    FocusPane(Direction),
    NextPane,
    PreviousPane,
    ToggleZoom,
    /// Move a divider next to the focused pane in this direction.
    ResizePane(Direction),
    /// Show the command palette.
    ShowCommands,
    /// Show the profile selector.
    ShowProfiles,
    /// Select the next item of the command palette or profile selector.
    PaletteNext,
    /// Select the previous item of the command palette or profile selector.
    PalettePrevious,
    /// Close the command palette or profile selector without choosing anything.
    PaletteClose,
    /// Paste the text the paste warning asks about.
    PasteConfirm,
    /// Close the paste warning without pasting.
    PasteCancel,
    /// Copy the text selected in the focused terminal to the clipboard.
    Copy,
    /// Paste the clipboard into the focused terminal.
    Paste,
    /// Paste the selection clipboard into the focused terminal.
    PasteSelection,
    IncreaseFontSize,
    DecreaseFontSize,
    /// Go back to the font size of the configuration.
    ResetFontSize,
    /// Scroll the focused terminal through its scrollback buffer.
    Scroll(ScrollTo),
    /// Drop the scrollback buffer of the focused terminal.
    ClearHistory,
    ToggleFullscreen,
    ToggleMaximized,
    /// Put the text selected in the focused terminal on the selection clipboard.
    CopySelection,
    /// Drop the selection of the focused terminal.
    ClearSelection,
    /// Write text to the focused terminal, as if it was typed (`chars`).
    Chars(Arc<str>),
    /// Start a program (`command`), in the directory of the focused terminal's shell where that
    /// is known.
    RunCommand(Program),
    Minimize,
    /// Hide the window.
    Hide,
    /// Close the window, with all its tabs.
    Quit,
}

impl AppAction {
    /// The application action for a config binding action, if it is one.
    fn from_config(action: &Action) -> Option<Self> {
        Some(match action {
            Action::CreateNewTab => Self::NewTab,
            Action::CloseTab => Self::CloseTab,
            Action::SelectNextTab => Self::NextTab,
            Action::SelectPreviousTab => Self::PreviousTab,
            Action::SelectTab1 => Self::SelectTab(0),
            Action::SelectTab2 => Self::SelectTab(1),
            Action::SelectTab3 => Self::SelectTab(2),
            Action::SelectTab4 => Self::SelectTab(3),
            Action::SelectTab5 => Self::SelectTab(4),
            Action::SelectTab6 => Self::SelectTab(5),
            Action::SelectTab7 => Self::SelectTab(6),
            Action::SelectTab8 => Self::SelectTab(7),
            Action::SelectTab9 => Self::SelectTab(8),
            Action::SelectLastTab => Self::LastTab,
            Action::MoveTabLeft => Self::MoveTabLeft,
            Action::MoveTabRight => Self::MoveTabRight,
            // iced names the axis after the divider: a vertical divider puts panes side by side.
            Action::SplitRight => Self::Split(Axis::Vertical),
            Action::SplitDown => Self::Split(Axis::Horizontal),
            Action::ClosePane => Self::ClosePane,
            Action::FocusPaneLeft => Self::FocusPane(Direction::Left),
            Action::FocusPaneRight => Self::FocusPane(Direction::Right),
            Action::FocusPaneUp => Self::FocusPane(Direction::Up),
            Action::FocusPaneDown => Self::FocusPane(Direction::Down),
            Action::FocusNextPane => Self::NextPane,
            Action::FocusPreviousPane => Self::PreviousPane,
            Action::TogglePaneZoom => Self::ToggleZoom,
            Action::ResizePaneLeft => Self::ResizePane(Direction::Left),
            Action::ResizePaneRight => Self::ResizePane(Direction::Right),
            Action::ResizePaneUp => Self::ResizePane(Direction::Up),
            Action::ResizePaneDown => Self::ResizePane(Direction::Down),
            Action::ShowCommandPalette => Self::ShowCommands,
            Action::ShowProfiles => Self::ShowProfiles,
            Action::Copy => Self::Copy,
            Action::Paste => Self::Paste,
            Action::PasteSelection => Self::PasteSelection,
            Action::IncreaseFontSize => Self::IncreaseFontSize,
            Action::DecreaseFontSize => Self::DecreaseFontSize,
            Action::ResetFontSize => Self::ResetFontSize,
            Action::ScrollPageUp => Self::Scroll(ScrollTo::PageUp),
            Action::ScrollPageDown => Self::Scroll(ScrollTo::PageDown),
            Action::ScrollHalfPageUp => Self::Scroll(ScrollTo::HalfPageUp),
            Action::ScrollHalfPageDown => Self::Scroll(ScrollTo::HalfPageDown),
            Action::ScrollLineUp => Self::Scroll(ScrollTo::LineUp),
            Action::ScrollLineDown => Self::Scroll(ScrollTo::LineDown),
            Action::ScrollToTop => Self::Scroll(ScrollTo::Top),
            Action::ScrollToBottom => Self::Scroll(ScrollTo::Bottom),
            Action::ClearHistory => Self::ClearHistory,
            Action::ToggleFullscreen => Self::ToggleFullscreen,
            Action::ToggleMaximized => Self::ToggleMaximized,
            Action::CopySelection => Self::CopySelection,
            Action::ClearSelection => Self::ClearSelection,
            Action::Esc(chars) => Self::Chars(chars.as_str().into()),
            Action::Command(program) => Self::RunCommand(program.clone()),
            Action::Minimize => Self::Minimize,
            Action::Hide => Self::Hide,
            Action::Quit => Self::Quit,
            _ => return None,
        })
    }
}

/// What a bound key does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    Run(AppAction),
    /// `ReceiveChar`: the terminal gets the key, whatever else is bound to it.
    Pass,
    /// `None`: the key does nothing and doesn't reach the terminal.
    Swallow,
}

impl Effect {
    /// What a config binding action does, if the application knows the action.
    fn from_config(action: &Action) -> Option<Self> {
        match action {
            Action::ReceiveChar => Some(Self::Pass),
            Action::None => Some(Self::Swallow),
            action => AppAction::from_config(action).map(Self::Run),
        }
    }
}

/// Binding modes that are never active: the iced front end has no vi mode and no search yet.
const MISSING_MODES: BindingMode = BindingMode::VI.union(BindingMode::SEARCH);

/// A key with its modifiers, the terminal modes it is limited to, and what it does.
#[derive(Debug, Clone, PartialEq)]
struct Bound {
    key: Key,
    modifiers: Modifiers,
    location: KeyLocation,
    /// Terminal modes that have to be active.
    mode: BindingMode,
    /// Terminal modes that must not be active.
    notmode: BindingMode,
    effect: Effect,
}

impl Bound {
    /// A key that runs an action in every terminal mode.
    fn new(key: &Key, modifiers: Modifiers, action: AppAction) -> Self {
        Self {
            key: normalize(key),
            modifiers,
            location: KeyLocation::Any,
            mode: BindingMode::empty(),
            notmode: BindingMode::empty(),
            effect: Effect::Run(action),
        }
    }

    /// Whether a key press in a terminal with `mode` is this binding's.
    fn is_triggered_by(&self, press: &KeyPress<'_>, mode: BindingMode) -> bool {
        // An event doesn't tell a left or right key from one that could be anywhere.
        let location = match press.location {
            keyboard::Location::Standard => KeyLocation::Standard,
            keyboard::Location::Numpad => KeyLocation::Numpad,
            keyboard::Location::Left | keyboard::Location::Right => KeyLocation::Any,
        };

        self.key == normalize(press.key)
            && self.modifiers == press.modifiers
            && self.location == location
            && mode.contains(self.mode)
            && !mode.intersects(self.notmode)
    }
}

/// A key press, as bindings are matched against it.
#[derive(Debug, Clone, Copy)]
pub struct KeyPress<'a> {
    /// The key without its modifiers applied.
    pub key: &'a Key,
    pub modifiers: Modifiers,
    pub location: keyboard::Location,
}

/// What a key press does, going by the bindings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Triggered {
    /// The actions to run, in the order of their bindings.
    pub actions: Vec<AppAction>,
    /// Whether the key is kept from the terminal.
    pub captured: bool,
}

/// What a key press does, going by a list of bindings.
///
/// Every binding of the key runs. The key is kept from the terminal unless it has no binding
/// or one of its bindings is `ReceiveChar`.
fn triggered(bindings: &[Bound], press: &KeyPress<'_>, mode: BindingMode) -> Triggered {
    let mut triggered = Triggered::default();
    let mut pass = false;

    for bound in bindings.iter().filter(|bound| bound.is_triggered_by(press, mode)) {
        triggered.captured = true;
        match &bound.effect {
            Effect::Run(action) => triggered.actions.push(action.clone()),
            Effect::Pass => pass = true,
            Effect::Swallow => (),
        }
    }
    triggered.captured &= !pass;

    triggered
}

/// Keys bound to application actions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hotkeys {
    /// Keys that run their action when they are pressed.
    direct: Vec<Bound>,
    /// Keys that run their action when they are pressed after the leader key.
    sequences: Vec<Bound>,
    leader: Option<Leader>,
}

/// The key that starts a two key sequence, like tmux's prefix key.
#[derive(Debug, Clone, PartialEq)]
pub struct Leader {
    pub key: Key,
    pub modifiers: Modifiers,
    /// How long the leader waits for the second key.
    pub timeout: Duration,
}

impl Hotkeys {
    /// Collect the bindings of the configuration the application handles: the default ones,
    /// those of the selected preset and the user's.
    pub fn from_config(config: &UiConfig) -> Self {
        Self::from_bindings(&config.app_bindings(), Leader::from_config(config.leader()))
    }

    /// Collect the bindings the application handles from key bindings.
    ///
    /// Bindings with the `Leader` mode are reached through `leader`. Bindings that need vi mode
    /// or search are never active, since neither exists yet. Bindings to actions the
    /// application doesn't know and bindings to scancodes aren't supported; their keys are left
    /// to the terminal.
    pub fn from_bindings(bindings: &[KeyBinding], leader: Option<Leader>) -> Self {
        let mut hotkeys = Self { leader, ..Self::default() };

        for binding in bindings {
            let Some(effect) = Effect::from_config(&binding.action) else { continue };
            let BindingKey::Keycode { key, location } = &binding.trigger else { continue };
            if binding.mode.intersects(MISSING_MODES) {
                continue;
            }

            let after_leader = binding.mode.contains(BindingMode::LEADER);
            let bound = Bound {
                key: normalize(&iced_winit::conversion::key(key.clone())),
                modifiers: iced_winit::conversion::modifiers(binding.mods),
                location: *location,
                mode: binding.mode - BindingMode::LEADER,
                notmode: binding.notmode - MISSING_MODES - BindingMode::LEADER,
                effect,
            };

            if !after_leader {
                hotkeys.direct.push(bound);
            } else if matches!(bound.effect, Effect::Run(_))
                && !binding.notmode.contains(BindingMode::LEADER)
            {
                // Any other key after the leader key ends the sequence without reaching the
                // terminal, so unbinding one needs no entry.
                hotkeys.sequences.push(bound);
            }
        }

        hotkeys
    }

    /// Use this leader key.
    #[cfg(test)]
    pub fn with_leader(mut self, key: Key, modifiers: Modifiers, timeout: Duration) -> Self {
        self.leader = Some(Leader { key: normalize(&key), modifiers, timeout });
        self
    }

    /// Bind a key pressed after the leader key.
    #[cfg(test)]
    pub fn with_sequence(mut self, key: Key, modifiers: Modifiers, action: AppAction) -> Self {
        self.sequences.push(Bound::new(&key, modifiers, action));
        self
    }

    /// The keys that work while the command palette or profile selector is shown.
    pub fn palette() -> Self {
        use keyboard::key::Named;

        Self::default()
            .with(Key::Named(Named::Escape), Modifiers::empty(), AppAction::PaletteClose)
            .with(Key::Named(Named::ArrowDown), Modifiers::empty(), AppAction::PaletteNext)
            .with(Key::Named(Named::ArrowUp), Modifiers::empty(), AppAction::PalettePrevious)
    }

    /// The keys that answer the paste warning.
    pub fn paste_warning() -> Self {
        use keyboard::key::Named;

        Self::default()
            .with(Key::Named(Named::Enter), Modifiers::empty(), AppAction::PasteConfirm)
            .with(Key::Named(Named::Escape), Modifiers::empty(), AppAction::PasteCancel)
    }

    /// Bind a key directly.
    pub fn with(mut self, key: Key, modifiers: Modifiers, action: AppAction) -> Self {
        self.direct.push(Bound::new(&key, modifiers, action));
        self
    }

    /// What a key press does in a terminal with `mode`.
    pub fn press(&self, press: &KeyPress<'_>, mode: BindingMode) -> Triggered {
        triggered(&self.direct, press, mode)
    }

    /// What a key press that follows the leader key does in a terminal with `mode`.
    ///
    /// The key never reaches the terminal, whether it is bound or not.
    pub fn press_after_leader(&self, press: &KeyPress<'_>, mode: BindingMode) -> Triggered {
        Triggered { captured: true, ..triggered(&self.sequences, press, mode) }
    }

    /// The first action bound to a key press, in a terminal without any modes.
    #[cfg(test)]
    pub fn action(&self, key: &Key, modifiers: Modifiers) -> Option<AppAction> {
        let location = keyboard::Location::Standard;
        let press = KeyPress { key, modifiers, location };
        self.press(&press, BindingMode::empty()).actions.first().cloned()
    }

    /// The first action bound to a key press that follows the leader key, in a terminal without
    /// any modes.
    #[cfg(test)]
    pub fn sequence(&self, key: &Key, modifiers: Modifiers) -> Option<AppAction> {
        let location = keyboard::Location::Standard;
        let press = KeyPress { key, modifiers, location };
        self.press_after_leader(&press, BindingMode::empty()).actions.first().cloned()
    }

    /// The leader key, if any binding needs it.
    ///
    /// Without such bindings the key isn't special and is left to the shell.
    pub fn leader(&self) -> Option<&Leader> {
        self.leader.as_ref().filter(|_| !self.sequences.is_empty())
    }
}

impl Leader {
    /// The configured leader key. Leader keys given as scancodes aren't supported.
    pub fn from_config(leader: &LeaderKey) -> Option<Self> {
        let BindingKey::Keycode { key, .. } = &leader.key else { return None };

        Some(Self {
            key: normalize(&iced_winit::conversion::key(key.clone())),
            modifiers: iced_winit::conversion::modifiers(leader.mods),
            timeout: Duration::from_millis(leader.timeout),
        })
    }

    fn is(&self, key: &Key, modifiers: Modifiers) -> bool {
        self.key == normalize(key) && self.modifiers == modifiers
    }
}

/// A key with its modifiers, written like `Ctrl+Shift+T`.
fn describe_key(key: &Key, modifiers: Modifiers) -> Option<String> {
    let names = [
        (Modifiers::CTRL, "Ctrl"),
        (Modifiers::ALT, "Alt"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::LOGO, if cfg!(target_os = "macos") { "Cmd" } else { "Super" }),
    ];
    let mut parts: Vec<String> = names
        .iter()
        .filter(|(modifier, _)| modifiers.contains(*modifier))
        .map(|(_, name)| (*name).to_owned())
        .collect();

    parts.push(match key {
        Key::Character(text) => text.to_uppercase(),
        Key::Named(named) => format!("{named:?}"),
        Key::Unidentified => return None,
    });

    Some(parts.join("+"))
}

impl Hotkeys {
    /// The first key bound to an action, written like `Ctrl+Shift+T`, or like `Ctrl+A C` for a
    /// key that follows the leader key.
    pub fn describe(&self, action: &AppAction) -> Option<String> {
        let runs = |bound: &&Bound| matches!(&bound.effect, Effect::Run(run) if run == action);
        if let Some(bound) = self.direct.iter().find(runs) {
            return describe_key(&bound.key, bound.modifiers);
        }

        let leader = self.leader()?;
        let bound = self.sequences.iter().find(runs)?;
        let leader = describe_key(&leader.key, leader.modifiers)?;

        Some(format!("{leader} {}", describe_key(&bound.key, bound.modifiers)?))
    }
}

/// Widget bindings for the config's mouse bindings to the middle and right buttons.
///
/// `Paste`, `PasteSelection` and `ExpandSelection` are the actions the terminal widget knows for
/// these buttons. A button bound to anything else does nothing, which takes the middle button's
/// paste away if the config rebinds it. Bindings limited to a terminal mode aren't supported.
pub fn mouse_bindings(bindings: &[MouseBinding]) -> Vec<(WidgetBinding<InputKind>, BindingAction)> {
    use winit::event::MouseButton;

    let bindings = bindings.iter().filter_map(|binding| {
        let button = match binding.trigger {
            MouseEvent::Button(MouseButton::Middle) => mouse::Button::Middle,
            MouseEvent::Button(MouseButton::Right) => mouse::Button::Right,
            _ => return None,
        };
        if !binding.mode.is_empty() {
            return None;
        }

        let action = match binding.action {
            Action::Paste => BindingAction::Paste,
            Action::PasteSelection => BindingAction::PasteSelection,
            Action::Mouse(MouseAction::ExpandSelection) => BindingAction::ExpandSelection,
            _ => BindingAction::Ignore,
        };
        let trigger = WidgetBinding {
            target: InputKind::Mouse(button),
            modifiers: iced_winit::conversion::modifiers(binding.mods),
            terminal_mode_include: TermMode::empty(),
            terminal_mode_exclude: TermMode::empty(),
        };

        Some((trigger, action))
    });

    bindings.collect()
}

/// Make character keys compare equal regardless of case, since Shift changes the case.
fn normalize(key: &Key) -> Key {
    match key {
        Key::Character(text) => Key::Character(text.to_lowercase().into()),
        key => key.clone(),
    }
}

/// Wraps the window's content and handles the keys bound in [`Hotkeys`].
///
/// A bound key press is turned into a message and hidden from the content, as is its release.
pub struct KeyFilter<'a, Message> {
    content: Element<'a, Message>,
    hotkeys: &'a Hotkeys,
    on_action: fn(AppAction) -> Message,
    exclusive: bool,
    /// Modes of the terminal the keys are typed into, for bindings limited to some.
    mode: BindingMode,
}

impl<'a, Message> KeyFilter<'a, Message> {
    pub fn new(
        content: impl Into<Element<'a, Message>>,
        hotkeys: &'a Hotkeys,
        on_action: fn(AppAction) -> Message,
    ) -> Self {
        Self {
            content: content.into(),
            hotkeys,
            on_action,
            exclusive: false,
            mode: BindingMode::empty(),
        }
    }

    /// The modes of the terminal the keys are typed into.
    pub fn mode(mut self, mode: BindingMode) -> Self {
        self.mode = mode;
        self
    }

    /// Keep every key from the content, not only the bound ones.
    ///
    /// This is for dialogs that are answered with a key: whatever else is typed meanwhile
    /// shouldn't end up in a terminal.
    pub fn exclusive(mut self, exclusive: bool) -> Self {
        self.exclusive = exclusive;
        self
    }
}

/// What the filter remembers between key events.
#[derive(Default)]
struct State {
    /// Keys that were taken from the content and are still held down; their release is taken
    /// as well.
    held: Vec<Key>,
    /// When the leader key was pressed, while the key after it is awaited.
    leader: Option<Instant>,
}

impl State {
    fn hold(&mut self, key: &Key) {
        let key = normalize(key);
        if !self.held.contains(&key) {
            self.held.push(key);
        }
    }
}

/// Whether a key is a modifier by itself, like Shift.
fn is_modifier(key: &Key) -> bool {
    use keyboard::key::Named;

    matches!(
        key,
        Key::Named(
            Named::Shift
                | Named::Control
                | Named::Alt
                | Named::AltGraph
                | Named::Super
                | Named::Meta
        )
    )
}

impl<Message> Widget<Message, Theme, Renderer> for KeyFilter<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                location,
                repeat,
                ..
            }) => {
                let press = KeyPress { key, modifiers: *modifiers, location: *location };

                // The key after the leader key, unless it took too long.
                let leader = self.hotkeys.leader();
                let awaited = state
                    .leader
                    .take()
                    .zip(leader)
                    .filter(|(since, leader)| since.elapsed() <= leader.timeout);

                if let Some((since, leader)) = awaited {
                    if *repeat || is_modifier(key) {
                        // Holding the leader key down or reaching for a modifier isn't an answer.
                        state.leader = Some(since);
                    } else if leader.is(key, *modifiers) {
                        // The leader key twice is the leader key for the shell: pass it on.
                        self.content.as_widget_mut().update(
                            &mut tree.children[0],
                            event,
                            layout,
                            cursor,
                            renderer,
                            clipboard,
                            shell,
                            viewport,
                        );
                        return;
                    } else {
                        let triggered = self.hotkeys.press_after_leader(&press, self.mode);
                        for action in triggered.actions {
                            shell.publish((self.on_action)(action));
                        }
                    }

                    // An unbound key ends the sequence without reaching the shell either.
                    state.hold(key);
                    shell.capture_event();
                    return;
                }

                if leader.is_some_and(|leader| leader.is(key, *modifiers)) {
                    state.leader = Some(Instant::now());
                    state.hold(key);
                    shell.capture_event();
                    return;
                }

                let triggered = self.hotkeys.press(&press, self.mode);
                for action in triggered.actions {
                    shell.publish((self.on_action)(action));
                }
                if triggered.captured {
                    state.hold(key);
                    shell.capture_event();
                    return;
                }
            },
            Event::Keyboard(keyboard::Event::KeyReleased { key, .. }) => {
                let key = normalize(key);
                if let Some(index) = state.held.iter().position(|held| *held == key) {
                    state.held.swap_remove(index);
                    shell.capture_event();
                    return;
                }
            },
            _ => (),
        }

        if self.exclusive && matches!(event, Event::Keyboard(_)) {
            shell.capture_event();
            return;
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message: 'a> From<KeyFilter<'a, Message>> for Element<'a, Message> {
    fn from(filter: KeyFilter<'a, Message>) -> Self {
        Element::new(filter)
    }
}

#[cfg(test)]
mod tests {
    use winit::keyboard::ModifiersState;

    use super::*;
    use crate::config::{Binding, BindingMode, KeyLocation};

    fn binding(key: &str, mods: ModifiersState, action: Action) -> KeyBinding {
        Binding {
            trigger: BindingKey::Keycode {
                key: winit::keyboard::Key::Character(key.into()),
                location: KeyLocation::Standard,
            },
            mods,
            action,
            mode: BindingMode::empty(),
            notmode: BindingMode::empty(),
            help: None,
        }
    }

    #[test]
    fn tab_and_pane_bindings_become_hotkeys() {
        let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let hotkeys = Hotkeys::from_bindings(
            &[
                binding("t", mods, Action::CreateNewTab),
                binding("s", mods, Action::SplitRight),
                binding("c", mods, Action::Copy),
            ],
            None,
        );

        let pressed = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(hotkeys.action(&Key::Character("t".into()), pressed), Some(AppAction::NewTab));
        assert_eq!(
            hotkeys.action(&Key::Character("s".into()), pressed),
            Some(AppAction::Split(Axis::Vertical))
        );
        assert_eq!(hotkeys.action(&Key::Character("c".into()), pressed), Some(AppAction::Copy));
    }

    #[test]
    fn keys_of_unsupported_actions_are_left_to_the_terminal() {
        let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let hotkeys = Hotkeys::from_bindings(
            &[
                binding("f", mods, Action::SearchForward),
                binding("h", mods, Action::ToggleHelp),
                binding("n", mods, Action::SpawnNewInstance),
                binding("o", mods, Action::HideOtherApplications),
            ],
            None,
        );

        assert_eq!(hotkeys, Hotkeys::default());
        assert_eq!(pressed(&hotkeys, "f", CTRL_SHIFT, BindingMode::empty()), Triggered::default());
    }

    #[test]
    fn hotkeys_ignore_case_but_not_modifiers() {
        let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let hotkeys = Hotkeys::from_bindings(&[binding("T", mods, Action::CreateNewTab)], None);

        let pressed = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(hotkeys.action(&Key::Character("T".into()), pressed), Some(AppAction::NewTab));
        assert_eq!(hotkeys.action(&Key::Character("t".into()), pressed), Some(AppAction::NewTab));
        assert_eq!(hotkeys.action(&Key::Character("t".into()), Modifiers::CTRL), None);
    }

    #[test]
    fn mode_specific_bindings_are_skipped() {
        let mut vi_only = binding("t", ModifiersState::CONTROL, Action::CreateNewTab);
        vi_only.mode = BindingMode::VI;

        assert_eq!(Hotkeys::from_bindings(&[vi_only], None), Hotkeys::default());
    }
    /// The hotkeys of a configuration, with the macOS or the other platforms' preset keys.
    fn hotkeys(config: &str, macos: bool) -> Hotkeys {
        let config: UiConfig = toml::from_str(config).unwrap();
        Hotkeys::from_bindings(
            &config.tab_bindings_for(macos),
            Leader::from_config(config.leader()),
        )
    }

    fn character(key: &str) -> Key {
        Key::Character(key.into())
    }

    /// What pressing a character key at its standard location does in a terminal with `mode`.
    fn pressed(hotkeys: &Hotkeys, key: &str, modifiers: Modifiers, mode: BindingMode) -> Triggered {
        let location = keyboard::Location::Standard;
        hotkeys.press(&KeyPress { key: &character(key), modifiers, location }, mode)
    }

    /// What pressing a named key does in a terminal with `mode`.
    fn pressed_named(
        hotkeys: &Hotkeys,
        key: keyboard::key::Named,
        modifiers: Modifiers,
        mode: BindingMode,
    ) -> Triggered {
        let location = keyboard::Location::Standard;
        hotkeys.press(&KeyPress { key: &Key::Named(key), modifiers, location }, mode)
    }

    /// A key that runs these actions and is kept from the terminal.
    fn runs(actions: &[AppAction]) -> Triggered {
        Triggered { actions: actions.to_vec(), captured: true }
    }

    /// A key that does nothing and reaches the terminal.
    fn unbound() -> Triggered {
        Triggered::default()
    }

    fn arrow_left() -> Key {
        Key::Named(keyboard::key::Named::ArrowLeft)
    }

    const CTRL_SHIFT: Modifiers = Modifiers::CTRL.union(Modifiers::SHIFT);
    const CTRL_ALT: Modifiers = Modifiers::CTRL.union(Modifiers::ALT);
    const CTRL_SHIFT_ALT: Modifiers = CTRL_SHIFT.union(Modifiers::ALT);

    /// The keys shared by the Tabby and WezTerm presets.
    fn assert_shared_keys(hotkeys: &Hotkeys) {
        use keyboard::key::Named;

        let bound = [
            (character("t"), CTRL_SHIFT, AppAction::NewTab),
            (character("w"), CTRL_SHIFT, AppAction::CloseTab),
            (Key::Named(Named::Tab), Modifiers::CTRL, AppAction::NextTab),
            (Key::Named(Named::Tab), CTRL_SHIFT, AppAction::PreviousTab),
            (Key::Named(Named::PageUp), CTRL_SHIFT, AppAction::MoveTabLeft),
            (Key::Named(Named::PageDown), CTRL_SHIFT, AppAction::MoveTabRight),
            (character("s"), CTRL_SHIFT, AppAction::Split(Axis::Vertical)),
            (character("d"), CTRL_SHIFT, AppAction::Split(Axis::Horizontal)),
            (character("z"), CTRL_SHIFT, AppAction::ToggleZoom),
            (character("w"), CTRL_ALT, AppAction::ClosePane),
            (arrow_left(), CTRL_SHIFT_ALT, AppAction::ResizePane(Direction::Left)),
            (character("p"), CTRL_SHIFT, AppAction::ShowCommands),
            (character("e"), CTRL_SHIFT, AppAction::ShowProfiles),
        ];
        for (key, modifiers, action) in bound {
            assert_eq!(hotkeys.action(&key, modifiers), Some(action), "{key:?} {modifiers:?}");
        }

        // Without bindings that need it, the leader key belongs to the shell.
        assert_eq!(hotkeys.leader(), None);
    }

    #[test]
    fn tabby_preset_is_the_default() {
        let hotkeys = hotkeys("", false);
        assert_eq!(hotkeys, self::hotkeys("[keyboard]\npreset = \"Tabby\"\n", false));
        assert_shared_keys(&hotkeys);

        assert_eq!(hotkeys.action(&character("1"), Modifiers::ALT), Some(AppAction::SelectTab(0)));
        assert_eq!(hotkeys.action(&character("9"), Modifiers::ALT), Some(AppAction::SelectTab(8)));
        assert_eq!(
            hotkeys.action(&arrow_left(), CTRL_ALT),
            Some(AppAction::FocusPane(Direction::Left))
        );
        assert_eq!(hotkeys.describe(&AppAction::LastTab), None);

        // These reach the applications in the terminal.
        assert_eq!(hotkeys.action(&arrow_left(), CTRL_SHIFT), None);
        assert_eq!(hotkeys.action(&character("1"), CTRL_SHIFT), None);
    }

    #[test]
    fn wezterm_preset_keeps_everything_on_control_shift() {
        let hotkeys = hotkeys("[keyboard]\npreset = \"WezTerm\"\n", false);
        assert_shared_keys(&hotkeys);

        assert_eq!(hotkeys.action(&character("1"), CTRL_SHIFT), Some(AppAction::SelectTab(0)));
        assert_eq!(hotkeys.action(&character("8"), CTRL_SHIFT), Some(AppAction::SelectTab(7)));
        assert_eq!(hotkeys.action(&character("9"), CTRL_SHIFT), Some(AppAction::LastTab));
        assert_eq!(
            hotkeys.action(&arrow_left(), CTRL_SHIFT),
            Some(AppAction::FocusPane(Direction::Left))
        );

        // These reach the applications in the terminal.
        assert_eq!(hotkeys.action(&character("1"), Modifiers::ALT), None);
        assert_eq!(hotkeys.action(&arrow_left(), CTRL_ALT), None);
    }

    #[test]
    fn leader_preset_keeps_three_direct_keys() {
        use keyboard::key::Named;

        let hotkeys = hotkeys("[keyboard]\npreset = \"Leader\"\n", false);

        assert_eq!(hotkeys.action(&character("t"), CTRL_SHIFT), Some(AppAction::NewTab));
        assert_eq!(
            hotkeys.action(&character("s"), CTRL_SHIFT),
            Some(AppAction::Split(Axis::Vertical))
        );
        assert_eq!(
            hotkeys.action(&character("d"), CTRL_SHIFT),
            Some(AppAction::Split(Axis::Horizontal))
        );
        assert_eq!(hotkeys.direct.len(), 3);

        // Everything else passes through, unless it follows the leader key.
        assert_eq!(hotkeys.action(&character("w"), CTRL_SHIFT), None);
        assert_eq!(hotkeys.action(&Key::Named(Named::Tab), Modifiers::CTRL), None);
        assert_eq!(hotkeys.action(&character("1"), Modifiers::ALT), None);
        assert_eq!(hotkeys.action(&character("c"), Modifiers::empty()), None);

        let leader = hotkeys.leader().expect("no leader key");
        assert_eq!((&leader.key, leader.modifiers), (&character("a"), Modifiers::CTRL));
        assert_eq!(leader.timeout, Duration::from_millis(1000));

        let none = Modifiers::empty();
        let sequences = [
            (character("c"), none, AppAction::NewTab),
            (character("k"), none, AppAction::CloseTab),
            (character("n"), none, AppAction::NextTab),
            (character("p"), none, AppAction::PreviousTab),
            (character("1"), none, AppAction::SelectTab(0)),
            (character("9"), none, AppAction::SelectTab(8)),
            (character("0"), none, AppAction::LastTab),
            (character(","), none, AppAction::MoveTabLeft),
            (character("."), none, AppAction::MoveTabRight),
            (character("\\"), none, AppAction::Split(Axis::Vertical)),
            (character("-"), none, AppAction::Split(Axis::Horizontal)),
            (character("x"), none, AppAction::ClosePane),
            (character("z"), none, AppAction::ToggleZoom),
            (character("o"), none, AppAction::NextPane),
            (character(";"), none, AppAction::PreviousPane),
            (arrow_left(), none, AppAction::FocusPane(Direction::Left)),
            (arrow_left(), Modifiers::CTRL, AppAction::ResizePane(Direction::Left)),
            (Key::Named(Named::Space), none, AppAction::ShowCommands),
            (character("e"), none, AppAction::ShowProfiles),
        ];
        for (key, modifiers, action) in sequences {
            assert_eq!(hotkeys.sequence(&key, modifiers), Some(action), "{key:?} {modifiers:?}");
        }

        assert_eq!(hotkeys.describe(&AppAction::CloseTab).as_deref(), Some("Ctrl+A K"));
        assert_eq!(hotkeys.describe(&AppAction::NewTab).as_deref(), Some("Ctrl+Shift+T"));
    }

    #[test]
    fn macos_presets_use_command_keys() {
        let tabby = hotkeys("", true);
        assert_eq!(tabby, hotkeys("[keyboard]\npreset = \"WezTerm\"\n", true));

        let shift = Modifiers::LOGO | Modifiers::SHIFT;
        assert_eq!(tabby.action(&character("t"), Modifiers::LOGO), Some(AppAction::NewTab));
        assert_eq!(tabby.action(&character("w"), Modifiers::LOGO), Some(AppAction::CloseTab));
        assert_eq!(tabby.action(&character("9"), Modifiers::LOGO), Some(AppAction::LastTab));
        assert_eq!(tabby.action(&character("d"), shift), Some(AppAction::Split(Axis::Horizontal)));
        assert_eq!(
            tabby.action(&arrow_left(), Modifiers::LOGO | Modifiers::ALT),
            Some(AppAction::FocusPane(Direction::Left))
        );
        assert_eq!(tabby.action(&character("t"), CTRL_SHIFT), None);

        let leader = hotkeys("[keyboard]\npreset = \"Leader\"\n", true);
        assert_eq!(leader.direct.len(), 3);
        assert_eq!(leader.action(&character("t"), Modifiers::LOGO), Some(AppAction::NewTab));
        assert_eq!(
            leader.action(&character("d"), Modifiers::LOGO),
            Some(AppAction::Split(Axis::Vertical))
        );
        assert_eq!(leader.action(&character("w"), Modifiers::LOGO), None);
        assert_eq!(
            leader.sequence(&character("x"), Modifiers::empty()),
            Some(AppAction::ClosePane)
        );
    }

    #[test]
    fn user_bindings_replace_and_unbind_preset_keys() {
        let hotkeys = hotkeys(
            r#"
            [keyboard]
            bindings = [
                { key = "1", mods = "Alt", action = "ReceiveChar" },
                { key = "T", mods = "Control|Shift", action = "SplitDown" },
                { key = "G", mods = "Control|Shift", action = "FocusNextPane" },
            ]
            "#,
            false,
        );

        // Unbound: the key reaches the shell again.
        assert_eq!(hotkeys.action(&character("1"), Modifiers::ALT), None);
        assert_eq!(hotkeys.action(&character("2"), Modifiers::ALT), Some(AppAction::SelectTab(1)));
        // Replaced, and a new key.
        assert_eq!(
            hotkeys.action(&character("t"), CTRL_SHIFT),
            Some(AppAction::Split(Axis::Horizontal))
        );
        assert_eq!(hotkeys.action(&character("g"), CTRL_SHIFT), Some(AppAction::NextPane));
    }

    #[test]
    fn leader_bindings_and_the_leader_key_are_configurable() {
        // Leader bindings work with any preset.
        let hotkeys = hotkeys(
            r#"
            [keyboard]
            leader = { key = "B", mods = "Control", timeout = 250 }
            bindings = [{ key = "G", mode = "Leader", action = "TogglePaneZoom" }]
            "#,
            false,
        );
        assert_shared_keys_with_leader(&hotkeys);
        let leader = hotkeys.leader().expect("no leader key");
        assert_eq!((&leader.key, leader.modifiers), (&character("b"), Modifiers::CTRL));
        assert_eq!(leader.timeout, Duration::from_millis(250));
        assert_eq!(
            hotkeys.sequence(&character("g"), Modifiers::empty()),
            Some(AppAction::ToggleZoom)
        );
        assert_eq!(hotkeys.action(&character("g"), Modifiers::empty()), None);

        // Keys of the leader preset are replaced and unbound like direct ones.
        let hotkeys = self::hotkeys(
            r#"
            [keyboard]
            preset = "Leader"
            bindings = [
                { key = "C", mode = "Leader", action = "ReceiveChar" },
                { key = "X", mode = "Leader", action = "CloseTab" },
            ]
            "#,
            false,
        );
        assert_eq!(hotkeys.sequence(&character("c"), Modifiers::empty()), None);
        assert_eq!(
            hotkeys.sequence(&character("x"), Modifiers::empty()),
            Some(AppAction::CloseTab)
        );
        assert_eq!(hotkeys.sequence(&character("n"), Modifiers::empty()), Some(AppAction::NextTab));
    }

    /// The hotkeys of a configuration on this platform, with the default bindings.
    fn configured(config: &str) -> Hotkeys {
        Hotkeys::from_config(&toml::from_str(config).unwrap())
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn default_bindings_copy_paste_scroll_and_change_the_font_size() {
        use keyboard::key::Named;

        let hotkeys = configured("");
        let none = BindingMode::empty();

        let characters = [
            ("c", CTRL_SHIFT, AppAction::Copy),
            ("v", CTRL_SHIFT, AppAction::Paste),
            ("0", Modifiers::CTRL, AppAction::ResetFontSize),
            ("=", Modifiers::CTRL, AppAction::IncreaseFontSize),
            ("+", Modifiers::CTRL, AppAction::IncreaseFontSize),
            ("-", Modifiers::CTRL, AppAction::DecreaseFontSize),
        ];
        for (key, modifiers, action) in characters {
            assert_eq!(pressed(&hotkeys, key, modifiers, none), runs(&[action]), "{key}");
        }

        let named = [
            (Named::Copy, Modifiers::empty(), AppAction::Copy),
            (Named::Paste, Modifiers::empty(), AppAction::Paste),
            (Named::Insert, Modifiers::SHIFT, AppAction::PasteSelection),
            (Named::PageUp, Modifiers::SHIFT, AppAction::Scroll(ScrollTo::PageUp)),
            (Named::PageDown, Modifiers::SHIFT, AppAction::Scroll(ScrollTo::PageDown)),
            (Named::Home, Modifiers::SHIFT, AppAction::Scroll(ScrollTo::Top)),
            (Named::End, Modifiers::SHIFT, AppAction::Scroll(ScrollTo::Bottom)),
        ];
        for (key, modifiers, action) in named {
            assert_eq!(pressed_named(&hotkeys, key, modifiers, none), runs(&[action]), "{key:?}");
        }

        // The tab and pane keys of the preset are still there.
        assert_eq!(pressed(&hotkeys, "t", CTRL_SHIFT, none), runs(&[AppAction::NewTab]));

        // `Control+L` is bound to `ReceiveChar` next to an action that doesn't exist yet, and
        // keys without a binding aren't touched.
        assert_eq!(pressed(&hotkeys, "l", Modifiers::CTRL, none), unbound());
        assert_eq!(pressed(&hotkeys, "c", Modifiers::CTRL, none), unbound());
        assert_eq!(pressed_named(&hotkeys, Named::PageUp, Modifiers::empty(), none), unbound());
    }

    #[test]
    fn bindings_follow_the_terminal_modes() {
        use keyboard::key::Named;

        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [
                { key = "U", mods = "Alt", mode = "Alt", action = "ScrollHalfPageUp" },
                { key = "D", mods = "Alt", mode = "AppCursor|~AppKeypad", action = "ScrollHalfPageDown" },
                { key = "]", mode = "Leader|Alt", action = "ScrollPageDown" },
            ]
            "#,
        );
        let page_up = |mode| pressed_named(&hotkeys, Named::PageUp, Modifiers::SHIFT, mode);

        // On the alternate screen the scrolling keys belong to the application.
        assert_eq!(page_up(BindingMode::empty()), runs(&[AppAction::Scroll(ScrollTo::PageUp)]));
        assert_eq!(page_up(BindingMode::APP_CURSOR), runs(&[AppAction::Scroll(ScrollTo::PageUp)]));
        assert_eq!(page_up(BindingMode::ALT_SCREEN), unbound());

        let half_up = runs(&[AppAction::Scroll(ScrollTo::HalfPageUp)]);
        assert_eq!(pressed(&hotkeys, "u", Modifiers::ALT, BindingMode::empty()), unbound());
        assert_eq!(pressed(&hotkeys, "u", Modifiers::ALT, BindingMode::ALT_SCREEN), half_up);

        let half_down = runs(&[AppAction::Scroll(ScrollTo::HalfPageDown)]);
        let both = BindingMode::APP_CURSOR | BindingMode::APP_KEYPAD;
        assert_eq!(pressed(&hotkeys, "d", Modifiers::ALT, BindingMode::APP_CURSOR), half_down);
        assert_eq!(pressed(&hotkeys, "d", Modifiers::ALT, both), unbound());

        // Keys after the leader key can be limited to terminal modes as well.
        let after_leader = |mode| {
            let location = keyboard::Location::Standard;
            let press = KeyPress { key: &character("]"), modifiers: Modifiers::empty(), location };
            hotkeys.press_after_leader(&press, mode).actions
        };
        assert_eq!(after_leader(BindingMode::empty()), []);
        assert_eq!(after_leader(BindingMode::ALT_SCREEN), [AppAction::Scroll(ScrollTo::PageDown)]);
    }

    #[test]
    fn vi_and_search_bindings_are_inactive() {
        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [
                { key = "J", mods = "Alt", mode = "Vi", action = "ScrollLineDown" },
                { key = "K", mods = "Alt", mode = "Search", action = "ScrollLineUp" },
                { key = "U", mods = "Alt", mode = "~Vi|~Search", action = "ScrollHalfPageUp" },
            ]
            "#,
        );
        let none = BindingMode::empty();

        assert_eq!(pressed(&hotkeys, "j", Modifiers::ALT, none), unbound());
        assert_eq!(pressed(&hotkeys, "k", Modifiers::ALT, none), unbound());
        // Not being in a mode that doesn't exist is no condition at all.
        assert_eq!(
            pressed(&hotkeys, "u", Modifiers::ALT, none),
            runs(&[AppAction::Scroll(ScrollTo::HalfPageUp)])
        );

        // The default vi mode keys are ordinary keys: `y` copies in vi mode only.
        for key in ["y", "g", "v", "i"] {
            assert_eq!(pressed(&hotkeys, key, Modifiers::empty(), none), unbound(), "{key}");
        }
    }

    #[test]
    fn chars_commands_and_window_actions_are_bound() {
        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [
                { key = "L", mods = "Alt", chars = "clear\n" },
                { key = "B", mods = "Alt", command = { program = "notify-send", args = ["hi"] } },
                { key = "E", mods = "Alt", command = "xdg-open" },
                { key = "Q", mods = "Alt", action = "Quit" },
                { key = "H", mods = "Alt", action = "Hide" },
                { key = "M", mods = "Alt", action = "Minimize" },
                { key = "X", mods = "Alt", action = "ClearSelection" },
                { key = "Y", mods = "Alt", action = "CopySelection" },
                { key = "K", mods = "Alt", chars = "\u000c" },
                { key = "K", mods = "Alt", action = "ClearHistory" },
                { key = "Z", mode = "Leader", chars = "zz" },
            ]
            "#,
        );
        let alt = |key| pressed(&hotkeys, key, Modifiers::ALT, BindingMode::empty());

        assert_eq!(alt("l"), runs(&[AppAction::Chars("clear\n".into())]));
        assert_eq!(
            alt("b"),
            runs(&[AppAction::RunCommand(Program::WithArgs {
                program: "notify-send".into(),
                args: vec!["hi".into()],
            })])
        );
        assert_eq!(alt("e"), runs(&[AppAction::RunCommand(Program::Just("xdg-open".into()))]));
        assert_eq!(alt("q"), runs(&[AppAction::Quit]));
        assert_eq!(alt("h"), runs(&[AppAction::Hide]));
        assert_eq!(alt("m"), runs(&[AppAction::Minimize]));
        assert_eq!(alt("x"), runs(&[AppAction::ClearSelection]));
        assert_eq!(alt("y"), runs(&[AppAction::CopySelection]));
        // Like Command+K on macOS: clear the screen, then the scrollback.
        assert_eq!(alt("k"), runs(&[AppAction::Chars("\x0c".into()), AppAction::ClearHistory]));
        assert_eq!(
            hotkeys.sequence(&character("z"), Modifiers::empty()),
            Some(AppAction::Chars("zz".into()))
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn default_chars_bindings_are_left_to_the_terminal() {
        use keyboard::key::Named;

        let hotkeys = configured("");
        let named = |key, modifiers, mode| pressed_named(&hotkeys, key, modifiers, mode);

        // The terminal widget encodes these keys itself, under the kitty keyboard protocol too.
        assert_eq!(named(Named::Backspace, Modifiers::empty(), BindingMode::empty()), unbound());
        assert_eq!(named(Named::Backspace, Modifiers::ALT, BindingMode::empty()), unbound());
        assert_eq!(named(Named::Tab, Modifiers::SHIFT, BindingMode::empty()), unbound());
        assert_eq!(named(Named::F1, Modifiers::empty(), BindingMode::empty()), unbound());
        assert_eq!(named(Named::ArrowUp, Modifiers::empty(), BindingMode::APP_CURSOR), unbound());
        assert_eq!(named(Named::Home, Modifiers::empty(), BindingMode::APP_CURSOR), unbound());

        // A key the user binds to `chars` is theirs, default binding or not.
        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [{ key = "Back", chars = "\u0008" }]
            "#,
        );
        assert_eq!(
            pressed_named(&hotkeys, Named::Backspace, Modifiers::empty(), BindingMode::empty()),
            runs(&[AppAction::Chars("\x08".into())])
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn receive_char_gives_a_key_back_and_none_swallows_it() {
        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [
                { key = "V", mods = "Control|Shift", action = "ReceiveChar" },
                { key = "C", mods = "Control|Shift", action = "None" },
                { key = "B", action = "None" },
            ]
            "#,
        );
        let none = BindingMode::empty();

        // The default paste binding is gone and the key reaches the terminal.
        assert_eq!(pressed(&hotkeys, "v", CTRL_SHIFT, none), unbound());
        // Nothing is copied, and the key doesn't reach the terminal either.
        assert_eq!(pressed(&hotkeys, "c", CTRL_SHIFT, none), runs(&[]));
        assert_eq!(pressed(&hotkeys, "b", Modifiers::empty(), none), runs(&[]));
        // Other default bindings are untouched.
        assert_eq!(
            pressed(&hotkeys, "0", Modifiers::CTRL, none),
            runs(&[AppAction::ResetFontSize])
        );
    }

    #[test]
    fn actions_are_rebound_and_keys_run_all_of_their_bindings() {
        use keyboard::key::Named;

        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [
                { key = "Y", mods = "Alt", action = "Copy" },
                { key = "P", mods = "Alt", action = "Paste" },
                { key = "K", mods = "Alt", action = "ScrollToBottom" },
                { key = "K", mods = "Alt", action = "ClearHistory" },
                { key = "K", mods = "Alt", action = "ReceiveChar" },
                { key = "F11", action = "ToggleFullscreen" },
                { key = "F11", mods = "Shift", action = "ToggleMaximized" },
                { key = "PageUp", mods = "Shift", action = "ScrollLineUp" },
                { key = "[", mode = "Leader", action = "ScrollPageUp" },
            ]
            "#,
        );
        let none = BindingMode::empty();

        assert_eq!(pressed(&hotkeys, "y", Modifiers::ALT, none), runs(&[AppAction::Copy]));
        assert_eq!(pressed(&hotkeys, "p", Modifiers::ALT, none), runs(&[AppAction::Paste]));
        assert_eq!(
            pressed_named(&hotkeys, Named::F11, Modifiers::empty(), none),
            runs(&[AppAction::ToggleFullscreen])
        );
        assert_eq!(
            pressed_named(&hotkeys, Named::F11, Modifiers::SHIFT, none),
            runs(&[AppAction::ToggleMaximized])
        );

        // Every binding of a key runs, in the order they are written in, and `ReceiveChar`
        // among them lets the terminal have the key as well.
        assert_eq!(pressed(&hotkeys, "k", Modifiers::ALT, none), Triggered {
            actions: vec![AppAction::Scroll(ScrollTo::Bottom), AppAction::ClearHistory],
            captured: false,
        });

        // A user binding replaces the default one of its key, on the alternate screen too.
        let line_up = runs(&[AppAction::Scroll(ScrollTo::LineUp)]);
        assert_eq!(pressed_named(&hotkeys, Named::PageUp, Modifiers::SHIFT, none), line_up);
        let alt_screen = BindingMode::ALT_SCREEN;
        assert_eq!(pressed_named(&hotkeys, Named::PageUp, Modifiers::SHIFT, alt_screen), line_up);

        // Every action can follow the leader key.
        assert_eq!(
            hotkeys.sequence(&character("["), Modifiers::empty()),
            Some(AppAction::Scroll(ScrollTo::PageUp))
        );
        assert_eq!(pressed(&hotkeys, "[", Modifiers::empty(), none), unbound());
    }

    #[test]
    fn numpad_bindings_only_take_the_numpad_key() {
        let hotkeys = configured(
            r#"
            [keyboard]
            bindings = [
                { key = "NumpadAdd", mods = "Alt", action = "IncreaseFontSize" },
                { key = "Minus", mods = "Alt", action = "DecreaseFontSize" },
            ]
            "#,
        );
        let press = |key: &str, location| {
            let key = character(key);
            let press = KeyPress { key: &key, modifiers: Modifiers::ALT, location };
            hotkeys.press(&press, BindingMode::empty())
        };

        assert_eq!(press("+", keyboard::Location::Numpad), runs(&[AppAction::IncreaseFontSize]));
        assert_eq!(press("+", keyboard::Location::Standard), unbound());
        assert_eq!(press("-", keyboard::Location::Standard), runs(&[AppAction::DecreaseFontSize]));
        assert_eq!(press("-", keyboard::Location::Numpad), unbound());
    }

    /// The shared direct keys still work next to leader bindings.
    fn assert_shared_keys_with_leader(hotkeys: &Hotkeys) {
        assert_eq!(hotkeys.action(&character("t"), CTRL_SHIFT), Some(AppAction::NewTab));
        assert_eq!(hotkeys.action(&character("1"), Modifiers::ALT), Some(AppAction::SelectTab(0)));
    }
}
