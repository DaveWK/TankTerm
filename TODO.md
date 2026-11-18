# TODO

## Planned

Approved and waiting their turn, in this order.

1. Delete the `xitely/iced_term` repository on Forgejo (web UI; `forgejo-repos` can't).
2. Config options the iced front end ignores:
   - Port later, low priority: `window.resize_increments` and `window.option_as_alt`.

## Deferred

Nothing below is started; each item lists what has to be decided or built first.

### Actions TankTerm doesn't have yet

Bindings to these are accepted and logged as warnings, but do nothing. Several are still in
the default bindings inherited from Alacritty.

- The key help (`ToggleHelp`): the Bubbles-style list of keys the OpenGL front end showed.
- New windows (`SpawnNewInstance`, `CreateNewWindow`): TankTerm is one window with tabs. Needs
  a decision on several windows per process, which also unblocks `create-window` in
  `tankTerm msg`.
- The message bar and `ClearLogNotice`: config errors and warnings shown in the window instead
  of only in the log.
- macOS only: `HideOtherApplications` and `ToggleSimpleFullscreen`.
- Mouse bindings beyond pasting, expanding the selection and selecting a command's output: other
  actions, the left and extra buttons, and bindings limited to a mode.
- Taking the actions that stay unsupported out of the default bindings and the bindings man
  page.

### Rest of `tankTerm migrate`

- SSH profiles from Tabby and WezTerm run `ssh`; switch them to `tankTerm ssh` once it exists.
- VHS theme JSON (`name`, `background`, `foreground`, `cursor`, `selection`, `black` …
  `brightWhite`) as a source of colors, if wanted: it is Charm's only file format with a
  terminal palette. Lip Gloss and CharmTone have no theme files, only Go code.
- WezTerm's ~1000 built-in color schemes and Crush's built-in themes other than
  `charmtone-panther` are only known by name; converting them needs their colors bundled.
- Key sequences (Tabby's multi-stroke hotkeys, Ghostty's `a>b` triggers and key tables) as
  leader bindings or `[keyboard.tables]`, mouse bindings, and Tabby's per-profile color schemes
  as profile themes.

### Rest of `wezterm cli` in `tankTerm msg`

`get-text --escapes`, `split-pane --cells`, `--top-level` and `--move-pane-id`,
`activate-pane-direction`, `get-pane-direction`, `adjust-pane-size` and `move-pane-to-new-tab`;
`create-window`, `config` and `get-config` once there is more than one window.

### Rest of shell integration

- `ssh` and `sudo` helpers like Ghostty's and kitty's: carrying the integration (and the
  terminfo) to the other side of `ssh`, and keeping it in `sudo -s`. Decide whether they wrap the
  commands in the scripts or come with `tankTerm ssh` (planned item 3).
- A list of the integration's features to turn on and off one by one, in place of
  `terminal.shell_integration = "Auto"`/`"None"`.

### Rest of WezTerm's key tables and pane select

Options given where a table is activated (`replace_current`, and `one_shot` and `timeout` per
binding instead of per table), `PaneSelect` modes that move the chosen pane to a new tab or
window, and a configurable label alphabet.

### Lip Gloss's box properties outside the tabs

`width` and `max_width` for the buttons and status segments, `gap` between the status segments
and buttons, the `border_*_background` colors, and Lip Gloss's half block borders.

### More status segments

- Powerline arrows drawn by TankTerm, so they don't need a font that has them.
- The remote host from OSC 7 in the `Host` segment, for a shell on another computer.
- `Process` on macOS (`proc_name`) and Windows; the program is read from `/proc` on Linux only.
  The battery is Linux only too.

### Event API (planned, deferred)

Let scripts react to what happens in TankTerm, as WezTerm does with `wezterm.on`, without
embedding a scripting language. Events leave TankTerm as JSON; scripts act through
`tankTerm msg`.

Needs first:

- Done: `tankTerm msg` with tab and pane commands and stable IDs (`TANKTERM_PANE_ID` in each
  shell's environment).
- Done: shell integration with OSC 7 and OSC 133, which `tankterm_terminal` reports as
  `Event::WorkingDirectory`, `Event::CommandStarted` and `Event::CommandFinished`, for
  `cwd-changed` and `command-finished`.
- OSC 1337 user variables, for `user-var-changed`.

Plan:

1. Event model: versioned JSON payloads for `window-focus-changed`, `window-resized`,
   `tab-opened`, `tab-closed`, `tab-activated`, `pane-opened`, `pane-closed`, `pane-focused`,
   `title-changed`, `bell`, `config-reloaded`, `cwd-changed`\*, `command-finished`\*,
   `user-var-changed`\*.
2. `tankTerm msg subscribe [--event NAME]...` prints one JSON object per event, for long-running
   scripts in any language.
3. `[[hooks]]` in `tankterm.toml`: `event = "bell"`, `command = { program = "...", args = [...] }`,
   run once per event with the payload on stdin and in `TANKTERM_EVENT_*` variables.
4. Custom events: an `EmitEvent` binding action and `tankTerm msg emit NAME`, so a key can
   trigger a script.
5. Only if a use for it turns up: hooks TankTerm waits on, with a timeout (for example to decide
   whether closing a pane needs confirmation).

To decide when this is picked up:

- Whether any hook may block TankTerm, or all are fire-and-forget.
- Hooks come from the config file only, never from escape sequences sent by programs.
- How the event stream works on Windows, where `tankTerm msg` has no socket.

Not part of this: WezTerm's `format-tab-title` and `update-status` callbacks. Those need an answer
before the next frame, so they are better served by title templates and declarative status
segments in the config.

### Mux server with attach and detach

Keep terminals running in a background process so a window can be closed and reattached later,
like WezTerm's multiplexer domains or tmux.

- Terminal state (grid, scrollback, PTY) currently lives inside the window's process, in the
  `iced_tterm` widget. It would have to move into a server process, with the window as a client
  that receives screen updates and sends input.
- Decide the scope first: local sessions only (unix socket), or also remote sessions over SSH.
- Depends on stable pane IDs and the `tankTerm msg` protocol, which the server would extend.

### Research spike: ligatures and font shaping

Find out what it would take to draw ligatures and shape text across cells.

- `iced_tterm` draws every cell's character on its own, so neighbouring characters are never
  shaped together and programming ligatures cannot form.
- Questions: can runs of cells with the same style be shaped as one string and still land on the
  grid; what that does to performance when the screen changes quickly; how wide characters,
  combining characters and the cursor behave inside a ligature; whether iced's text stack
  (cosmic-text) exposes what is needed or the widget needs its own glyph cache.
- Outcome: a short write-up with a recommendation, not an implementation.

### Research spike: image protocols

Find the cheapest way to show images in the terminal: sixel, the kitty graphics protocol or
iTerm2 inline images.

- A cell in `tankterm_terminal` holds a character, colours, flags, an underline colour and a
  hyperlink. Images need their own store, anchored to grid rows so they scroll, clear and resize
  with the text.
- The escape parser (the external `vte` crate) does not decode any of the three protocols.
- `iced_tterm` would have to draw textures between the cell backgrounds and the text.
- Outcome: which protocol to support first, what each of the three layers needs, and a size
  estimate. No implementation.

### Ref test recorder

The 45 ref tests in `crates/tankterm_terminal/tests/ref` still run, but new ones cannot be
recorded: the `--ref-test` flag that wrote the recordings went away with the OpenGL front end.

- A recording is the raw PTY output (`tankterm.recording`) plus the resulting grid, terminal size
  and config (`grid.json`, `size.json`, `config.json`).
- Options: a hidden flag in the iced front end that records a session the same way, or a small
  headless tool in `tankterm_terminal` that runs a command in a PTY at a given size and writes
  the four files, with no window involved.
- Update the ref test section of `CONTRIBUTING.md` when this exists.

### Rest of terminfo negotiation over SSH

`tankTerm ssh` covers profiles and commands typed by hand, on Unix.

- Plain `ssh` typed in a pane isn't covered; that would need a shell alias or function that the
  shell integration scripts set up.
- Windows: `tankTerm ssh` needs `exec` replaced by running `ssh` as a child, and the cache would
  go in `%LOCALAPPDATA%\tankterm\`.
- Answering XTGETTCAP (DCS `+q`) doesn't remove the need, since an unknown `TERM` fails before
  any query, but it would give Neovim true colour on servers that drop `COLORTERM`. TankTerm
  answers no DCS queries today.
- `extra/tankterm.info` is behind ncurses 6.6 in several capabilities; refreshing it is a small
  separate job.
- With `ControlMaster yes` or `ask` in `~/.ssh/config`, the session doesn't attach to the probe's
  connection and authenticates a second time on the first connection.

### Built-in SSH, serial and vault (requirements open)

Tabby and WezTerm both have built-in SSH and serial clients; Tabby also has an encrypted store for
secrets. Until requirements are settled, SSH and serial sessions are profiles that run `ssh` or a
serial tool.

Requirements to capture:

- What `ssh` in a pane does not do today that a built-in client should (connection manager, jump
  hosts, port forwarding UI, reconnect, SFTP, agent handling).
- Whether WezTerm's Rust crates (`wezterm-ssh`, `portable-pty` for serial ports) are acceptable
  dependencies; check their license first.
- What the vault stores (passwords, key passphrases, whole config), what unlocks it, and whether
  the system keyring is enough instead.
- Serial: which devices and settings matter (baud rate, flow control, hex view, reconnect).
