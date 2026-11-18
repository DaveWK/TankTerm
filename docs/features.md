# Features

This document gives an overview over TankTerm's features beyond its terminal
emulation capabilities. To get a list with supported control sequences take a
look at the [tankTerm-escapes(7) manpage].

[tankTerm-escapes(7) manpage]: ../extra/man/tankTerm-escapes.7.scd

## Vi Mode

The vi mode allows moving around TankTerm's viewport and scrollback using the
keyboard. It also serves as a jump-off point for other features like search and
opening URLs with the keyboard. By default you can launch it using
<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Space</kbd>.

### Motion

The cursor motions are setup by default to mimic vi, however they are fully
configurable. If you don't like vi's bindings, take a look at the configuration
file to change the various movements.

The top right corner shows how many lines the vi cursor is above the last line,
and the vi cursor takes the `[style.vi_mode_cursor]` colors and the
`cursor.vi_mode_style` shape.

### Selection

One useful feature of vi mode is the ability to make selections and copy text to
the clipboard. By default you can start a selection using <kbd>v</kbd> and copy
it using <kbd>y</kbd>. All selection modes that are available with the mouse can
be accessed from vi mode, including the semantic (<kbd>Alt</kbd> <kbd>v</kbd>),
line (<kbd>Shift</kbd> <kbd>v</kbd>) and block selection (<kbd>Ctrl</kbd>
<kbd>v</kbd>). You can also toggle between them while the selection is still
active. A selection made in vi mode goes on the selection clipboard, like one
made with the mouse.

## Search

Search allows you to find anything in TankTerm's scrollback buffer. You can
search forward using <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>f</kbd> (<kbd>Command</kbd> <kbd>f</kbd> on macOS) and
backward using <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>b</kbd> (<kbd>Command</kbd> <kbd>b</kbd> on macOS).

### Vi Search

In vi mode the search is bound to <kbd>/</kbd> for forward and <kbd>?</kbd> for
backward search. This allows you to move around quickly and help with selecting
content. The `SearchStart` and `SearchEnd` keybinding actions can be bound if
you're looking for a way to jump to the start or the end of a match.

### Normal Search

The regex you type goes into a bar on the terminal's last row, and the view
scrolls to the first match while you type. Matches are drawn in the
`[style.search_match]` style and the focused one in
`[style.search_focused_match]`. During normal search you don't have the
opportunity to move around freely, but you can still jump between matches using
<kbd>Enter</kbd> and <kbd>Shift</kbd> <kbd>Enter</kbd>. After leaving search
with <kbd>Escape</kbd> your active match stays selected, allowing you to easily
copy it. The top right corner shows how many lines the view is scrolled back.

<kbd>Up</kbd> and <kbd>Down</kbd> go through the regexes you searched for
before, <kbd>Ctrl</kbd> <kbd>w</kbd> deletes the last word and <kbd>Ctrl</kbd>
<kbd>u</kbd> the whole regex. Pasted text goes into the bar, without its line
breaks. The command palette offers the search too.

## Hints

Terminal hints allow easily interacting with visible text without having to
start vi mode. They consist of a regex that detects these text elements and then
either feeds them to an external application or triggers one of TankTerm's
built-in actions.

A hint's key binding labels its matches, in the `[style.hint_start]` and
`[style.hint_end]` styles, and typing a label triggers the hint for that match.
<kbd>Escape</kbd> takes the labels away and <kbd>Backspace</kbd> takes back the
last key typed. By default <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>o</kbd> labels
the URLs.

Hints can also be triggered using the mouse or vi mode cursor. If a hint is
enabled for mouse interaction and recognized as such, it will be underlined when
the mouse or vi mode cursor is on top of it. Using the left mouse button or
<kbd>Enter</kbd> key in vi mode will then trigger the hint. The URI of a
hyperlink underlined this way is shown at the bottom of the terminal.

Hints can be configured in the `hints` and `colors.hints` sections in the
TankTerm configuration file.

## Selection expansion

