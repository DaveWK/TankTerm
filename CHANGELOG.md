# Changelog

All notable changes to TankTerm are documented in this file.
The sections should follow the order `Added`, `Changed`, `Fixed` and `Removed`.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

Notable changes to the `tankterm_terminal` crate are documented in its
[CHANGELOG](./crates/tankterm_terminal/CHANGELOG.md).

## 0.18.0-dev

### Added

- Key and mouse bindings in the configuration file that do nothing in TankTerm, such as those to
    `CreateNewWindow` or to the left mouse button, are logged as warnings
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
    work on Linux, and elsewhere with shell integration
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
- `[style.tab_bar]`, `[style.tab_active]`, `[style.tab_inactive]`, `[style.tab_button]`,
    `[style.tab_hovered]`, `[style.tab_activity]` and `[style.tab_attention]` style the tab bar,
    with their `[style.blurred.tab_*]` variants. Styles take Lip Gloss's `padding`, `margin`,
    `border` (`Normal`, `Rounded`, `Thick`, `Hidden`) and `border_foreground`, which only the
    tab bar uses
- Tab bar styles take Lip Gloss's `border_top`, `border_right`, `border_bottom` and `border_left`
    for borders on single sides, such as a line under the shown tab, a color for each side
    (`border_top_foreground` and so on), and the `Double`, `Block` and `ASCII` borders. Tabs take
    `width` and `max_width` in terminal cells in place of the fixed 240 pixels, and
    `[style.tab_bar]` takes `gap`, the space between the tabs
- `tabs.title` sets what tabs show in the tab bar, with `{index}`, `{title}`, `{panes}`, `{cwd}`,
    `{path}`, `{process}`, `{marker}` and `{zoom}` placeholders; zoomed tabs are marked with ` Z`
- Status segments at the end of the tab bar, as `[[tabs.status]]` tables: the active key table,
    a clock with a strftime format, the focused pane's directory and the battery charge, each
    with a `{placeholder}` format and a style on top of `[style.tab_status]`
- `Zoom`, `Count`, `Text`, `Host`, `Process` and `Load` status segments, and `side = "Left"` to
    put a segment before the tabs. Segments with an empty text aren't drawn, and segments touch,
    so `Text` segments can join them with separators
- `Command` status segments show the first line a program prints, run every `interval` seconds
    in the focused pane's directory
- With `window.decorations = "None"` the tab bar is the title bar: it is always shown, has
    minimize, maximize and close buttons where `window.buttons` says (`Desktop`, the default,
    follows KDE's and GNOME's button layout; `Right`, `Left`, `None`), moves the window when
    dragged, maximizes it when double-clicked and opens the window menu when right-clicked.
    The window's edges and corners resize it, the maximize button shows a restore icon while the
    window is maximized, and the close button turns red under the mouse
    (`[style.tab_close_hovered]`)
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
- A gear at the end of the tab bar and the `OpenConfig` action open the configuration file in
    `$VISUAL` or `$EDITOR` in a new tab, creating it if there is none; a configuration file
    created while TankTerm runs is read like one that was there from the start
- Shell integration: OSC 133 prompt marks and OSC 7 working directories, with scripts for bash,
    zsh and fish in `extra/shell-integration`. Prompts are marked at the left edge, in red after a
    failed command; `ScrollToPreviousPrompt`, `ScrollToNextPrompt` and `SelectCommandOutput` move
    between prompts and select a command's output. Finished commands are noticed from the marks,
    and new tabs and panes start in the reported directory, on every platform
- Default keys for the prompt actions: `Control+Shift+Up`/`Down` to scroll to prompts (`Shift` in
    the `WezTerm` preset, `Command` on macOS), left to the application on the alternate screen,
    and `Control+Shift+G` (`Command+Shift+A` on macOS) to select a command's output.
    `terminal.prompt_marks = false` hides the marks at the left edge
- bash, zsh and fish load the shell integration when TankTerm starts them, on Unix, like in
    Ghostty: through `ENV` and POSIX mode, `ZDOTDIR` and `XDG_DATA_DIRS`, with the user's startup
    files read as before. `terminal.shell_integration = "None"` turns it off; the scripts are in
    `$TANKTERM_SHELL_INTEGRATION_DIR` for shells started otherwise
- `Alt` and a right click select the output of the command under the mouse
    (`SelectCommandOutput` on a mouse binding)
- A click in a shell's command line moves its cursor there, for shells that take clicks with
    OSC 133 `click_events` (fish 4.1 and later) or `cl` (the bash and zsh scripts);
    `mouse.click_to_move = false` turns it off
- Scrollback search in the iced front end: `SearchForward` and `SearchBackward` open a bar on the
    terminal's last row, and the search actions, its history and the `search_match`,
    `search_focused_match` and `footer_bar` styles work as they did in the OpenGL front end.
    The command palette offers both searches
- Vi mode in the iced front end: `ToggleViMode`, the motions, selections, copying, searching from
    vi mode and the vi mode actions. The `vi_mode_cursor` and `line_indicator` styles and
    `cursor.vi_mode_style` apply, and the command palette offers "Toggle Vi Mode"
