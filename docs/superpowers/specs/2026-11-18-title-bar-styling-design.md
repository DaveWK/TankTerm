# Title bar styling — design

Date: 2026-11-18. Status: approved by Dave.

## Intent

Dave wants TankTerm's top to be one bar: the tabs, a new-tab button, settings and the window
buttons together, with slimmer, lighter tabs. TankTerm is Dave's fork and this is the look he
uses, so it becomes **the default look**, not an opt-in.

What TankTerm shows today with an empty config:

- KDE's own title bar ("TankTerm", KDE's buttons), then a separate tab bar under it, because
  `window.decorations` defaults to `Full`.
- Tabs that share the bar up to 240 px each (`MAX_TAB_WIDTH`), labelled `1: title`, a `×` on
  every tab at all times, the shown tab marked by a filled background.
- A small font `+`, and a font gear (`⚙`) at the far right end.

The target look:

```
┃1  davewk@mainbox:~ ┃  +                              ⚙     –   ▢   ✕
 ‾‾ 2 px green line along the top of the shown tab; no × until the mouse is over a tab
```

TankTerm's version keeps its own icon at the left end, where KDE draws it today:

```
▣ ┃1  davewk@mainbox:~ ┃  +                            ⚙     –   ▢   ✕
```

- One bar, about 36 px high at a 14 px font, on the terminal's own background (it is as
  translucent as the terminal).
- The shown tab has a 2 px accent line along its top edge and otherwise blends into the bar.
- The tab's number is drawn dimmer than its title, with space and no colon between them.
- A tab is as wide as its title plus padding, not a share of the bar.
- The `×` of a tab only shows while the mouse is over that tab.
- Thin line icons for `+`, the gear and minimize, maximize and close, all the same weight. The gear
  sits a little before the window buttons, not at the far edge.

## Goals

1. With an empty `tankterm.toml`, TankTerm opens with no system title bar. The tab bar is the title
   bar, and it is shown even with one tab, as `decorations = "None"` already does.
2. With an empty `tankterm.toml`, the tab bar looks as described in the list above,
   and keeps TankTerm's icon at the top left, where KDE puts it today.
3. A new tab is named after where its shell is (`davewk@mainbox:~`) instead of
   "TankTerm".
4. Each new behaviour can be configured. Anyone who wants the old look can get it back with a few
   lines of config, and `tankTerm(5)` says how.

## Non-goals

- A profiles and connections button next to `+`. TankTerm has nothing behind it. *(My call: left out. It could open the command palette later.)*
- A settings tab. The gear keeps opening the config file, as it does today.
- Changing the font. Tabs keep using the terminal font.
- Tab drag-reordering, tab animations, and anything else beyond how the bar looks and
  how the close button appears.

## Anti-goals

- A second look kept alive in code beside the new one. The old look is reachable through
  ordinary configuration (`decorations`, `tabs.title`, `[style.tab_*]` and the new options), not
  a `look = "Classic"` switch.
- Breaking the existing `[style.tab_*]` system. Everything new is a new *default* for those
  blocks or a new option, and user style blocks still override it.
- Dragging or resizing the window getting worse. The title-bar behaviour from `4f8a0eac`
  (drag to move, double-click to maximize, right-click for the window menu, edge resizing) stays
  exactly as it is.

## Constraints

