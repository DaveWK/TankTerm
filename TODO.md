# TODO

## Planned

Approved and waiting their turn, in this order.

1. Delete the `xitely/iced_term` repository on Forgejo (web UI; `forgejo-repos` can't).
2. Config options the iced front end ignores:
   - Port later, low priority: `window.resize_increments` and `window.option_as_alt`.
3. Vi mode, scrollback search and hints in the iced front end.
4. Tab bar: tab title templates, status segments, and window buttons in the tab bar in place of
   the OS title bar. The `[style.tab_*]` blocks are done.
5. `tankTerm ssh` with terminfo negotiation; design and decisions are in the section below
   the deferred list.

## Deferred

Nothing below is started; each item lists what has to be decided or built first.

### Rest of `wezterm cli` in `tankTerm msg`

`get-text --escapes`, `split-pane --cells`, `--top-level` and `--move-pane-id`,
`activate-pane-direction`, `get-pane-direction`, `adjust-pane-size` and `move-pane-to-new-tab`;
`create-window`, `config` and `get-config` once there is more than one window.

### Rest of shell integration

- Injecting the integration into bash, zsh and fish when TankTerm starts them, like Ghostty and
  kitty, instead of having users source the scripts in `extra/shell-integration`. Decide how to
  reach each shell's startup (`ENV`/`--rcfile`, `ZDOTDIR`, `XDG_DATA_DIRS`) and how to turn it off.
- Default keys for `ScrollToPreviousPrompt`, `ScrollToNextPrompt` and `SelectCommandOutput`, and
  an option to hide the prompt marks.
- Selecting a command's output with the mouse, and clicking in a command line to move the cursor
  there (OSC 133 `click_events`).
- The fish script marks the prompt on the `fish_prompt` event only; a prompt fish erases and
  draws again, such as on a resize, loses its mark until the next prompt. Wrapping `fish_prompt`
  would fix that, if it keeps `$status` and `$pipestatus` for the prompt.

### Rest of WezTerm's key tables and pane select

Options given where a table is activated (`replace_current`, and `one_shot` and `timeout` per
binding instead of per table), `PaneSelect` modes that move the chosen pane to a new tab or
window, and a configurable label alphabet.

### Rest of Lip Gloss's box properties in the tab bar

Borders on single sides (`border_top` and so on, for a line under the shown tab), the `Double`,
`Block` and `ASCII` borders, `width`/`max_width` per tab style in place of the fixed 240 pixels,
and the space between tabs. iced's `Border` has one width for all sides, so single sides need
widgets of their own.

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

## Terminfo negotiation over SSH ("greedyterm")

Planned item 8. Send a server only a `TERM` value it actually has. Assessed against Fedora 45,
Rocky 10.2, Debian 13 and 11, and Ubuntu 24.04.

Recommended design: a `tankTerm ssh` wrapper that execs the real `ssh`.

- Probing is opt-in per host or profile. Hosts that have not opted in get `xterm-256color`.
- First connection to an opted-in host: one fixed probe over a shared SSH connection
  (ControlMaster) that tests which terminfo files exist in the system directories. The answer is
  cached per resolved user, host and port until `--reprobe` is given; later connections add no
  latency and run nothing on the server.
- Order: `alacritty`, `xterm-256color`, `xterm`; `xterm-256color` when the probe fails. The list
  is configurable. TankTerm keeps the `alacritty` names.
- It never installs an entry on a server. On Debian and Ubuntu hosts without one it prints a
  hint to install `ncurses-term`.
- The probe tests files instead of calling `infocmp`, which is missing from Fedora and Rocky
  container images. It ignores `~/.terminfo` so the answer still holds after `sudo`.
- Covered: profiles that use it as their shell, and `tankTerm ssh` typed by hand. Plain `ssh` is
  not covered.
- Size: medium. Unix only at first.

Findings that shaped it:

- `alacritty` is always present on Fedora and Rocky. On Debian and Ubuntu it comes with
  `ncurses-term`, which `openssh-server` pulls in by default but minimal and container installs
  lack. `INSTALL.md` currently says the entry ships everywhere, which is wrong for those.
- `alacritty-direct` and `xterm-direct` are left out of the default order: outside tmux they
  turn colours 8 and above into RGB values, so `tput setaf 9` comes out nearly black.
- Installing the entry on the server lost: root cannot see it after `sudo -i`, it goes stale,
  and Fedora and Rocky images have no `tic`.
- Answering XTGETTCAP does not remove the need, since an unknown `TERM` fails before any query,
  but it would give Neovim true colour on servers that drop `COLORTERM`. TankTerm answers no
  DCS queries today. Worth its own item.
- `extra/tankterm.info` is behind ncurses 6.6 in several capabilities; refreshing it is a small
  separate job.
