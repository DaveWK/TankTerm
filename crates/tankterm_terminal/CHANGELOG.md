# Changelog

All notable changes to tankterm_terminal are documented in this file. The
sections should follow the order `Added`, `Changed`, `Deprecated`, `Fixed` and
`Removed`.

**Breaking changes are written in bold style.**

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## 0.26.1-dev

### Added

- `Term::set_color_scheme` and `Term::report_color_scheme`, answering `CSI ? 996 n` color scheme
    queries and reporting changes to applications that enable private mode 2031
- `parser::Parser`, `vte`'s processor plus the shell integration sequences it drops: OSC 7
    (`Event::WorkingDirectory`) and OSC 133 prompt marks (`Event::CommandStarted`,
    `Event::CommandFinished`)
- `Cell::zone` records the prompt, command line or output a cell was written in;
    `Term::scroll_to_prompt` and `Term::select_command_output` use it

### Changed

- **Crate renamed from `alacritty_terminal` to `tankterm_terminal`**
- **`Event` has the shell integration events, and `Cell` the `zone` field**
- **Child processes receive `TANKTERM_WINDOW_ID` instead of `ALACRITTY_WINDOW_ID`**
- Ref test recordings are written to `tankterm.recording` instead of `alacritty.recording`
- Generated hyperlink IDs end in `_tankterm` instead of `_alacritty`

### Fixed

- `CSI 1 J` (erase above) not clearing the first line when the cursor is on the second line

## Forked from Alacritty

`tankterm_terminal` was forked from `alacritty_terminal` at commit
[`aa5d942f`](https://github.com/alacritty/alacritty/commit/aa5d942f46c40184a3f214830ff5f7989f4733a1).
