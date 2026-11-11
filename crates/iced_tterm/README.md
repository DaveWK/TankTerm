# iced_tterm

The terminal emulator widget TankTerm is built on: it renders a `tankterm_terminal` terminal in an
[iced](https://github.com/iced-rs/iced) application and handles its keyboard, mouse, selection,
scrolling and hyperlinks.

`iced_tterm` is a fork of [iced_term](https://github.com/Harzu/iced_term) by Ilia Shvyrialkin,
released under the MIT license (see [LICENSE](./LICENSE)). It was developed in a separate
repository until it was moved into the TankTerm workspace; the history up to that point is in
[xitely/iced_term](https://git.infra.xitely.net/xitely/iced_term). The upstream examples and
screenshots were not carried over.

## Overview

An application owns one `Terminal` per terminal and shows it with `TerminalView::show`.

- **Commands** change the terminal: `Command::ChangeTheme`, `Command::ChangeFont`,
  `Command::AddBindings` and `Command::ProxyToBackend`, passed to `Terminal::handle`.
- **Events** come from the widget and the terminal's subscription as
  `Event::BackendCall(id, command)`, where `id` is the terminal's ID. The application passes them
  back with `Command::ProxyToBackend`.
- **Actions** are what `Terminal::handle` returns for the application to act on:
  `Action::Shutdown` when the shell exits and `Action::ChangeTitle` when the title changes.

TankTerm's `crates/tankterm/src/app/` is the reference for using the widget with tabs and split
panes.

The widget's API is not stable.
