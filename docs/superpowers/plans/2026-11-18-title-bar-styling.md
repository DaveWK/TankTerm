# Title Bar Styling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make TankTerm's default top bar one frameless bar with the app icon, title-wide tabs with a top accent line, hover-only close buttons, line icons, a see-through background, and `user@host:path` tab names.

**Architecture:** Everything lives in the iced front end (`crates/tankterm`). New behaviour is added as config options and `[style.*]` defaults (`config/tabs.rs`, `config/style.rs`), drawn in `app/view.rs` with `app/tab_strip.rs` for widths and a new `app/icons.rs` for canvas icons. The `decorations = "None"` default flips last, so earlier tasks don't have to fight the default bar in every test.

**Tech Stack:** Rust, iced 0.14 (canvas; `svg` added in Task 7), `iced_test` Simulator for tests.

**Spec:** `docs/superpowers/specs/2026-11-18-title-bar-styling-design.md` (read it first; it says why for every choice below).

## Global Constraints

- Work on branch `title-bar-styling` off local `main`; merge into `main` locally at the end. **Never push.** (Dave's global rule: nothing public without his explicit yes.)
- Commit messages: plain imperative sentence like the existing history ("Add …", "Draw …"). **No `Co-Authored-By` lines.**
- GUI is tested headlessly with `iced_test::simulator::Simulator` only. Never screenshot or inject input on the desktop. `render_tests.rs` tests are `#[ignore]` manual screenshots; update them so they still compile and run, but they assert nothing.
- Every new option and style block is documented in `extra/man/tankTerm.5.scd`, `docs/features.md` and `CHANGELOG.md` in the task that adds it.
- Config enums follow the existing pattern: `#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]` with `#[default]` on a variant (see `ShowTabs` in `config/tabs.rs`).
- Test commands: `cargo test -p tankterm <filter>`; full check before each commit: `cargo test -p tankterm && cargo clippy -p tankterm --all-targets -- -D warnings && cargo +nightly-2026-09-08 fmt --all -- --check` (the toolchain CI pins as `RUSTFMT_TOOLCHAIN`; never plain `+nightly`, which rustup auto-installs).

## Review Focus

1. **A very long title in a title-wide tab** (deep path, long OSC title): the tab stops at 240 px (or `max_width`) and is clipped; `+` and the window buttons stay on the bar and clickable. → test in Task 2.
2. **Many tabs that don't fit**: title-wide tabs shrink evenly; nothing is pushed off the bar. → test in Task 2.
3. **Clicking the close button's spot on a tab that isn't hovered yet**: with `close_button = "Hover"` a single click there closes that tab exactly once (the hovered layer takes the click). → test in Task 3.
4. **Fallback title when it would lie**: shell is `ssh` → `window.title`; directory unknown → `window.title`; a program's OSC title still wins; `dynamic_title = false` → `window.title`. → tests in Task 1.
5. **Clicking the app icon**: opens the window menu and does *not* start a window drag or a double-click maximize. → test in Task 7.

---

### Task 1: `user@host:path` tab title when the shell hasn't set one

**Files:**
- Modify: `crates/tankterm/src/app/tabs.rs` (`tab_title`, `pane_title`, `tab_label` at ~432-484)
- Modify: `crates/tankterm/src/app/status.rs` (make `user_name`, `host_name` `pub(super)`)
- Modify: `crates/tankterm/src/app/update.rs` (`poll_directories` ~788, `poll_process_names` ~806; callers of `tab_title` ~324)
- Modify: `crates/tankterm/src/app/ipc.rs:74`, `crates/tankterm/src/app/tabs.rs:140-145` (callers)
- Test: `crates/tankterm/src/app/tab_tests.rs`
- Docs: `extra/man/tankTerm.5.scd` (`dynamic_title`, `tabs.title`'s `{title}`), `docs/features.md`, `CHANGELOG.md` (note IPC `title`/`tab_title` change)

**Interfaces:**
- Produces: `fn pane_title(&self, tab: &Tab) -> Cow<'_, str>` and `fn tab_title(&self, tab: &Tab) -> Cow<'_, str>` (were `&str`); `fn fallback_title(shell: &Shell) -> Option<String>` (free fn in `tabs.rs`).

- [ ] **Step 1: Write the failing tests** in `tab_tests.rs`:

```rust
#[test]
fn untitled_shells_are_named_after_where_they_are() {
    let mut app = app();
    let focused = app.focused_terminal().unwrap();
    let home = home::home_dir().unwrap();
    let (user, host) = (status::user_name().unwrap(), status::host_name().unwrap());

    // Until the directory is known, the configured title.
    assert_eq!(app.tab_title(&app.tabs[0]), "test");

    app.terminals.get_mut(&focused).unwrap().working_directory = Some(home.join("src"));
    assert_eq!(app.tab_title(&app.tabs[0]), format!("{user}@{host}:~/src"));
    assert_eq!(app.title(), format!("{user}@{host}:~/src"));

    // ssh's local directory says nothing about the remote one.
    app.terminals.get_mut(&focused).unwrap().process = Some("ssh".into());
    assert_eq!(app.tab_title(&app.tabs[0]), "test");
}

#[test]
fn titles_programs_set_win_over_the_directory() {
    let config = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \
                  \"printf '\\\\033]2;vim\\\\007'; exec cat\"] }\n";
    let mut app = app_with(config, Hotkeys::default());
    let focused = app.focused_terminal().unwrap();
    tick_until(&mut app, "the shell's title", |app| app.tab_title(&app.tabs[0]) == "vim");
    app.terminals.get_mut(&focused).unwrap().working_directory = Some("/etc".into());
    assert_eq!(app.tab_title(&app.tabs[0]), "vim");
}

#[test]
fn without_dynamic_titles_the_configured_title_stays() {
    let mut app = app_with("[window]\ndynamic_title = false\n", Hotkeys::default());
    // NB: `app_from` hardcodes `dynamic_title: true`; read it from the config instead
    // (`config.window.dynamic_title`) so this test means something.
    let focused = app.focused_terminal().unwrap();
    app.terminals.get_mut(&focused).unwrap().working_directory = Some("/etc".into());
    assert_eq!(app.tab_title(&app.tabs[0]), "test");
}

#[cfg(target_os = "linux")]
#[test]
fn directories_and_processes_are_polled_for_the_fallback_title() {
    let mut app = app();
    tick_until(&mut app, "the directory to be known", |app| {
        let id = app.focused_terminal().unwrap();
        app.terminals[&id].working_directory.is_some() && app.terminals[&id].process.is_some()
    });
}
```

- [ ] **Step 2: Run them to see them fail.** `cargo test -p tankterm untitled_shells directories_and_processes without_dynamic_titles titles_programs_set` → FAIL (title is "test"; nothing polled).

- [ ] **Step 3: Implement.**
  - `fallback_title(shell)` returns `None` when `shell.working_directory` is `None`, or when `shell.process == Some("ssh")`. `tankTerm ssh` runs `ssh` as the foreground process, so the same check covers it. Verify that in `ssh.rs`; if it doesn't, also match the name it does run. Otherwise it returns `format!("{user}@{host}:{}", status::display_path(dir))`, and a missing user or host gives `None`.
  - `pane_title`: shell title if `dynamic_title` → else, if `dynamic_title`, `fallback_title` → else `options.title`. Return `Cow`.
  - `poll_directories` / `poll_process_names`: also poll while `self.options.dynamic_title`. (The fallback can show whenever dynamic titles are on.)
  - Fix callers for `Cow` (`.into_owned()` / `&*`).
  - Make `app_from` in `tab_tests.rs` take `dynamic_title` from `config.window.dynamic_title`.

- [ ] **Step 4: Run the new tests and the whole crate.** `cargo test -p tankterm`. Expected: all pass. Fix existing tests that assumed the title "test" after a tick (`titles_follow_the_focused_terminal`, `tab_titles_follow_the_title_format`, session tests) by setting `working_directory = None` or asserting the new title — each change must be explained by this task.

- [ ] **Step 5: Docs** (man page, features.md, CHANGELOG), then the full check from Global Constraints.

- [ ] **Step 6: Commit** `git commit -m "Name untitled tabs after their user, host and directory"`.

---

### Task 2: `tabs.width`: tabs as wide as their title

**Files:**
- Modify: `crates/tankterm/src/config/tabs.rs` (new field and enum)
- Modify: `crates/tankterm/src/app/tab_strip.rs` (`TabWidth`, `tab_widths`, `layout`)
- Modify: `crates/tankterm/src/app/view.rs` (`tab_bar`, tab width setup ~244-253)
- Test: `crates/tankterm/src/app/tab_strip.rs` (unit), `crates/tankterm/src/app/tab_tests.rs`
- Docs: man page `[tabs] width`, features.md, CHANGELOG

**Interfaces:**
- Produces: `pub enum TabSizing { #[default] Title, Shared }` and `Tabs.width: TabSizing` (TOML key `width`; the enum is not named `TabWidth` because `tab_strip::TabWidth` exists). `tab_strip::TabWidth` gains `pub fit: bool`.

- [ ] **Step 1: Write the failing tests.**

```rust
// tab_strip.rs unit tests
#[test]
fn fitted_tabs_keep_their_natural_width_until_the_room_runs_out() {
    // tab_widths now takes the natural widths of fitted tabs:
    // fn tab_widths(room: f32, tabs: &[TabWidth], natural: &[Option<f32>]) -> Vec<f32>
    let fit = TabWidth { fixed: None, max: 240., fit: true };
    assert_eq!(tab_widths(1000., &[fit, fit], &[Some(120.), Some(300.)]), [120., 240.]);
    // Not enough room: they shrink by the same share.
    assert_eq!(tab_widths(180., &[fit, fit], &[Some(120.), Some(240.)]), [60., 120.]);
}

// tab_tests.rs
#[test]
fn tabs_are_as_wide_as_their_titles_by_default() {
    // Two tabs renamed (`app.tabs[i].title = Some(..)`) to "a" and "a much longer title".
    // Tab bounds come from the `TabStrip` node's children. Add
    // `pub(super) fn tab_bounds(app: &App) -> Vec<Rectangle>` in tab_tests.rs, built on
    // `text_x` and the find bounds `tabs_share_the_bar_evenly_up_to_the_widest_tab` uses.
    let [a, long] = tab_bounds(&app)[..] else { panic!() };
    assert!(a.width < long.width);
    assert!(long.width < MAX_TAB_WIDTH);
    // The `+` starts right after the last tab, not at 2 * MAX_TAB_WIDTH.
    assert!(text_x(&app, "+") < a.width + long.width + 40.); // Task 6 changes this lookup to an id
}

#[test]
fn long_titles_and_crowded_bars_keep_the_buttons_on_the_bar() {
    // Review Focus 1: one tab titled "x".repeat(200).
    assert!(tab_bounds(&app)[0].width <= MAX_TAB_WIDTH + 2.);
    // Review Focus 2: 30 tabs in an 800 px window.
    assert!(tab_bounds(&crowded).iter().all(|tab| tab.width < 800. / 30.));
    assert!(actions_along_the_tab_bar(&crowded, false).contains(&AppAction::NewTab));
    // (`actions_along_the_tab_bar` is in option_tests.rs; make it `pub(super)`.)
}
```

Update `tabs_share_the_bar_evenly_up_to_the_widest_tab` to set `[tabs]\nwidth = "Shared"`.

- [ ] **Step 2: Run them to see them fail.** `cargo test -p tankterm fitted_tabs tabs_are_as_wide long_titles` → compile error / FAIL.

- [ ] **Step 3: Implement.**
  - `tab_widths(room, tabs, natural)`: a fitted tab with `natural = Some(w)` acts as `fixed = Some(w.min(tab.max))`; everything else is unchanged.
  - `TabStrip::layout`: for each fitted tab, measure first with `Limits::new(Size::ZERO, Size::new(width.max, max.height))` and use that node's width as its natural width, then lay out at the final width as today.
  - In `view.rs`, with `TabSizing::Title`, build the tab's title container and select button with `Length::Shrink` widths (they are `Fill` today, which would measure as the maximum) and set `fit: true`. A style `width` still sets `fixed` and wins.

- [ ] **Step 4: Run the tests.** `cargo test -p tankterm`. Expected: PASS.

- [ ] **Step 5: Docs, full check, commit** `git commit -m "Size tabs to their titles"`.

---

### Task 3: `tabs.close_button`: show a tab's `×` only under the mouse

**Files:**
- Modify: `crates/tankterm/src/config/tabs.rs`
- Modify: `crates/tankterm/src/app/view.rs` (the tab `view` closure ~214-232)
- Test: `crates/tankterm/src/app/tab_tests.rs`
- Docs: man page `[tabs] close_button`, features.md, CHANGELOG

**Interfaces:**
- Produces: `pub enum CloseButton { #[default] Hover, Always, Never }` and `Tabs.close_button: CloseButton`.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test]
fn close_buttons_show_under_the_mouse() {
    // Default (Hover): the tab's width is the same with the mouse over it or not
    // (room is kept), and the `×` is only drawn in the hovered layer
    // (compare pictures with `same_picture`-style rendering, mouse away vs over the tab).
}

#[test]
fn a_hidden_close_button_closes_its_tab_with_one_click() {
    // Review Focus 3: point at where the `×` of tab 0 is (find it as in
    // `bordered_tabs_and_buttons_take_clicks`), click once:
    // messages == [Message::CloseTab(0)].
}

#[test]
fn close_buttons_can_always_show_or_never() {
    // "Always": `×` findable without hovering. "Never": no `CloseTab` message anywhere
    // along the tab; a middle click still gives `CloseTab(0)`.
}
```

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement.** In the tab `view` closure, take a `show_close: bool`. The normal layer gets `show_close = close_button == Always`. With `Hover`, the normal layer draws the close glyph in a fully transparent colour with no `on_press`, which keeps its room. The hovered layer gets `show_close = close_button != Never`. With `Never`, the close button is left out of both layers.

- [ ] **Step 4: Run the tests** (`cargo test -p tankterm`). Expected: PASS.

- [ ] **Step 5: Docs, full check, commit** `git commit -m "Show a tab's close button under the mouse"`.

---

### Task 4: Tab label: `{index}  {title}` with a faint number (`[style.tab_index]`)

**Files:**
- Modify: `crates/tankterm/src/template.rs` (new method)
- Modify: `crates/tankterm/src/config/tabs.rs` (`TabTitle` default)
- Modify: `crates/tankterm/src/config/style.rs` (`StyleConfig.tab_index`, `BlurredStyleConfig.tab_index`, `TabBlocks.index`, `TabBarStyles.index`)
- Modify: `crates/tankterm/src/app/tabs.rs` (`tab_label_parts`), `crates/tankterm/src/app/view.rs` (`label` learns `faint`; tab label as a row of parts)
- Test: `template.rs`, `config/style.rs` tests, `tab_tests.rs`
- Docs: man page (`tabs.title` default, `[style.tab_index]`), features.md, CHANGELOG

**Interfaces:**
- Produces: `Template::render_parts<'a>(&self, field: &str, value: impl Fn(&str) -> Option<Cow<'a, str>>) -> Option<Vec<(String, bool)>>`. This gives the rendered pieces with adjacent pieces merged, and `true` for text that `field` produced. Also `App::tab_label_parts(&self, index: usize, tab: &Tab) -> Vec<(String, bool)>`, and `TabBarStyles.index: TabLayer`. `tab_label` stays, returning the joined `String`.

- [ ] **Step 1: Write the failing tests.**

```rust
// template.rs
#[test]
fn parts_say_which_text_a_field_produced() {
    let parts = Template::parse("{index}  {title}!").render_parts("index", |name| match name {
        "index" => Some("2".into()),
        "title" => Some("vim".into()),
        _ => None,
    });
    assert_eq!(parts.unwrap(), [("2".into(), true), ("  vim!".into(), false)]);
}

