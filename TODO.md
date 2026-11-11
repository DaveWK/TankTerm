# TODO

## Planned

Approved and waiting their turn, in this order.

1. Delete the `xitely/iced_term` repository and the merged `forgejo-ci` branch on Forgejo.
2. Split `crates/tankterm/src/app.rs` into a module.
3. Config options the iced front end ignores:
   - Remove the OpenGL-only ones: `debug.renderer`, `debug.prefer_egl`, `debug.render_timer`,
     `debug.highlight_damage`, `window.embed` / `--embed`, `--print-events`, `--ref-test`.
   - Port first: window basics (`dimensions`, `position`, `startup_mode`, `decorations`,
     `opacity`, `level`), key binding actions (copy, paste, font size, scrolling, clear history,
     fullscreen), scrolling, cursor and bell options, and live config reload.
   - Port after those: font faces and offsets, the remaining colour options, selection and mouse
     options, `--hold`.
   - Port later, low priority: `window.blur`, `window.resize_increments`,
     `window.dynamic_padding`, `window.option_as_alt`.
4. `tankTerm msg` in the iced front end: spawn, split, send-text, get-text, list, activate.
5. Key tables and user-defined leader bindings in `[keyboard] bindings` (the leader key itself
   comes with the key preset option), with pane select, rotate panes, tab navigator and rename
   tab.
6. Vi mode, scrollback search and hints in the iced front end.
7. Shell integration: OSC 7 working directory and OSC 133 prompt zones.
8. Tab bar: `[style.tab_*]` blocks, tab title templates, status segments, and window buttons in
   the tab bar in place of the OS title bar.

## Deferred

Nothing below is started; each item lists what has to be decided or built first.

### Event API (planned, deferred)

Let scripts react to what happens in TankTerm, as WezTerm does with `wezterm.on`, without
embedding a scripting language. Events leave TankTerm as JSON; scripts act through
`tankTerm msg`.

Needs first:

- `tankTerm msg` working in the iced front end, with tab and pane commands and stable IDs
  (`TANKTERM_PANE_ID` in each shell's environment).
- Shell integration (OSC 7, OSC 133, OSC 1337 user variables) for the events marked `*`.

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

### Assess: negotiating the best terminfo a server has ("greedyterm")

When connecting to another machine, pick the best `TERM` value that machine actually has instead
of sending one it may not know.

- Today `TERM` is `alacritty` when that terminfo entry is installed locally and `xterm-256color`
  otherwise (`crates/tankterm_terminal/src/tty/mod.rs`). Over SSH the value is forwarded as is, so a
  server without the `alacritty` entry gets a broken session.
- Idea: try an ordered list, best first (for example `alacritty-direct`, `alacritty`,
  `xterm-256color`, `xterm`), and use the first one the server has.
- To assess:
  - Where the negotiation can run. The terminal cannot see the server's terminfo database, so it
    needs an SSH wrapper or shell integration that asks the server (`infocmp NAME`) before the
    session starts, or the built-in SSH client once that exists.
  - Whether to install the entry on the server instead (`infocmp -x | ssh HOST tic -x -`), as
    other terminals' SSH helpers do, and when that is acceptable.
  - Whether answering XTGETTCAP queries reduces the need, since applications can then ask the
    terminal for capabilities directly.
  - How this fits profiles that run `ssh`, and nested cases such as `sudo`, `su` and containers.
- Outcome: a recommendation and a size estimate. No implementation.

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
