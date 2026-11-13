# Changelog

All notable changes to TankTerm are documented in this file.
The sections should follow the order `Added`, `Changed`, `Fixed` and `Removed`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

Notable changes to the `tankterm_terminal` crate are documented in its
[CHANGELOG](./crates/tankterm_terminal/CHANGELOG.md).

## 0.18.0-dev

### Added

- Tabs: a tab bar, shown while there is more than one tab, with `CreateNewTab`, `CloseTab`,
    `SelectNextTab`, `SelectPreviousTab`, `SelectTab1` to `SelectTab9`, `SelectLastTab`,
    `MoveTabLeft` and `MoveTabRight` key binding actions, like in Tabby and WezTerm
- Split panes: `SplitRight`, `SplitDown`, `ClosePane`, `FocusPaneLeft`/`Right`/`Up`/`Down`,
    `FocusNextPane`, `FocusPreviousPane`, `TogglePaneZoom` and `ResizePaneLeft`/`Right`/`Up`/`Down`
    key binding actions; dividers can also be dragged with the mouse
- `[tabs]` config section with `show` (`Always`, `Multiple`, `Never`) and `position` (`Top`,
    `Bottom`) for the tab bar
- Command palette (`ShowCommandPalette`, `Ctrl+Shift+P`) listing the tab and pane actions with
    their key bindings, like in Tabby and WezTerm
- Shell profiles in `[[profiles]]` with their own `shell`, `working_directory`, `env` and `theme`,
    opened in a new tab from the profile selector (`ShowProfiles`, `Ctrl+Shift+E`)
- `tabs.restore` to open the tabs and split panes of the last window on startup, like Tabby's
    tab recovery; the layout is remembered, the shells are started fresh
- `tabs.inherit_working_directory`, on by default, starts the shells of new tabs and panes in the
    directory of the current pane's shell; restored panes start in their directories too. Both
    only work on Linux
- Tabs that aren't shown are marked in the tab bar when they print something (`*`)
- Terminals that ring the bell or finish a command while they aren't looked at mark their tab
    (`!`) and pane, and ask the desktop for attention if the window isn't focused. Finished
    commands are noticed on Unix without shell integration
- `keyboard.preset` selects the default tab and pane keys: `Tabby` (default), `WezTerm` or
    `Leader`, which reaches most actions through a leader key like tmux
- `keyboard.leader`, the leader key (`Control+A` by default) with its timeout, and the `Leader`
    binding mode for keys pressed after it
- `terminal.confirm_multiline_paste`, on by default, asks before pasting text with line breaks
    into a program without bracketed paste
- Mouse bindings of the middle and right buttons to `Paste` and `PasteSelection`, for example
    for pasting with a right click; the middle button and `Shift+Insert` paste the selection
- Selected text is put on the selection clipboard, and `selection.save_to_clipboard` copies it
    to the clipboard as well
- `Spring` visual bell animation with `bell.spring`, a damped spring like Charm's Harmonica
- `general.accessible`, or `ACCESSIBLE`/`TANKTERM_ACCESSIBLE` like Huh, turns off cursor blinking and
    the visual bell
- `[style.blurred]` styles for unfocused windows, like Huh's focused and blurred styles
- Key help like Charm's Bubbles: `ToggleHelp` (`Ctrl+Shift+H`, `Cmd+Shift+H` on macOS) shows the key
    bindings as a single line or in columns, with `help` descriptions on bindings and
    `[style.help_*]` blocks
- Themes in the style of Glamour and Huh: `theme = "<name or path>"`, `--theme` or `TANKTERM_THEME`,
    with built-in `green_screen`, `charm`, `dracula`, `catppuccin` and `base16` themes
- Colors can be named: ANSI names like `"bright_red"` and CharmTone names like `"charple"`
- Color scheme reporting: `CSI ? 996 n` queries and mode 2031 notifications tell applications whether
    the colors are dark or light, following `colors.mode` or the background
- Colors can be given like in Lip Gloss: ANSI colors (`"5"`, `"201"`) and `{ light, dark }` pairs
- `colors.mode` to choose the light or dark color of pairs; detected from the background by default
- `[style]` blocks for the cursor, selection, search matches, hints and bars, with Lip Gloss style
    properties (colors, bold, italic, faint, underline, strikethrough, reverse)
- Named styles in `[style.defs]` that other styles can `inherit` from, like Lip Gloss's `Inherit`
- `window.padding` accepts Lip Gloss/CSS shorthand in terminal cells, like `[1, 2]`, including
    different padding on every side
- `tankTerm msg` commands for tabs and panes, following `wezterm cli`: `list`, `spawn`,
    `split-pane`, `send-text`, `get-text`, `activate-pane`, `activate-tab`, `kill-pane`,
    `zoom-pane` and `set-tab-title`. Shells find their pane in `TANKTERM_PANE_ID`, and errors
    are reported with exit status 1
- Key tables like WezTerm's in `[keyboard.tables]`, activated by a binding's `table` and left with
    `PopKeyTable`, `ClearKeyTableStack`, `one_shot`, `timeout` or `until_unknown`; the active table
    is shown in the tab bar
- `PaneSelect` and `PaneSelectSwap` label the panes to focus or swap one by typing its label,
    `RotatePanesClockwise` and `RotatePanesCounterClockwise` move every pane to the next place,
    `ShowTabNavigator` lists the tabs and `RenameTab` gives a tab a title, which the session file
    remembers. The `Leader` preset binds them to `Q`, `W` and `Control+O` like tmux