// style.rs
#[test]
fn tab_numbers_are_faint_by_default() {
    let styles = load("").style.resolved().tab_bar(true);
    assert!(styles.active.with(&styles.index).attributes.faint);
    let styles = load("[style.tab_index]\nfaint = false\nforeground = '#ff0000'\n")
        .style.resolved().tab_bar(true);
    let index = styles.inactive.with(&styles.index);
    assert!(!index.attributes.faint);
    assert_eq!(index.foreground, Rgb::new(0xff, 0, 0));
}

// tab_tests.rs: update tab_titles_follow_the_title_format
assert_eq!(app.tab_label(0, &app.tabs[0]), "1  test");
assert_eq!(app.tab_label_parts(0, &app.tabs[0]), [("1".into(), true), ("  test".into(), false)]);
```

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement.**
  - The default format is `"{index}  {title}{zoom}{marker}"`.
  - `tab_index` resolves like the other blocks. Its default is `TabLayer { faint: Some(true), .. }.below(user layer)`, the same way `hovered` builds its default. The blurred block works like the others.
  - In `view.rs`, the tab's label is a `row` with one `label()` per part. An index part is drawn with `style.with(&styles.index)`, applied **after** attention/activity/hover.
  - `label()` draws `faint` as the foreground blended 50 % toward `style.colors().1`, using `palette::blend` or the `mix` equivalent.
  - Tests that find the tab by `"1: test"` (`tab_bar_middle`, `the_tab_bar_is_the_title_bar_only_without_the_system_one`, `the_window_buttons_can_be_at_both_ends`, others found with `grep -rn '"1: test"' crates/tankterm/src/app`) find the title part instead (`"  test"`), or a small `find_tab(ui, index)` helper does.

- [ ] **Step 4: Run the tests** (`cargo test -p tankterm`). Expected: PASS.

- [ ] **Step 5: Docs, full check, commit** `git commit -m "Draw tab numbers faint, without a colon"`.

---

### Task 5: Bar colours: see-through bar, lifted shown tab, top accent line, taller bar

**Files:**
- Modify: `crates/tankterm/src/config/style.rs` (`Fill`, `TabStyle.fill`, `TabLayer.fill`, `TabBarStyles::new` defaults)
- Modify: `crates/tankterm/src/app/view.rs` (`button_style`, bar container, status segments, accent side for `TabsPosition::Bottom`)
- Test: `config/style.rs` tests, `app/option_tests.rs`
- Docs: man page (style defaults, opacity rule for the bar), features.md, CHANGELOG

**Interfaces:**
- Produces: `pub enum Fill { Color, Bar, Lift(u8) }` (`Lift(p)` = the style's foreground at `p` % alpha). `TabStyle.fill: Fill`, `TabLayer.fill: Option<Fill>`. `TabLayer::new` sets `fill = Some(Fill::Color)` whenever the user's style sets `background`. In `view.rs`: `fn fill(style: &TabStyle) -> Option<iced::Background>`.

- [ ] **Step 1: Write the failing tests.**

```rust
// style.rs
#[test]
fn tabs_sit_on_the_bar_with_an_accent_line_on_the_shown_one() {
    let config = load("");
    let primary = &config.colors.primary;
    let tabs = config.style.resolved().tab_bar(true);
    assert_eq!(tabs.bar.background, primary.background);           // no 12 % mix any more
    assert_eq!(tabs.active.fill, Fill::Lift(5));
    assert_eq!(tabs.inactive.fill, Fill::Bar);
    assert_eq!(tabs.button.fill, Fill::Bar);
    assert_eq!(tabs.status.fill, Fill::Bar);
    assert_eq!(tabs.active.border, Border::Thick);
    assert_eq!(tabs.active.border_sides(), Sides { top: true, ..Sides::default() });
    assert_eq!(tabs.active.border_foreground, primary.foreground);
    // Other tabs have no line, and 2 px more top padding, so the titles line up.
    assert_eq!(tabs.inactive.border, Border::Hidden);
    assert_eq!(tabs.inactive.padding.top, tabs.active.padding.top + 2);
    // At the bottom the line stays on the tab's top edge, which is then next to the
    // terminal (spec), so `tabs.position` changes nothing here.
}

