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

### Selection

One useful feature of vi mode is the ability to make selections and copy text to
the clipboard. By default you can start a selection using <kbd>v</kbd> and copy
it using <kbd>y</kbd>. All selection modes that are available with the mouse can
be accessed from vi mode, including the semantic (<kbd>Alt</kbd> <kbd>v</kbd>),
line (<kbd>Shift</kbd> <kbd>v</kbd>) and block selection (<kbd>Ctrl</kbd>
<kbd>v</kbd>). You can also toggle between them while the selection is still
active.

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

During normal search you don't have the opportunity to move around freely, but
you can still jump between matches using <kbd>Enter</kbd> and <kbd>Shift</kbd>
<kbd>Enter</kbd>. After leaving search with <kbd>Escape</kbd> your active match
stays selected, allowing you to easily copy it.

## Hints

Terminal hints allow easily interacting with visible text without having to
start vi mode. They consist of a regex that detects these text elements and then
either feeds them to an external application or triggers one of TankTerm's
built-in actions.

Hints can also be triggered using the mouse or vi mode cursor. If a hint is
enabled for mouse interaction and recognized as such, it will be underlined when
the mouse or vi mode cursor is on top of it. Using the left mouse button or
<kbd>Enter</kbd> key in vi mode will then trigger the hint.

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
`{marker}` (` !` or ` *`) and `{zoom}` (` Z` while a pane is zoomed). The
default is `"{index}: {title}{zoom}{marker}"`; this shows the program and the
directory instead:

```toml
[tabs]
title = "{index} {process} in {cwd}{zoom}{marker}"
```

The directory and the program are only known on Linux.

### Status segments

The right end of the tab bar holds status segments, listed as `[[tabs.status]]`
tables. Each shows one thing in a format of its own: the active key table
(`KeyTable`), the time (`Clock`, with a strftime format), the directory of the
focused pane (`Cwd`) or the battery charge (`Battery`). Formats use
placeholders in braces, and every segment can have a `style` of its own on top
of `[style.tab_status]`:

```toml
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

Without a `[[tabs.status]]` table, only the key table is shown. The directory
and the battery are only known on Linux.

### Tab bar styles

The tab bar is styled with `[style.tab_*]` blocks, which take the same Lip Gloss
properties as the other `[style]` blocks, plus `padding`, `margin`, `border`
and `border_foreground`:

- `tab_bar`: the bar itself.
- `tab_active` and `tab_inactive`: the shown tab and the others.
- `tab_button`: the `+` button and the gear.
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

[tankTerm(5) manpage]: ../extra/man/tankTerm.5.scd
[tankTerm-bindings(5) manpage]: ../extra/man/tankTerm-bindings.5.scd

## Shell Integration

Shells can tell TankTerm where their prompts, command lines and command output
are (OSC 133) and which directory they are in (OSC 7). The `extra/shell-integration`
folder has scripts that do this for bash, zsh and fish; source the one for your
shell at the end of its startup file, after anything else that sets the prompt:

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

The three actions are in the command palette and have no keys by default; bind
them in the `[keyboard]` section:

```toml
[keyboard]
bindings = [
  { key = "ArrowUp", mods = "Shift", action = "ScrollToPreviousPrompt" },
  { key = "ArrowDown", mods = "Shift", action = "ScrollToNextPrompt" },
]
```

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

## Multi-Window

TankTerm supports running multiple terminal emulators from the same TankTerm
instance. New windows can be created either by using the `CreateNewWindow`
keybinding action, or by executing the `tankTerm msg create-window` subcommand.