- Hints in the iced front end: a hint's `binding` labels its matches to choose one by typing its
    label, a hint for the mouse is underlined under the pointer with its `mouse.mods` held and
    chosen with a click, and the vi mode `Open` action chooses the one under the vi cursor.
    `Copy`, `Paste`, `Select`, `MoveViModeCursor` and `command` hints work, the `hint_start` and
    `hint_end` styles apply, and the URIs of hyperlinks under the mouse or the vi cursor are shown
    at the bottom of the terminal. URLs open with a plain click and with `Control+Shift+O`, as the
    default URL hint says, instead of with `Control`+click
- `tankTerm ssh` runs `ssh` with a `TERM` the server has. Hosts matching the `[ssh]` table's
    `probe` patterns are checked once, over the connection the session then reuses, for the
    terminfo entries in `order` (`alacritty`, `xterm-256color`, `xterm`); other hosts get
    `fallback` (`xterm-256color`). Answers are cached per user, host and port as `ssh -G`
    resolves them, `--reprobe`, `--probe` and `--no-probe` override the cache and the patterns,
    and Debian and Ubuntu servers without `alacritty` get a hint to install `ncurses-term`.
    Not available on Windows
- `tankTerm migrate` converts the configuration of Alacritty, Tabby, WezTerm or Ghostty, or
    Crush's theme, into TankTerm's: settings, colors, profiles and the key bindings whose actions
    TankTerm has, with a summary of what was converted and what was skipped and why. The source
    is never changed; existing TankTerm files are only replaced after a yes or `--force`, and
    are backed up to `<name>.bak` first. `--from` picks the source, `--dry-run` prints the result
- TankTerm logs a hint about `tankTerm migrate` when it starts without a configuration file and
    finds another terminal's

### Changed

- `window.decorations` defaults to `None`: the tab bar is the window's title bar, shown even with
    one tab and whatever `tabs.show` says; `decorations = "Full"` brings back the system's title
    bar, and with it `tabs.show = "Multiple"` and `"Never"`
- A tab bar that is the title bar has TankTerm's icon at its left end, or where KDE's or GNOME's
    button layout puts the window menu; clicking it opens the window menu
- The tab bar's `+`, gear, window buttons and close buttons are drawn as thin line icons
    instead of font glyphs, and the gear keeps a gap from the window buttons after it
- The tab bar is restyled: it has the terminal's background and is as see-through as the
    terminals, the shown tab is lifted off the bar a little with a line along its top, other
    tabs and buttons have no background of their own, and the bar is taller (`padding = [9, 14]`)
- A hovered tab or button is drawn in place of the normal one rather than over it
- The default `tabs.title` is `"{index}  {title}{zoom}{marker}"`, without the colon, and the
    tab's number is drawn faint, in the new `[style.tab_index]`
- A tab shows its close button only while the mouse is over it; `tabs.close_button = "Always"`
    brings back one on every tab, and `"Never"` leaves it out
- Tabs are as wide as their titles, up to 240 pixels, instead of sharing the bar evenly;
    `tabs.width = "Shared"` brings that back
- A tab whose programs set no title is named after where its shell is, as `user@host:path`,
    instead of `TankTerm`, while `window.dynamic_title` is on; this is also the window title and
    the `title` and `tab_title` of `tankTerm msg list`
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
    that rang, and the command runs in the directory of that terminal's shell where that is known
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

- `tankTerm migrate --skip-renames`, and migrating TankTerm's own configuration file in place;
    `migrate` converts other terminals' configurations instead
- `--daemon`, and the `create-window`, `config` and `get-config` messages of `tankTerm msg`
- Key bindings to the key help (`ToggleHelp`), new windows (`SpawnNewInstance`,
    `CreateNewWindow`), `HideOtherApplications`, `ClearLogNotice` and `ToggleSimpleFullscreen`;
    mouse bindings other than pasting, expanding the selection and selecting a command's output;
    the message bar
- `--embed`, `--print-events`, `--ref-test` and the `debug.renderer`, `debug.prefer_egl`,
    `debug.render_timer`, `debug.highlight_damage` and `debug.print_events` options, which only
    applied to the OpenGL front end

### Fixed

- `Alt` with a character, `Enter`, `Tab`, `Space` or `Escape` sends an escape before the key's text
    outside the Kitty keyboard protocol too, so `Alt+B` or `Alt+.` work in the shell. Characters
    were sent without it, and the other keys weren't sent at all
- `window.dimensions` opening more columns than it says, and `window.padding` in cells being
    bigger than the cells, with a font whose cells aren't whole pixels
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
- Log messages from the terminal widget being dropped, such as the error for a hint regex that
    doesn't compile
- A program's last output missing from a held terminal (`--hold`) when the window was drawing
    the terminal as the program exited
- Prompts that fish 3 draws again, such as after a resize, losing their mark; the fish script
    leaves the prompt marks to fish 4, which sends them itself

## Forked from Alacritty

TankTerm was forked from Alacritty at commit
[`aa5d942f`](https://github.com/alacritty/alacritty/commit/aa5d942f46c40184a3f214830ff5f7989f4733a1).