#[test]
fn a_background_someone_sets_is_drawn_solid() {
    let tabs = load("[style.tab_inactive]\nbackground = '#123456'\n").style.resolved().tab_bar(true);
    assert_eq!(tabs.inactive.fill, Fill::Color);
}

// option_tests.rs (Review Focus: opacity)
#[test]
fn the_tab_bar_is_as_see_through_as_the_terminal() {
    // With [window] opacity = 0.5: the bar's container background alpha is 0.5;
    // a [style.tab_inactive] background stays alpha 1.0. With opacity = 1: bar alpha 1.0.
    // Test through `view::fill` and a small `App::bar_background()` helper; there is
    // no pixel read-back of alpha.
}
```

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement.**
  - `TabBarStyles::new`:
    - **bar:** `background = primary.background`; padding is zero.
    - **base:** padding becomes `Spacing::new(9, 14)` as the starting value. Tune it so the bar is about `2.6 × font size` high (about 36 px at 14 px) and record the final numbers in the man page.
    - **active:** `fill: Lift(5)`; `border: Thick`; `border_sides` is top only; `border_foreground` is the foreground.
    - **inactive, button, status:** `fill: Bar`.
    - **inactive:** no border, with `padding.top` 2 px more than the shown tab's, which makes up for its 2 px line.
    - **hovered:** add `fill: Some(Lift(8))`.
    - **close_hovered:** stays `Color` (opaque red).
  - `with`/`below` carry `fill` like the other properties.
  - `view.rs`:
    - `button_style` and the status segment containers use `fill(style)`.
    - The bar container's background is `Color { a: self.opacity(), ..bar background }`, via `pub(super) fn bar_background(&self) -> Color`.

- [ ] **Step 4: Run the tests** (`cargo test -p tankterm`). Update `tab_bar_defaults_blend_primary_colors` and its neighbours in `style.rs` to the new defaults. Each update is explained by this task.

- [ ] **Step 5: Docs.** Write the "old colours" override (bar mixed 12 % toward the foreground, filled shown tab) into the man page and features.md, then run the full check.

- [ ] **Step 6: Commit** `git commit -m "Restyle the tab bar's colours and height"`.

---

### Task 6: Line icons and the gear's place

**Files:**
- Create: `crates/tankterm/src/app/icons.rs`
- Modify: `crates/tankterm/src/app/mod.rs` (`mod icons;`), `crates/tankterm/src/app/view.rs` (`+`, gear, `window_icon`, tab `×`, gear spacing)
- Test: `crates/tankterm/src/app/icons.rs` (unit), `option_tests.rs`, `tab_tests.rs`, `key_table_tests.rs:249`

**Interfaces:**
- Produces:
  - `pub enum Icon { Plus, Gear, Minimize, Maximize, Restore, Close }`
  - `pub fn icon<'a, M: 'a>(icon: Icon, size: f32, color: iced::Color) -> Element<'a, M>`: an iced `Canvas` of `size × size` drawing 1 px strokes.
  - `pub fn id(icon: Icon) -> iced::widget::Id` (`"icon-plus"`, `"icon-gear"`, …), so tests can find icons. The tab `×` uses `Id::new(format!("close-tab-{index}"))`.