After making a selection, you can use the right mouse button to expand it.
Double-clicking will expand the selection semantically, while triple-clicking
will perform line selection. If you hold <kbd>Ctrl</kbd> while expanding the
selection, it will switch to the block selection mode.

## Opening URLs with the mouse

You can open URLs with your mouse by clicking on them. The modifiers required to
be held and program which should open the URL can be setup in the configuration
file. If an application captures your mouse clicks, which is indicated by a
change in mouse cursor shape, you're required to hold <kbd>Shift</kbd> to bypass
that.

## Tabs and Panes

A TankTerm window holds tabs, and every tab can be split into panes that each
run their own terminal, like in Tabby and WezTerm.

New tabs are opened with <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>T</kbd>
(<kbd>Command</kbd> <kbd>T</kbd> on macOS) or the `+` button of the tab bar.
<kbd>Ctrl</kbd> <kbd>Tab</kbd> and <kbd>Ctrl</kbd> <kbd>Shift</kbd>
<kbd>Tab</kbd> switch between tabs and <kbd>Alt</kbd> <kbd>1</kbd> to
<kbd>9</kbd> select a tab by its position. The tab bar is only shown while there
is more than one tab; the `[tabs]` section of the configuration file changes
that and can move it to the bottom of the window.

<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>S</kbd> splits the current pane and
opens a terminal on its right, <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>D</kbd>
opens one below it. Focus moves between panes with <kbd>Ctrl</kbd>
<kbd>Alt</kbd> and the arrow keys or with a click, and the focused pane of a
split tab is framed. Dividers are dragged with the mouse or moved with
<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Alt</kbd> and the arrow keys.
<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Z</kbd> zooms the current pane to fill
the tab until it is pressed again.