- iced 0.14. `canvas` is enabled (through `iced_tterm`'s features), so icons can be drawn as
  vectors with no new dependencies.
- GUI changes are tested headlessly with `iced_test` snapshots and the simulator, never by
  screenshotting or injecting input on the desktop. Existing render snapshots will change, and
  they get updated deliberately and reviewed, not regenerated blindly.
- Hard fork of Alacritty. Work happens on a branch that is merged into `main` locally. Nothing is
  pushed without Dave's explicit yes.
- Docs to update: `docs/features.md` (tab bar and title bar sections), `extra/man/tankTerm.5.scd`
  and `CHANGELOG.md`.

## Approaches considered

1. **Make the new look the default (chosen by Dave).** New defaults for `decorations` and the
   tab styles, plus new options for what styles can't express. One look to maintain. Everyone's
   TankTerm changes look, and snapshots need updating.
2. **Keep the old defaults and ship the look as an example config.** Defaults don't move, but nobody
   sees the look unless they go looking for it.
3. **A named switch (`tabs.look = "Slim"`).** Two built-in looks to maintain, overlapping with the
   style blocks that already exist.

Dave picked 1: "make it the default".

## The design

### Window

- `window.decorations` defaults to `None` on every platform. `window.buttons` keeps its
  `Desktop` default, so the buttons follow KWin's or GNOME's layout, and sit on the left on macOS.

### Tab bar colours and size (new defaults in `TabBarStyles::new`)

- **Bar:** the primary background, no longer mixed 12 % toward the foreground. It shows the
  window's opacity like the terminal does.
- **Shown tab (`tab_active`):** very slightly lifted from the bar (the foreground at around 5 %
  alpha over it, tuned by eye against the snapshot; see Opacity below), `border = "Thick"` (2 px)
  on the **top side only**, in the foreground colour. The existing single-side borders already
  draw this.
- **Other tabs (`tab_inactive`):** no background of their own, and the dim foreground, as today. A
  transparent top border of the same thickness keeps the titles of shown and other tabs on the
  same line.
- **Hovered:** as today (full foreground), plus a slight background lift.
- **Opacity** *(my call)*: the bar's background takes `window.opacity`, always, even when
  `[style.tab_bar]` sets it. That's the same rule the terminal's background follows, so the bar
  and terminal look like one surface. By default, tabs and buttons draw no background of their
  own, so the bar shows through. The shown tab's and the hovered lift are drawn as the foreground
  at a low alpha over the bar, not as opaque mixed colours, so they don't show as boxes on a
  translucent bar. A background set in `[style.tab_*]` for a tab, button or status segment stays
  opaque, as today. The red close-hover stays opaque. An opaque bar comes back with
  `window.opacity = 1`.
- **Bottom bar** *(my call)*: with `tabs.position = "Bottom"`, the accent line goes on the edge
  next to the terminal, which is the tab's top side. `view.rs` picks the side from the position.
  A border side set in `[style.tab_active]` wins.
- **Height:** vertical padding raised so the bar is about 2.6× the font size high (≈36 px at
  14 px). Horizontal tab padding is about one font size on each side. *(Exact numbers are the
  builder's to settle against the snapshot.)*

### Tab label

- `tabs.title` defaults to `{index}  {title}{zoom}{marker}`: two spaces and no colon.
- The text that the `{index}` placeholder produces is drawn with a new `[style.tab_index]` block
  (with `[style.blurred.tab_index]`). All other text keeps the tab's style. A user format without
  `{index}` is unaffected.
- *My call:* `tab_index` is a layer applied **last**, on top of active/inactive → activity/attention
  → hovered. Its default is `faint = true`, which draws the number at the tab's own foreground
  blended about halfway toward the bar's background. The number is therefore dimmer than the title
  on every tab, whether shown, other, hovered or wanting attention, and it follows the tab's colour.
  A `foreground` set in `[style.tab_index]` replaces that. The tab-bar label has to learn to draw
  `faint`.

### Tab title when the shell hasn't set one

Today a new tab shows "TankTerm" rather than `davewk@mainbox:~`. The cause: Fedora's
`/etc/bashrc` sets the title (`printf "\033]0;%s@%s:%s\007" …`) only when `$TERM` matches `xterm*`.
Terminals that run shells with `TERM=xterm-256color` get that title; TankTerm's shells get `TERM=alacritty`, so bash never
sets a title and `pane_title` falls back to `window.title` ("TankTerm").

- When the focused shell hasn't set a title and `dynamic_title` is on (the default), the tab's
  title becomes `user@host:path` instead of `window.title`, built from what TankTerm already
  knows:
  - **path:** the pane's working directory as `status::display_path` shows it (`~`, `~/src`,
    `/etc`).
  - **host:** the local short hostname, as `${HOSTNAME%%.*}` gives it. *(My call: TankTerm
    drops the host of a remote OSC 7 today (`ShellSequence::RemoteDirectory`), and the ssh rule
    below covers the usual remote case, so the terminal crate isn't changed for this.)*
  - **user:** the local user name.
- When the working directory isn't known, the title falls back to `window.title`, as today. On
  macOS and Windows that means it shows only when the shell reports its directory with OSC 7.
- A title that the shell or a program sets (OSC 0/2) still wins, as today.
- *My call:* this is part of what `dynamic_title` means, "the title follows what's in the
  terminal". `dynamic_title = false` always shows `window.title`, as today, and that's how to turn
  this off. It goes in the "old look" block.
- *My call:* this is the tab's title everywhere it's used, not only in the tab label: the window
  title the desktop shows, the command palette's tab list, the text the rename prompt starts with,
  and the IPC pane list's `title`/`tab_title`. One rule in `pane_title` keeps them all
  consistent. IPC scripts that compared against "TankTerm" will see `user@host:path` instead,
  which the CHANGELOG notes.
- The working directory and process name are only polled today when something shows them
  (`poll_directories` / `poll_process_names` in `app/update.rs`). They now also have to be polled
  while the fallback can show, which on Linux means reading `/proc` for each shell once a second,
  as those polls already do.
- *My call:* not switching `TERM` to `xterm-256color`, which would give up TankTerm's terminfo
  entry to get one line of Fedora's bashrc. Not making the shell integration scripts set the
  title either, because that would only cover shells that have integration.
- *Known limit:* over plain `ssh` without shell integration on the remote end, the working
  directory TankTerm sees is the local `ssh` process's, so the fallback would show a local path.
  When the focused pane's foreground process is `ssh` (or `tankTerm ssh`), the fallback keeps
  `window.title` instead.

### Tab width

- New option `tabs.width`, with two values:
  - `"Title"` (new default): each tab is as wide as its label, padding and close button. Labels
    longer than the old maximum (240 px, or the style's `max_width`) are clipped. When all tabs
    together don't fit, they shrink evenly, as `TabStrip` already does for fixed widths.
  - `"Shared"`: today's behaviour.
- A style `width` still wins over both, as it does today.

### Close button

- New option `tabs.close_button`, with three values:
  - `"Hover"` (new default): a tab shows its `×` only while the mouse is over it. The room for the
    `×` is always kept, so the tab doesn't change width when the mouse enters it.
  - `"Always"`: today's behaviour.
  - `"Never"`: no `×`. A middle click or the keybinding still closes the tab.

### Icons

- `+`, the gear, minimize, maximize, restore and close are drawn as 1 px line icons with iced's
  canvas, all the same size (about the font's cap height) and in the button's foreground colour.
  This replaces the font `+`, the font `⚙`, the font `×` of the close button, and the
  rectangle-based window icons in `window_icon`. The tab's own `×` uses the same drawn close icon,
  smaller.
- The gear moves from the far right edge to just before the window buttons on that side, with a
  gap about two buttons wide between them. When the window buttons are on the left (macOS, or
  `buttons = "Left"`), the gear stays at the right end.
- The close button still turns red under the mouse (`tab_close_hovered`).

### App icon at the top left

Dave: "keep the icon on the top left". KDE draws TankTerm's icon at the left end of its title bar
today. That's KWin's menu button, `M` in its default `ButtonsOnLeft = "MS"`, which Dave's
`kwinrc` doesn't change. The bar keeps it.

- A new `TitleButton::Menu`, drawn as TankTerm's icon (`extra/logo/compat/tankterm-64.png`, which
  `main.rs` already embeds as the window icon, scaled to the icon size), with no button
  background. A click opens the window menu, as a right click on the bar already does.
- It goes where the desktop's layout says: KWin's `M`, or GNOME's `appmenu`. With
  `buttons = "Right"`, `"Left"`, on macOS, and on desktops whose layout has no such button, it goes
  at the left end of the bar, after any window buttons there. With `buttons = "None"` there is no
  icon.
- *My call:* KWin's `S` (on all desktops, the blue pin in the screenshot) and its other buttons
  stay undrawn, as today.
- How the icon is drawn (iced's `image` widget from the decoded RGBA, which may need iced's
  `image` feature, or the SVG) is left to the builder.

### Getting the old look back

The docs and man page show this block:

```toml
[window]
decorations = "Full"
dynamic_title = false   # only if the "TankTerm" fallback title is wanted back too

[tabs]
title = "{index}: {title}{zoom}{marker}"
width = "Shared"
close_button = "Always"
```

The old colours (bar mixed toward the foreground, shown tab filled) are a `[style.tab_bar]` and
`[style.tab_active]` override, shown next to it.

## Left to the builder

- Exact padding, background lift percentages and icon sizes. Tune them against an `iced_test`
  snapshot.
- How the `{index}` span reaches the label: for example, the template rendering into segments,
  or `tab_label` returning the index part separately. Pick whatever is simplest given
  `Template` and `label()`.
- How `TabStrip` learns a tab's natural width in `"Title"` mode: measure it in `layout`, or
  compute it from the label.
- Names of the new enums and where their tests live, following the existing `config/tabs.rs`
  patterns.
- Which existing snapshots and tests need new expected values. Each change should be explained by
  this design.