- [ ] **Step 1: Write the failing tests.**

```rust
// icons.rs: every icon draws something, in its box
#[test]
fn icons_are_drawn_in_their_box() { /* render each `icon(...)` in a Simulator 20×20 snapshot and assert it isn't blank */ }

// option_tests.rs
#[test]
fn the_gear_sits_apart_from_the_window_buttons() {
    // FRAMELESS: x of the gear's right edge + 2 * button width <= x of the minimize button's left
    // edge, and the gear isn't at the far right.
}
```

Replace text lookups of icons with ids: `ui.find("+")` → `ui.find(icons::id(Icon::Plus))`, `ui.click("\u{2699}")` → `ui.click(icons::id(Icon::Gear))`, `click(&app, "\u{d7}")` → the tab close id. `the_maximize_button_of_a_maximized_window_shows_the_restore_icon` stays as is (pictures differ).

- [ ] **Step 2: Run them to see them fail.** If `iced_test` 0.14 can't select by `Id`, fall back to finding the button by position next to a known text, and note it in the commit.

- [ ] **Step 3: Implement.**
  - The icons are drawn with `canvas::Path` strokes, 1 px wide, at about the cap height (`size * 0.7`).
  - The gear is a circle with 6–8 short teeth and a small inner circle.
  - Restore is two offset squares, with the back one cut where the front covers it, as `window_icon` does today.
  - The font `+`, `⚙` and `×`, and the rectangle icons in `window_icon`, are replaced. The close button's red hover is unchanged.
  - The gear is followed by a `Space` about two buttons wide when the right group of window buttons isn't empty.