New tabs and panes start in the directory the shell of the current pane is in,
unless `inherit_working_directory = false` is set in the `[tabs]` section. A
shell with [shell integration](#shell-integration) reports its directory on
every platform; without it TankTerm only finds the directory on Linux. With `restore = true` there, TankTerm opens the tabs and panes of the
last window again when it starts, in the directories their shells were in; the
shells themselves are started fresh.

A pane closes when its shell exits, a tab closes with its last pane and the
window closes with its last tab.

The gear at the right end of the tab bar opens the configuration file in a new
tab, in `$VISUAL` or `$EDITOR`, and creates the file if there is none yet.
Saving it applies the changes. The command palette has it as "Open Config".

### Key presets

The keys above are those of the default `"Tabby"` preset. `preset` in the
`[keyboard]` section of the configuration file selects another set of default
keys for tabs and panes:

- `"Tabby"`: <kbd>Alt</kbd> and a digit selects a tab, <kbd>Ctrl</kbd>
  <kbd>Alt</kbd> and an arrow key moves between panes.
- `"WezTerm"`: everything is on <kbd>Ctrl</kbd> <kbd>Shift</kbd>, with a digit
  to select a tab and with an arrow key to move between panes.
- `"Leader"`: like tmux, only opening a tab and the two splits have keys of
  their own. Everything else is the leader key, <kbd>Ctrl</kbd> <kbd>A</kbd>,
  followed by a second key: <kbd>C</kbd> opens a tab, <kbd>N</kbd> and
  <kbd>P</kbd> switch tabs, a digit selects one, the arrow keys move between
  panes, <kbd>X</kbd> closes the pane and <kbd>Z</kbd> zooms it. Pressing the
  leader key twice sends it to the shell.

The leader key is set with `leader = { key = "B", mods = "Control" }`, and any
key binding with `mode = "Leader"` is a key pressed after it, whatever the
preset. All tab and pane keys are regular key bindings that can be replaced or
unset; see the actions in the [tankTerm(5) manpage] and the keys of every
preset in the [tankTerm-bindings(5) manpage].

### Choosing panes and tabs

`PaneSelect` puts a letter over every pane of the tab; typing it focuses that
pane, and `PaneSelectSwap` swaps the current pane with it instead.
`RotatePanesClockwise` and `RotatePanesCounterClockwise` move every pane to the
next or previous place. `ShowTabNavigator` lists the tabs to pick one from, and
`RenameTab` gives the current tab a title of its own. With the `"Leader"` preset
these are the leader key followed by <kbd>Q</kbd>, <kbd>W</kbd> and
<kbd>Ctrl</kbd> <kbd>O</kbd>, like in tmux; all of them are in the command
palette as well.

### Key tables

Key tables, like WezTerm's, are named sets of key bindings that a key turns on
for a while: for one key, for some time, or until they are left. This makes a
mode for resizing panes with the arrow keys:

```toml
[keyboard]
preset = "Leader"
bindings = [{ key = "R", mode = "Leader", table = "resize" }]

[keyboard.tables.resize]
one_shot = false
bindings = [
  { key = "ArrowLeft", action = "ResizePaneLeft" },
  { key = "ArrowRight", action = "ResizePaneRight" },
  { key = "ArrowUp", action = "ResizePaneUp" },
  { key = "ArrowDown", action = "ResizePaneDown" },
  { key = "Escape", action = "PopKeyTable" },
]
```

The name of the active table is shown at the end of the tab bar, in the
`KeyTable` status segment. Keys that
aren't bound in a table keep working as usual, unless the table sets
`prevent_fallback = true`.

### Activity and finished commands

Tabs that aren't shown are marked with `*` in the tab bar when they print
something. A terminal that rings the bell or finishes a command while it isn't
looked at marks its tab with `!` and gets a bright frame, and the desktop is
asked to draw attention to the window if that doesn't have focus. A shell with
[shell integration](#shell-integration) says when its commands finish. Without
it, TankTerm watches which process has the terminal, which works on Unix with
shells that have job control.

### Tab titles

`title` in the `[tabs]` section sets what each tab shows, with placeholders in
braces: `{index}`, `{title}`, `{panes}`, `{cwd}` (the last part of the
directory), `{path}`, `{process}` (the program that has the terminal),
`{marker}` (` !` or ` *`) and `{zoom}` (` Z` while a pane is zoomed).
`{title}` is the title the pane's programs set or, while they haven't set one,
where its shell is: `user@host:~/src`, like the titles many shells set; with
`dynamic_title = false` in `[window]` it is always the configured title.
A tab's close button shows while the mouse is over it; `close_button` in
`[tabs]` set to `"Always"` shows it on every tab, and `"Never"` leaves it out.
The
default is `"{index}: {title}{zoom}{marker}"`; this shows the program and the
directory instead:

```toml
[tabs]
title = "{index} {process} in {cwd}{zoom}{marker}"
```

The program is only known on Linux, and the directory on Linux and with
[shell integration](#shell-integration).

### Status segments

The ends of the tab bar hold status segments, listed as `[[tabs.status]]`
tables. Each shows one thing in a format of its own:

- `KeyTable`: the active key table.
- `Clock`: the time, with a strftime format.
- `Cwd`: the directory of the focused pane.
- `Battery`: the battery charge.
- `Zoom`: its format, while a pane is zoomed.
- `Count`: `{tab}`, `{tabs}` and `{panes}`.
- `Text`: its format, always, for separators and labels.
- `Host`: `{user}`, `{host}` and `{fqdn}`.
- `Process`: the program running in the focused pane, while a command runs.
- `Load`: the load average, `{load1}`, `{load5}` and `{load15}`.
- `Command`: the first line a program prints, `{output}`.

Formats use placeholders in braces. A segment with nothing to show, or an empty
text, isn't drawn. Segments go after the tabs unless they say
`side = "Left"`, and every segment can have a `style` of its own on top of
`[style.tab_status]`:

```toml
[[tabs.status]]
segment = "Host"
side = "Left"

[[tabs.status]]
segment = "KeyTable"

[[tabs.status]]
segment = "Cwd"
format = "{basename}"

[[tabs.status]]
segment = "Battery"
format = "{percent}% {status}"

[[tabs.status]]
segment = "Clock"
format = "%a %H:%M"
style = { foreground = "#000000", background = "#00ff00" }
```

Segments touch, so `Text` segments can join them. With a font that has
powerline arrows, this draws one between the directory and the clock:

```toml
[[tabs.status]]
segment = "Cwd"
style = { foreground = "#000000", background = "#f4d35e" }

[[tabs.status]]
segment = "Text"
format = "\ue0b2"
style = { foreground = "#00ff00", background = "#f4d35e" }

[[tabs.status]]
segment = "Clock"
style = { foreground = "#000000", background = "#00ff00" }
```

A `Command` segment runs its `command` every `interval` seconds (5 by
default), in the directory of the focused pane and with `TANKTERM_PANE_ID` set
to that pane, and again when another pane gets the focus. The program runs
without a shell, so a pipeline needs one:

```toml
[[tabs.status]]
segment = "Command"
command = { program = "sh", args = ["-c", "git branch --show-current"] }
interval = 10
```

A program that fails, or runs longer than its interval or 10 seconds, hides
the segment and is logged once. Commands come from the configuration file and
`-o` only.

Without a `[[tabs.status]]` table, only the key table is shown. The battery and
the program are only known on Linux, the directory on Linux and with
[shell integration](#shell-integration), and the load on Unix.

### Tab bar styles

The tab bar is styled with `[style.tab_*]` blocks, which take the same Lip Gloss
properties as the other `[style]` blocks, plus Lip Gloss's box properties:
`padding`, `margin`, `border`, `border_foreground`, `width` and `max_width`:

- `tab_bar`: the bar itself.
- `tab_active` and `tab_inactive`: the shown tab and the others.
- `tab_button`: the `+` button, the gear and the window buttons.
- `tab_close_hovered`: changes to the window's close button under the mouse,
  over `tab_hovered`. It is white on red unless set.
- `tab_hovered`, `tab_activity` and `tab_attention`: changes to a tab under the
  mouse, a tab marked with `*` and a tab marked with `!`.
- `tab_status`: the status segments.

Like the other styles, they can `inherit` from `[style.defs]`, and
`[style.blurred.tab_*]` blocks change them while the window doesn't have focus.
This draws the tabs as rounded pills and the shown one in green:

```toml
[style.defs.pill]
border = "Rounded"
padding = [2, 12]
margin = [0, 2]

[style.tab_active]
inherit = "pill"
foreground = "#000000"
background = "#00ff00"
bold = true

[style.tab_inactive]
inherit = "pill"
border_foreground = "#3a3a4a"

[style.tab_hovered]
border_foreground = "#00ff00"
```

Borders are named after Lip Gloss's: `Normal`, `Rounded`, `Thick`, `Double`,
`Block` and `ASCII`, drawn in pixels as a thin line, a thin line with rounded
corners, a 2 pixel line, two thin lines, a solid band and a dashed line. As in
Lip Gloss, `border_top`, `border_right`, `border_bottom` and `border_left`
draw single sides, and `border_top_foreground` and so on color them. Tabs are
`width` cells wide, or else as wide as their titles (`width = "Shared"` in
`[tabs]` has them share the bar evenly instead), up to `max_width` cells, and `gap` in
`[style.tab_bar]` is the space between them in pixels. This draws a green line
under the shown tab, and makes it 24 cells wide:

```toml
[style.tab_bar]
gap = 0

[style.tab_active]
border = "Thick"
border_bottom = true
border_foreground = "#00ff00"
width = 24

[style.tab_inactive]
max_width = 16
```

### The tab bar as title bar

With `decorations = "None"` in the `[window]` section the window has no title
bar of its own, and the tab bar takes its place. It is shown even with a single
tab and has minimize, maximize and close buttons where the desktop puts them:
on KDE as KWin's button layout says, on GNOME as its `button-layout` setting
says, on the left on macOS and on the right elsewhere. `buttons = "Right"` and
`buttons = "Left"` put them at one end instead.

Dragging the bar where there is no tab or button moves the window,
double-clicking it there maximizes the window, and right-clicking it opens the
window menu. The edges and corners of the window resize it, unless it is
maximized; the maximize button then shows a restore icon. `buttons = "None"`
leaves the tab bar as it is, for window managers that move windows on their
own.

```toml
[window]
decorations = "None"
buttons = "Desktop"
```

The buttons are drawn with the `tab_button` and `tab_hovered` styles, and the
close button under the mouse also with `tab_close_hovered`.

[tankTerm(5) manpage]: ../extra/man/tankTerm.5.scd
[tankTerm-bindings(5) manpage]: ../extra/man/tankTerm-bindings.5.scd

## Shell Integration

Shells can tell TankTerm where their prompts, command lines and command output
are (OSC 133) and which directory they are in (OSC 7). On Linux, BSD and macOS,
TankTerm starts bash, zsh and fish with scripts that do this, the way Ghostty
does; the shells still read their own startup files. Shells started to run a
command or a script, or told to read no startup files, are left alone, and
`shell_integration = "None"` in the `[terminal]` section turns it off.

Other shells, such as one started from another shell or over SSH, can source
the script by hand at the end of their startup file. TankTerm writes the
scripts to the directory in `$TANKTERM_SHELL_INTEGRATION_DIR`; they are also in
the `extra/shell-integration` folder:

```sh
source /path/to/tankterm.bash   # in ~/.bashrc
source /path/to/tankterm.zsh    # in ~/.zshrc
source /path/to/tankterm.fish   # in ~/.config/fish/config.fish
```

Shells that send these sequences themselves need nothing more. With them:

- A mark at the left edge shows where every prompt starts. It turns red when
  the command typed at that prompt fails.
- `ScrollToPreviousPrompt` and `ScrollToNextPrompt` scroll the next prompt up
  or down to the top of the terminal.
- `SelectCommandOutput` selects the output of the command whose prompt is at
  the top of the terminal, or of the last command while the terminal is
  scrolled to the bottom, and puts it on the selection clipboard.
- Finished commands are noticed on every platform, and new tabs and panes start
  in the shell's directory, also in nested shells.

The three actions are in the command palette and have keys in every preset:

| Preset | Previous / next prompt | Select output |
|---|---|---|
| Tabby and Leader | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>↑</kbd> / <kbd>↓</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>G</kbd> |
| WezTerm | <kbd>Shift</kbd> <kbd>↑</kbd> / <kbd>↓</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>G</kbd> |
| macOS, every preset | <kbd>Cmd</kbd> <kbd>↑</kbd> / <kbd>↓</kbd> | <kbd>Cmd</kbd> <kbd>Shift</kbd> <kbd>A</kbd> |

Like <kbd>Shift</kbd> <kbd>PageUp</kbd>, the arrows reach the application on
the alternate screen, such as an editor. To hide the marks at the left edge,
set `prompt_marks = false` in the `[terminal]` section.

With the mouse:

- <kbd>Alt</kbd> and a right click select the output of the command under the
  mouse, or of the command typed at the prompt under it.
- A click in the command line moves the shell's cursor there, while the shell
  waits at its prompt. TankTerm sends a mouse click to shells that ask for one
  (OSC 133 `click_events`, like fish 4.1 and later) and arrow keys to shells
  that take those (`cl`, like bash and zsh with TankTerm's scripts).
  `click_to_move = false` in the `[mouse]` section turns this off.

## Copying and Pasting

Selected text is put on the selection clipboard, where the platform has one,
and with `save_to_clipboard = true` in the `[selection]` section also on the
regular clipboard. <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>C</kbd> copies the
selected text to the clipboard. The middle mouse button and <kbd>Shift</kbd>
<kbd>Insert</kbd> paste the selection, <kbd>Ctrl</kbd> <kbd>Shift</kbd>
<kbd>V</kbd> pastes the clipboard. The keys are key bindings to the `Copy`,
`PasteSelection` and `Paste` actions and can be changed in the `[keyboard]`
section of the configuration file. To paste with the right mouse button, bind
it:

```toml
[mouse]
bindings = [{ mouse = "Right", action = "Paste" }]
```

Before text with line breaks is pasted into a program that didn't turn on
bracketed paste, where every line break would act as the <kbd>Enter</kbd> key,
TankTerm shows the text and asks. `confirm_multiline_paste = false` in the
`[terminal]` section turns the question off.

## Command Palette and Profiles

<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>P</kbd> (<kbd>Command</kbd>
<kbd>Shift</kbd> <kbd>P</kbd> on macOS) opens the command palette, a list of
the tab and pane actions with their key bindings. Type to filter it, move with
the arrow keys, run the selected action with <kbd>Enter</kbd> and close it with
<kbd>Escape</kbd>.

Profiles are named shells with their own command, working directory,
environment and theme, set up in `[[profiles]]` tables of the configuration
file:

```toml
[[profiles]]
name = "Build server"
shell = { program = "ssh", args = ["build.example.com"] }
theme = "dracula"
```

<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>E</kbd> (<kbd>Command</kbd>
<kbd>Shift</kbd> <kbd>E</kbd> on macOS) opens the profile selector, which works
like the command palette and opens the chosen profile in a new tab.

## SSH

`tankTerm ssh` runs `ssh` with a `TERM` the server has a terminfo entry for. A
server without an entry for `TERM` can't run full-screen programs properly, and
minimal Debian and Ubuntu installs lack `alacritty`, the entry TankTerm uses.
The arguments go to `ssh` unchanged:

```sh
tankTerm ssh -p 2222 build.lan
```

Hosts matching the `probe` patterns of the `[ssh]` table are checked once, over
the connection the session then uses, for the entries in `order`; the first
one the server has becomes `TERM`. Other hosts get `fallback`:

```toml
[ssh]
probe = ["*.lan", "build-*", "pi@10.0.0.*", "!nas.lan"]
order = ["alacritty", "xterm-256color", "xterm"]  # the default
fallback = "xterm-256color"                       # the default
```

Patterns match the host as typed or the host name `ssh -G` resolves it to,
and `user@host` when they contain `@`. The answer is cached per user, host and
port in `$XDG_STATE_HOME/tankterm/ssh-terms.json`. `--reprobe` checks a host
again, and `--probe` and `--no-probe` override the patterns for one connection.
The check only tests which terminfo files the server has and never installs
anything; on a Debian or Ubuntu server without `alacritty` it suggests
installing `ncurses-term`. A profile gets the same with
`shell = { program = "tankTerm", args = ["ssh", "build.lan"] }`. Not available
on Windows; see `man tankTerm-ssh`.

## Scripting

`tankTerm msg` controls a running TankTerm from scripts and other programs,
with the commands and options of `wezterm cli`: `list` the panes, `spawn` a
tab, `split-pane`, `send-text` to a pane or `get-text` from it, `activate-pane`,
`activate-tab`, `kill-pane`, `zoom-pane` and `set-tab-title`. Shells find the
ID of their pane in `$TANKTERM_PANE_ID`, which the commands use by default:

```sh
# Run the tests in a pane below this one and read what they printed.
pane=$(tankTerm msg split-pane --percent 30 -- sh -c 'cargo test; exec cat')
tankTerm msg get-text --pane-id "$pane"
```

See `man tankTerm-msg` for all options.

## Coming from another terminal

`tankTerm migrate` converts the configuration of Alacritty, Tabby, WezTerm or
Ghostty, or Crush's theme, into TankTerm's: colors, fonts, cursor, bell,
opacity, window settings, profiles, and the key bindings whose actions TankTerm
has. For Tabby and WezTerm it selects the matching key preset and writes only
the keys that differ from it. WezTerm's Lua is read without running it, so
anything computed or conditional is skipped with its line.

The source is never changed. The result goes to TankTerm's configuration file,
and a summary lists every key that was converted or skipped, and why. Existing
TankTerm files are replaced only after a yes, or with `--force`, and are backed
up to `<name>.bak` first.

```sh
# See what a WezTerm configuration becomes, without writing anything.
tankTerm migrate --from wezterm --dry-run
```

See `man tankTerm` for the sources and their options.

## Windows

A TankTerm process has one window, with tabs and split panes in it. New windows
aren't supported yet: bindings to `CreateNewWindow` and `SpawnNewInstance` are
logged as doing nothing, and `tankTerm msg` has no `create-window`. To open
another window, start `tankTerm` again.