### Changed

- Logs follow Charm's log layout (`<time> <LEVEL> <prefix>: <message>`), styled on a terminal unless
    `NO_COLOR` is set, with JSON and logfmt output through `TANKTERM_LOG_FORMAT`
- Default config location is now `tankterm/tankterm.toml`; `alacritty` config locations are no
    longer read
- Default window title and class are now `TankTerm`
- `Command+W` closes the current tab instead of quitting on macOS; the window still closes with
    its last tab
- With the default `Tabby` key preset, `Alt+1` to `Alt+9`, `Control+Alt+Arrow` and the other tab
    and pane keys no longer reach the applications in the terminal; bind a key to `ReceiveChar` to
    give it back to them
- Pasted line breaks are sent as carriage returns, like the Enter key, to programs without
    bracketed paste
- The terminal has keyboard focus when the window opens, without clicking it first
- New TankTerm logo replaces the Alacritty one in the window, application and installer icons
- Default font is now `Iosevka Term`; if it can't be loaded, the system monospace font is
    used
- Default color scheme is now green on black (`#00ff00` on `#000000`), with matching cursor,
    selection, search and hint colors
- `ALACRITTY_*` environment variables renamed to `TANKTERM_*`
- IPC sockets and log files now use the `TankTerm-` prefix and the `tankterm` runtime directory
- The window and rendering use iced through `iced_tterm` instead of the OpenGL renderer
- Config file font, colors, shell, environment, working directory and the window's title, class,
    `dimensions`, `position`, `startup_mode`, `decorations` and `level` apply to the iced front
    end; other config options are ignored for now. `Transparent` and `Buttonless` decorations are
    drawn like `Full`
- `scrolling.history` and `scrolling.multiplier` apply to the iced front end
- Key bindings to `Copy`, `Paste`, `PasteSelection`, `IncreaseFontSize`, `DecreaseFontSize`,
    `ResetFontSize`, the `Scroll` actions, `ClearHistory`, `ToggleFullscreen` and
    `ToggleMaximized` apply to the iced front end and can be changed, or unset with `ReceiveChar`
    and `None`, like the tab and pane keys
- The font size actions change the font size of every terminal in the window, a pixel at a
    time and down to four pixels
- Tab and pane key bindings can be limited to the `AppCursor`, `AppKeypad` and `Alt` modes
- `cursor.style`, `cursor.blink_interval`, `cursor.blink_timeout`, `cursor.unfocused_hollow` and
    `cursor.thickness` apply to the iced front end, and so do the cursor shapes and blinking
    applications ask for
- The cursor only blinks in the terminal that has the keyboard focus, and with
    `cursor.unfocused_hollow` it is hollow in the panes that don't have it, not only in a window
    without focus
- `bell.command` and the visual bell (`bell.duration`, `bell.color`, `bell.animation` and
    `bell.spring`) apply to the iced front end; the visual bell flashes the pane of the terminal
    that rang, and the command runs in the directory of that terminal's shell on Linux
- `general.live_config_reload` applies to the iced front end: within a second of a change to
    the configuration file or a file it imports, the colors, font, padding, key and mouse
    bindings, tabs, profiles, scrolling, cursor and bell options of the running window change.
    The window's `dimensions`, `position`, `startup_mode`, `decorations`, `level` and `class`
    need a restart, and running shells keep their program and environment
- `window.opacity` applies to the iced front end: the window is transparent and the background
    of the terminals and their padding is drawn at that opacity, while cells with a background
    color of their own, the tab bar and the command palette stay opaque. A window that started
    opaque needs a restart to become transparent
- The padding around a terminal takes the background color its application sets with OSC 11

### Removed

- `--daemon`, and the `create-window`, `config` and `get-config` messages of `tankTerm msg`
- Key bindings to `command`, `chars` and the actions for vi mode, search, hints, the key help,
    new windows, hiding, minimizing and quitting; mouse bindings other than pasting; hints, vi
    mode, search and the message bar
- `--hold`
- `--embed`, `--print-events`, `--ref-test` and the `debug.renderer`, `debug.prefer_egl`,
    `debug.render_timer`, `debug.highlight_damage` and `debug.print_events` options, which only
    applied to the OpenGL front end

### Fixed

- `-o colors.mode=...` not changing which color of light/dark pairs is used
- Setting only one color of `colors.cursor`, `colors.selection`, search or hint colors no longer
    resets the other one
- Config file changes not watched after `general.import` changes, and the watcher restarting
    on every reload when imports didn't change
- Possible deadlock on exit with ConPTY on Windows, from the window closing after the console
    was detached
- `CSI 1 J` (erase above) not clearing the first line when the cursor is on the second line
- Possible crash when a tab or pane key is pressed while the window closes with its last tab
- Pasting again while the paste warning is shown replacing the text the warning asks about;
    nothing is pasted until the warning is answered
- Resizing a zoomed pane with the keyboard moving a divider that isn't shown; the zoom ends
    first, like when focus moves
- Session file not written again after writing it failed, until the tabs or panes changed

## Forked from Alacritty

TankTerm was forked from Alacritty at commit
[`aa5d942f`](https://github.com/alacritty/alacritty/commit/aa5d942f46c40184a3f214830ff5f7989f4733a1).