- [ ] **Step 4: Run the tests** (`cargo test -p tankterm`). Expected: PASS.

- [ ] **Step 5: Full check, commit** `git commit -m "Draw the tab bar's icons as thin lines"`.

---

### Task 7: The app icon at the top left

**Files:**
- Modify: `crates/tankterm/Cargo.toml` (`iced` gets `features = ["svg"]`)
- Modify: `crates/tankterm/src/title_buttons.rs` (`TitleButton::Menu`; `kde`, `gnome`, `new`, `desktop`, `default`)
- Modify: `crates/tankterm/src/app/view.rs` (draw `Menu`)
- Test: `title_buttons.rs` tests, `option_tests.rs`
- Docs: man page (`window.buttons`), features.md (title bar section), CHANGELOG

**Interfaces:**
- Consumes: `Message::TitleBarMenu` (already shows the system menu).
- Produces: `TitleButton::Menu`. The icon is `svg::Handle::from_memory(include_bytes!("../../../../extra/logo/tankterm-small.svg"))`. Check the relative path from `view.rs`.

- [ ] **Step 1: Write the failing tests.**

```rust
// title_buttons.rs
#[test]
fn the_menu_button_goes_where_the_desktop_puts_it() {
    assert_eq!(TitleButtons::kde("MS", "HIAX").left, [Menu]);
    assert_eq!(TitleButtons::gnome("appmenu:minimize,maximize,close").left, [Menu]);
    // Layouts without one, and Right/Left, get it first on the left.
    assert_eq!(TitleButtons::kde("", "IAX").left, [Menu]);
    assert_eq!(TitleButtons::new(WindowButtons::Right, Default::default).unwrap().left, [Menu]);
    assert_eq!(TitleButtons::new(WindowButtons::Left, Default::default).unwrap().left,
               [Close, Maximize, Minimize, Menu]);
}

// option_tests.rs (Review Focus 5)
#[test]
fn clicking_the_app_icon_opens_the_window_menu_without_moving_the_window() {
    // FRAMELESS; click at x = 6 on the bar's middle line, twice (double click):
    // messages contain TitleBarMenu, and no TitleBarPressed / TitleBarDoubleClicked.
}
```

Update `the_tab_bar_of_a_frameless_window_has_the_window_buttons` (left case) and `the_window_buttons_can_be_at_both_ends`: from the left, the first message is now the menu, not an `AppAction`. Assert it with a messages helper, or skip it in `actions_along_the_tab_bar`, since it isn't an `AppAction`.

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement.**
  - `kde` maps `'M' => Some(Menu)`; `gnome` maps `"appmenu" => Some(Menu)`.
  - After a layout is built, if it has no `Menu`, push one at the end of `left`.
  - `buttons = "None"` still gives `None`, so there's no icon.
  - In `view.rs`, `Menu` is drawn as `mouse_area(svg(handle).width(size).height(size))` inside the same padded container as the other buttons, with `.on_press(Message::TitleBarMenu)` and `.on_double_click(Message::TitleBarMenu)`. It has no hover background. Because the `mouse_area` takes the press, the bar's own `mouse_area` doesn't see it.

- [ ] **Step 4: Run the tests** (`cargo test -p tankterm`). Expected: PASS.

- [ ] **Step 5: Docs, full check, commit** `git commit -m "Keep TankTerm's icon at the left of the title bar"`.

---

### Task 8: Make it the default

**Files:**
- Modify: `crates/tankterm/src/config/window.rs` (`Decorations`: `#[default]` moves to `None`)
- Modify: `crates/tankterm/src/main.rs` tests (~329, ~365-376)
- Modify: test configs that assumed a system title bar (`grep -rn 'assert!(!shown(""))\|tab_bar_is_shown_with_more_than_one_tab\|tab_bar_visibility_is_configurable' crates/tankterm/src`, and any position-dependent test that now has the bar above the terminal)
- Modify: `crates/tankterm/src/app/render_tests.rs` (still compiles; `readme_screenshot` shows the new bar)
- Docs: man page (`decorations` default), `docs/features.md` ("The tab bar as title bar" becomes the default; add the **old look** block below), `CHANGELOG.md`

**Interfaces:** none new.

- [ ] **Step 1: Write the failing test.**

```rust
// main.rs tests
#[test]
fn windows_open_without_the_system_title_bar_by_default() {
    let settings = window_settings(&UiConfig::default()); // the existing helper those tests use
    assert!(!settings.decorations);
}
```

- [ ] **Step 2: Run to see it fail.**

- [ ] **Step 3: Flip the default.** Fix every test that now fails because the bar is shown. Prefer adding `[window]\ndecorations = "Full"\n` to a test's config when the test isn't about the bar. Change an assertion only when the test is about the bar's visibility.

- [ ] **Step 4: Write the old-look block** into features.md and the man page, exactly:

```toml
[window]
decorations = "Full"
dynamic_title = false   # only if the "TankTerm" fallback title is wanted back too

[tabs]
title = "{index}: {title}{zoom}{marker}"
width = "Shared"
close_button = "Always"
```

Put the colour override from Task 5 next to it.

- [ ] **Step 5: Full check.** Then run the manual screenshots: `TANKTERM_SNAPSHOT_DIR=/tmp/claude-1000/-home-davewk-src-tankterm/c9de2f8a-0d9c-4ee8-9647-d02ef5152de1/scratchpad/shots cargo test -p tankterm render_tests -- --ignored`. Look at `tab_bar_styles` / `readme_screenshot` and tune Task 5's padding if the bar isn't about 36 px at 14 px.

- [ ] **Step 6: Commit** `git commit -m "Make the tab bar the title bar by default"`.

- [ ] **Step 7: Merge** into `main` locally: `git checkout main && git merge --no-ff title-bar-styling -m "Merge branch 'title-bar-styling' into main"`. **Do not push.**
