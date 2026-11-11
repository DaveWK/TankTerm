TANKTERM(5)

# NAME

TankTerm - TOML configuration file format.

# SYNTAX

TankTerm's configuration file uses the TOML format. The format's specification
can be found at _https://toml.io/en/v1.1.0_.

# LOCATION

TankTerm doesn't create the config file for you, but it looks for one in the
following locations on UNIX systems:

. _$XDG_CONFIG_HOME/tankterm/tankterm.toml_
. _$XDG_CONFIG_HOME/tankterm.toml_
. _$HOME/.config/tankterm/tankterm.toml_
. _$HOME/.tankterm.toml_
. _/etc/tankterm/tankterm.toml_

On Windows, the config file will be looked for in:

. _%APPDATA%\\tankterm\\tankterm.toml_

# THEME

*theme* = _"<name>"_ | _"<path>"_

	A theme is a configuration file layered underneath yours, like a Glamour
	style or a Huh theme: it can set colors, light/dark color pairs and *[style]*
	blocks, and anything your configuration sets takes precedence.

	Built-in themes:

	_green\_screen_
		TankTerm's defaults: phosphor green on black.

	_charm_
		Charm's CharmTone palette, with light and dark variants.

	_dracula_
		Dracula.

	_catppuccin_
		Catppuccin Mocha when dark, Latte when light.

	_base16_
		Base16 Default Dark, Alacritty's original colors.

	Any other value is a path to a theme file, relative to the configuration
	file's directory unless it's absolute or starts with _~/_. The built-in
	themes are also installed as files in _extra/themes_ to start from.

	The *--theme* flag and _$TANKTERM\_THEME_ take precedence over this setting,
	in that order. Light/dark pairs in themes follow _colors.mode_.

	Default: _"None"_

# GENERAL

This section documents the *[general]* table of the configuration file.

*import* = [_"<string>"_,]

	Import additional configuration files.

	Imports are loaded in order, skipping all missing files, with the importing
	file being loaded last. If a field is already present in a previous import,
	it will be replaced.

	All imports must either be absolute paths starting with _/_, paths relative
	to the user's home directory starting with _~/_, or paths relative from the
	current config file.

	Example:
		import = [++
	_"~/.config/tankterm/base16-dark.toml"_,++
	_"~/.config/tankterm/keybindings.toml"_,++
	_"alacritty-theme/themes/gruvbox_dark.toml"_,++
]

*working_directory* = _"<string>"_ | _"None"_

	Directory the shell is started in. When this is unset, or _"None"_, the
	working directory of the parent process will be used.

	Default: _"None"_

*live_config_reload* = _true_ | _false_

	Live config reload (changes require restart)

	Default: _true_

*ipc_socket* = _true_ | _false_ # _(unix only)_

	Offer IPC using _tankTerm msg_

	Default: _true_

*accessible* = _true_ | _false_

	Reduce motion: the cursor never blinks, even when applications ask for it,
	and the visual bell is turned off. Like Charm's Huh, a non-empty
	_$ACCESSIBLE_ or _$TANKTERM\_ACCESSIBLE_ also enables it. Options set
	through *-o* or IPC can't turn motion back on.

	Default: _false_

# ENV

All key-value pairs in the *[env]* section will be added as environment variables
for any process spawned by TankTerm, including its shell. Some entries may
override variables set by TankTerm itself.

Example:
	*[env]*++
WINIT_X11_SCALE_FACTOR = _"1.0"_

# WINDOW

This section documents the *[window]* table of the configuration file.

*dimensions* = { columns = _<integer>_, lines = _<integer>_ }

	Window dimensions (changes require restart).

	Number of lines/columns (not pixels) in the terminal. Both lines and columns
	must be non-zero for this to take effect. The number of columns must be at
	least _2_, while using a value of _0_ for columns and lines will fall back
	to the window manager's recommended size

	Default: { columns = _0_, lines = _0_ }

*position* = _"None"_ | { x = _<integer>_, y = _<integer>_ } # _(has no effect on Wayland)_

	Window startup position.

	Specified in number of pixels.

	If the position is _"None"_, the window manager will handle placement.

	Default: _"None"_

*padding* = { x = _<integer>_, y = _<integer>_ } | _<integer>_ | [_<integer>_, ...]

	Blank space added around the terminal.

	As a table, the padding is in pixels, scaled by DPI, and the _x_ and _y_
	values are added on both opposing sides.

	As a number or a list, the padding is in terminal cells and follows the
	CSS shorthand used by Lip Gloss: 1 value sets all sides, 2 set vertical and
	horizontal, 3 set top, horizontal and bottom, and 4 set top, right, bottom
	and left.

	Example: _padding = [1, 2]_

	Default: { x = _0_, y = _0_ }

*dynamic_padding* = _true_ | _false_

	Spread additional padding evenly around the terminal content.

	Default: _false_

*decorations* = _"Full"_ | _"None"_ | _"Transparent"_ | _"Buttonless"_

	Window decorations.

	*Full*
		Borders and title bar.
	*None*
		Neither borders nor title bar.
	*Transparent* _(macOS only)_
		Title bar, transparent background and title bar buttons.
	*Buttonless* _(macOS only)_
		Title bar, transparent background and no title bar buttons.

	Default: _"Full"_

*opacity* = _<float>_

	Background opacity as a floating point number from _0.0_ to _1.0_. The value
	_0.0_ is completely transparent and _1.0_ is opaque.

	Default: _1.0_

*blur* = _true_ | _false_ # _(works on macOS/KDE Wayland)_

	Request compositor to blur content behind transparent windows.

	Default: _false_

*startup_mode* = _"Windowed"_ | _"Maximized"_ | _"Fullscreen"_ | _"SimpleFullscreen"_

	Startup mode (changes require restart)

	*Windowed*
		Regular window.
	*Maximized*
		The window will be maximized on startup.
	*Fullscreen*
		The window will be fullscreened on startup.
	*SimpleFullscreen* _(macOS only)_
		Same as _Fullscreen_, but you can stack windows on top.

	Default: _"Windowed"_

*title* = _"<string>"_

	Window title.

	Default: _"TankTerm"_

*dynamic_title* = _true_ | _false_

	Allow terminal applications to change TankTerm's window title.

	Default: _true_

*class* = { instance = _"<string>"_, general = _"<string>"_ } # _(Linux/BSD only)_

	Window class.

	On Wayland, *general* is used as _app\_id_ and *instance* is ignored.

	Default: { instance = _"TankTerm"_, general = _"TankTerm"_ }

*decorations_theme_variant* = _"Dark"_ | _"Light"_ | _"None"_

	Override the variant of the System theme/GTK theme/Wayland client side
	decorations. Set this to _"None"_ to use the system's default theme variant.

	Default: _"None"_

*resize_increments* = _true_ | _false_ # _(works on macOS/X11/Wayland)_

	Prefer resizing window by discrete steps equal to cell dimensions.

	Default: _false_

*option_as_alt* = _"OnlyLeft"_ | _"OnlyRight"_ | _"Both"_ | _"None"_ # _(macOS only)_

	Make _Option_ key behave as _Alt_.

	Default: _"None"_

*level* = _"Normal"_ | _"AlwaysOnTop"_

	Sets window level.

	*Normal*
		Window adheres to system's default z-order.
	*AlwaysOnTop*
		Window is a toplevel window.

	Default: _"Normal"_

Example:
	*[window]*++
padding = { x = _3_, y = _3_ }++
dynamic_padding = _true_++
opacity = _0.9_


# TABS

This section documents the *[tabs]* table of the configuration file.

A window holds one or more tabs, and every tab holds one or more terminals in
split panes. Tabs and panes are controlled with the key bindings listed in
*tankTerm-bindings*(5) and with the tab bar, where a tab is selected by clicking
it and closed with its close button or a middle click.

A tab that isn't shown is marked with _\*_ when one of its terminals prints
something, until the tab is shown again. A terminal that rings the bell or
finishes a command while it isn't looked at marks its tab with _!_ and gets a
bright frame in a split tab, until its pane has focus in the shown tab and the
window has focus; if the window doesn't have focus, the desktop is also asked to
draw attention to it. A command counts as finished when the shell gets the
terminal back from it, which needs no shell integration but is only known on
Unix, for shells with job control, and within about a second.

*show* = _"Always"_ | _"Multiple"_ | _"Never"_

	When the tab bar is shown.

	*Always*
		Always show the tab bar.
	*Multiple*
		Only show the tab bar while there is more than one tab.
	*Never*
		Never show the tab bar.

	Default: _"Multiple"_

*position* = _"Top"_ | _"Bottom"_

	Edge of the window the tab bar is placed at.

	Default: _"Top"_

*restore* = _true_ | _false_

	Open the tabs and split panes the last window had when TankTerm starts.

	The layout is remembered, in _$XDG_STATE_HOME/tankterm/session.json_
	(_%LOCALAPPDATA%\\tankterm\\session.json_ on Windows), and on Linux also the
	directory every shell was in; the shells themselves are started fresh.
	Nothing is restored or remembered when TankTerm is started with a command to
	run.

	Default: _false_

*inherit_working_directory* = _true_ | _false_

	Start the shells of new tabs and panes in the directory the shell of the
	current pane is in. Profiles aren't affected.

	The directory is read from _/proc_, so this only works on Linux; elsewhere
	new shells start in *general.working_directory* as before.

	Default: _true_

Example:
	*[tabs]*++
show = _"Always"_++
position = _"Bottom"_++
restore = _true_


# PROFILES

This section documents the *[[profiles]]* tables of the configuration file.

A profile is a named shell that the profile selector (see _ShowProfiles_) opens
in a new tab. Everything a profile doesn't set is taken from the rest of the
configuration. Tabs opened by _CreateNewTab_, splits and restored tabs always run
the regular shell.

*name* = _"<string>"_

	Name shown in the profile selector. This field is required.

*shell* = _"<string>"_ | { program = _"<string>"_, args = [_"<string>"_,] }

	Program to run instead of *terminal.shell*, like _ssh_ with a host.

	Default: _None_

*working_directory* = _"<string>"_

	Directory the program is started in.

	Default: _None_

*env* = { _<name>_ = _"<string>"_ }

	Environment variables set in addition to the *[env]* section.

	Default: _{}_

*theme* = _"<string>"_

	Theme used for the terminals of this profile in place of the configuration's
	*theme*, by name or path. Like any theme it is layered underneath the
	configuration, so colors set in the configuration file itself still apply.

	Default: _None_

Example:
	*[[profiles]]*++
name = _"Build server"_++
shell = { program = _"ssh"_, args = [_"build.example.com"_] }++
theme = _"dracula"_

	*[[profiles]]*++
name = _"Project"_++
working_directory = _"/home/user/project"_++
env = { EDITOR = _"hx"_ }


# SCROLLING

This section documents the *[scrolling]* table of the configuration file.

*history* = _<integer>_

	Maximum number of lines in the scrollback buffer.++
Specifying _0_ will disable scrolling.++
Limited to _100000_.

	Default: _10000_

*multiplier* = _<integer>_

	Number of line scrolled for every input scroll increment.

	Default: _3_

# FONT

This section documents the *[font]* table of the configuration file.

*normal* = { family = _"<string>"_, style = _"<string>"_ }

	Default: { family = _"Iosevka Term"_, style = _"Regular"_ }

	If the font can't be loaded, the system's monospace font is used instead:
	_"monospace"_ on Linux/BSD, _"Consolas"_ on Windows and _"Menlo"_ on macOS.

*bold* = { family = _"<string>"_, style = _"<string>"_ }

	If the family is not specified, it will fall back to the value specified for
	the normal font.

	Default: { style = _"Bold"_ }

*italic* = { family = _"<string>"_, style = _"<string>"_ }

	If the family is not specified, it will fall back to the value specified for
	the normal font.

	Default: { style = _"Italic"_ }

*bold_italic* = { family = _"<string>"_, style = _"<string>"_ }

	If the family is not specified, it will fall back to the value specified for
	the normal font.

	Default: { style = _"Bold Italic"_ }

*size* = _<float>_

	Font size in points.

	Default: _11.25_

*offset* = { x = _<integer>_, y = _<integer>_ }

	Offset is the extra space around each character. _y_ can be thought of as
	modifying the line spacing, and _x_ as modifying the letter spacing.

	Default: { x = _0_, y = _0_ }

*glyph_offset* = { x = _<integer>_, y = _<integer>_ }

	Glyph offset determines the locations of the glyphs within their cells with
	the default being at the bottom. Increasing _x_ moves the glyph to the
	right, increasing _y_ moves the glyph upward.

*builtin_box_drawing* = _true_ | _false_

	When _true_, TankTerm will use a custom built-in font for box drawing
	characters (Unicode points _U+2500_ - _U+259F_), legacy computing symbols
	(_U+1FB00_ - _U+1FB3B_), and powerline symbols (_U+E0B0_ - _U+E0B3_).

	Default: _true_

# COLORS

This section documents the *[colors]* table of the configuration file.

Like Lip Gloss, colors can be written in any of these forms:

	_"#RRGGBB"_
		Hexadecimal color.

	_"<0-15>"_
		ANSI color from the configured _normal_ (0-7) and _bright_ (8-15) colors.

	_"<16-255>"_
		ANSI 256 color: the 6x6x6 color cube and the gray ramp, unless overridden by
		_indexed\_colors_.

	{ light = _<color>_, dark = _<color>_ }
		Light/dark pair. The _dark_ color is used when the background is dark,
		otherwise the _light_ one; see _mode_.

*mode* = _"Dark"_ | _"Light"_ | _"None"_

	Which color of light/dark pairs to use. When _"None"_, it is _"Dark"_ if the
	primary background is dark (HSL lightness below 50%, like Lip Gloss's
	_HasDarkBackground_), and _"Light"_ otherwise.

	Default: _"None"_

*primary*

	This section documents the *[colors.primary]* table of the configuration file.

	*foreground* = _"<string>"_

		Default: _"#00ff00"_

	*background* = _"<string>"_

		Default: _"#000000"_

	*dim_foreground* = _"<string>"_

		If this is not set, the color is automatically calculated based on the
		foreground color.

		Default: _"#00a800"_

	*bright_foreground* = _"<string>"_

		This color is only used when _draw\_bold\_text\_with\_bright\_colors_ is
		_true_.

		If this is not set, the normal foreground will be used.

		Default: _"#f1fff5"_

*cursor* = { text = _"<string>"_, cursor = _"<string>"_ }

	Colors which should be used to draw the terminal cursor.

	The matching *[style]* block takes precedence; see *STYLE*.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: { text = _"#000000"_, cursor = _"#00ff00"_ }

*vi_mode_cursor* = { text = _"<string>"_, cursor = _"<string>"_ }

	Colors for the cursor when the vi mode is active.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: { text = _"#000000"_, cursor = _"#66ff66"_ }

*search*

	This section documents the *[colors.search]* table of the configuration.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	*matches* = { foreground = _"<string>"_, background = _"<string>"_ }

		Default: { foreground = _"#000000"_, background = _"#00aa00"_ }

	*focused_match* = { foreground = _"<string>"_, background = _"<string>"_ }

		Default: { foreground = _"#000000"_, background = _"#f4d35e"_ }

*hints*

	This section documents the *[colors.hints]* table of the configuration.

	*start* = { foreground = _"<string>"_, background = _"<string>"_ }

		First character in the hint label.

		Allowed values are hexadecimal colors like _#ff00ff_, or
		_CellForeground_/_CellBackground_, which references the affected cell.

		Default: { foreground = _"#000000"_, background = _"#f4d35e"_ }

	*end* = { foreground = _"<string>"_, background = _"<string>"_ }

		All characters after the first one in the hint label.

		Allowed values are hexadecimal colors like _#ff00ff_, or
		_CellForeground_/_CellBackground_, which references the affected cell.

		Default: { foreground = _"#000000"_, background = _"#ff4d5a"_ }

*line_indicator* = { foreground = _"<string>"_, background = _"<string>"_ }

	Color used for the indicator displaying the position in history during
	search and vi mode.

	Setting this to _"None"_ will use the opposing primary color.

	Default: { foreground = _"None"_, background = _"None"_ }

*footer_bar* = { foreground = _"<string>"_, background = _"<string>"_ }

	Color used for the footer bar on the bottom, used by search regex input,
	hyperlink URI preview, etc.

	Default: { foreground = _"#000000"_, background = _"#00ff00"_ }

*selection* = { text = _"<string>"_, background = _"<string>"_ }

	Colors used for drawing selections.

	The matching *[style]* block takes precedence; see *STYLE*.

	Allowed values are hexadecimal colors like _#ff00ff_, or
	_CellForeground_/_CellBackground_, which references the affected cell.

	Default: { text = _"#000000"_, background = _"#008000"_ }

*normal*

	This section documents the *[colors.normal]* table of the configuration.

	*black* = _"<string>"_
		Default: _"#000000"_
	*red* = _"<string>"_
		Default: _"#ff4d5a"_
	*green* = _"<string>"_
		Default: _"#00aa00"_
	*yellow* = _"<string>"_
		Default: _"#f4d35e"_
	*blue* = _"<string>"_
		Default: _"#4db8ff"_
	*magenta* = _"<string>"_
		Default: _"#d687ff"_
	*cyan* = _"<string>"_
		Default: _"#35e6d0"_
	*white* = _"<string>"_
		Default: _"#d8ffe7"_

*bright*

	This section documents the *[colors.bright]* table of the configuration.

	*black* = _"<string>"_
		Default: _"#40604d"_
	*red* = _"<string>"_
		Default: _"#ff7a83"_
	*green* = _"<string>"_
		Default: _"#66ff66"_
	*yellow* = _"<string>"_
		Default: _"#ffe78a"_
	*blue* = _"<string>"_
		Default: _"#8ad1ff"_
	*magenta* = _"<string>"_
		Default: _"#e9b3ff"_
	*cyan* = _"<string>"_
		Default: _"#8afff0"_
	*white* = _"<string>"_
		Default: _"#f1fff5"_

*dim*

	This section documents the *[colors.dim]* table of the configuration.

	If the dim colors are not set, they will be calculated automatically based
	on the _normal_ colors. The defaults below are only used for colors missing
	from a configured *[colors.dim]* table.

	*black* = _"<string>"_
		Default: _"#07110b"_
	*red* = _"<string>"_
		Default: _"#a82f3a"_
	*green* = _"<string>"_
		Default: _"#005500"_
	*yellow* = _"<string>"_
		Default: _"#a68d3b"_
	*blue* = _"<string>"_
		Default: _"#2a6e99"_
	*magenta* = _"<string>"_
		Default: _"#804c99"_
	*cyan* = _"<string>"_
		Default: _"#218f82"_
	*white* = _"<string>"_
		Default: _"#7faf91"_

*indexed_colors* = [{ index = _<integer>_, color = _"<string>"_ },]

	The indexed colors include all colors from 16 to 256.
	When these are not set, they're filled with sensible defaults.

	Default: _[]_

*transparent_background_colors* = _true_ | _false_

	Whether or not _window.opacity_ applies to all cell backgrounds, or only to
	the default background. When set to _true_ all cells will be transparent
	regardless of their background color.

	Default: _false_

*draw_bold_text_with_bright_colors* = _true_ | _false_

	When _true_, bold text is drawn using the bright color variants.

	Default: _false_

# STYLE

This section documents the *[style]* table of the configuration file. Its blocks
style the elements TankTerm draws on top of the terminal content, following
Charm's Lip Gloss: every block is a style with the properties below, and styles
can inherit from named styles.

Each property of an element is taken from, in order:

	. Its *[style.<element>]* block, then the styles listed in its _inherit_.
	. The matching *[colors]* setting, like _colors.selection.background_.
	. TankTerm's default for the element.

*Elements*

	*cursor*, *vi_mode_cursor*
		The terminal cursor. _foreground_ is the text under the cursor and
		_background_ the cursor itself. Only colors apply.

	*selection*
		Selected text.

	*search_match*, *search_focused_match*
		Search matches, and the focused match.

	*hint_start*, *hint_end*
		First and remaining characters of hint labels.

	*footer_bar*, *line_indicator*
		The footer bar (search, hyperlink previews) and the vi mode / search line
		indicator. Only colors apply; _CellForeground_ and _CellBackground_ refer
		to the primary colors.

	*help_key*, *help_desc*, *help_separator*
		Keys, descriptions and separators of the key help. Only colors apply.
		They default to the grays of Bubbles' help view, lighter on dark
		backgrounds, on the primary background.

*blurred*

	Changes to the styles while the window doesn't have focus, like Huh's
	blurred styles. The *[style.blurred.<element>]* blocks for _cursor_,
	_vi\_mode\_cursor_, _selection_, _search\_match_, _search\_focused\_match_,
	_hint\_start_ and _hint\_end_ start as copies of the focused styles, and
	only the properties they set change. *[style.blurred.text]* applies its text
	attributes, like _faint = true_, to all terminal content.

	By default, nothing changes when the window loses focus, apart from
	_cursor.unfocused\_hollow_.

*defs*

	Named styles to inherit from, defined as *[style.defs.<name>]* tables with the
	same properties. Like Lip Gloss style variables, they aren't drawn on their own.

*Style properties*

	*inherit* = _"<name>"_ | [_"<name>"_, ...]

		Named styles to take unset properties from, like Lip Gloss's _Inherit_.
		With several names, earlier ones win. Inheritance is transitive; unknown
		names and cycles are reported and skipped.

	*foreground*, *background* = _"<color>"_ | _"CellForeground"_ | _"CellBackground"_

		Colors in any of the forms described in *COLORS*, or a reference to the
		affected cell's own colors.

	*bold*, *italic*, *faint*, *underline*, *strikethrough*, *reverse* = _true_ | _false_

		Text attributes, applied to the cells the element covers. _faint_ dims the
		foreground and _reverse_ swaps foreground and background.

		Default: _false_

Example:
	*[style.defs.accent]*++
foreground = _"#000000"_++
background = _"#00ff00"_++
bold = _true_

	*[style.selection]*++
inherit = _"accent"_++
background = _"#008000"_

	*[style.search_focused_match]*++
inherit = _"accent"_++
underline = _true_

# BELL

This section documents the *[bell]* table of the configuration file.

*animation* = _"Ease"_ | _"EaseOut"_ | _"EaseOutSine"_ | _"EaseOutQuad"_ | _"EaseOutCubic"_
\| _"EaseOutQuart"_ | _"EaseOutQuint"_ | _"EaseOutExpo"_ | _"EaseOutCirc"_ | _"Linear"_
\| _"Spring"_

	Visual bell animation effect for flashing the screen when the visual bell is rung.

	_Spring_ animates the flash with the damped spring set by _spring_, like
	Charm's Harmonica. An underdamped spring overshoots, which shows as weaker
	flashes after the first one. The flash always ends after _duration_.

	Default: _"Linear"_

*spring* = { frequency = _<float>_, damping = _<float>_ }

	Spring for the _Spring_ animation. _frequency_ is the angular frequency in
	radians per second; higher is faster. _damping_ is the damping ratio: below
	_1.0_ the spring bounces, _1.0_ settles as fast as possible without bouncing,
	and above _1.0_ it settles more slowly.

	Default: { frequency = _18.0_, damping = _0.35_ }

*duration* = _<integer>_

	Duration of the visual bell flash in milliseconds. A `duration` of `0` will
	disable the visual bell animation.

	Default: _0_

*color* = _"<string>"_

	Visual bell animation color.

	Default: _"#ffffff"_

*command* = _"<string>"_ | { program = _"<string>"_, args = [_"<string>"_,] }

	This program is executed whenever the bell is rung.

	When set to _"None"_, no command will be executed.

	Default: _"None"_

# SELECTION

This section documents the *[selection]* table of the configuration file.

*semantic_escape_chars* = _"<string>"_

	This string contains all characters that are used as separators for
	"semantic words" in TankTerm.

	Default: _",│`|:\\"' ()[]{}<>\\t"_

*save_to_clipboard* = _true_ | _false_

	When set to _true_, selected text will be copied to the primary clipboard.
	Selected text is always put on the selection clipboard, which the middle
	mouse button and _Shift+Insert_ paste, where the platform has one.

	Default: _false_

# CURSOR

This section documents the *[cursor]* table of the configuration file.

*style* = { *<shape>*, *<blinking>* }

	*shape* = _"Block"_ | _"Underline"_ | _"Beam"_

		Default: _"Block"_

	*blinking* = _"Never"_ | _"Off"_ | _"On"_ | _"Always"_

		*Never*
			Prevent the cursor from ever blinking
		*Off*
			Disable blinking by default
		*On*
			Enable blinking by default
		*Always*
			Force the cursor to always blink

		Default: _"Off"_

*vi_mode_style* = { *<shape>*, *<blinking>* } | _"None"_

	If the vi mode cursor style is _"None"_ or not specified, it will fall back to
	the active value of the normal cursor.

	Default: _"None"_

*blink_interval* = _<integer>_

	Cursor blinking interval in milliseconds.

	Default: _750_

*blink_timeout* = _<integer>_

	Time after which cursor stops blinking, in seconds.

	Specifying _0_ will disable timeout for blinking.

	Default: _5_

*unfocused_hollow* = _true_ | _false_

	When this is _true_, the cursor will be rendered as a hollow box when the
	window is not focused.

	Default: _true_

*thickness* = _<float>_

	Thickness of the cursor relative to the cell width as floating point number
	from _0.0_ to _1.0_.

	Default: _0.15_

# TERMINAL

This section documents the *[terminal]* table of the configuration file.

*shell* = _"<string>"_ | { program = _"<string>"_, args = [_"<string>"_,] }

	You can set _shell.program_ to the path of your favorite shell, e.g.
	_/bin/zsh_. Entries in _shell.args_ are passed as arguments to the shell.

	Default:
		Linux/BSD/macOS: _$SHELL_ or the user's login shell, if _$SHELL_ is unset++
Windows: _"powershell"_

	Example:
		*shell* = { program = _"/bin/zsh"_, args = [_"-l"_] }

*osc52* = _"Disabled"_ | _"OnlyCopy"_ | _"OnlyPaste"_ | _"CopyPaste"_

	Controls the ability to write to the system clipboard with the _OSC 52_
	escape sequence. While this escape sequence is useful to copy contents
	from the remote server, allowing any application to read from the clipboard
	can be easily abused while not providing significant benefits over
	explicitly pasting text.

	Default: _"OnlyCopy"_

*confirm_multiline_paste* = _true_ | _false_

	Ask before pasting text with line breaks into a program that didn't turn on
	bracketed paste. Such a program takes every line break as the _Enter_ key,
	so a shell would run the pasted lines one by one. The question shows the
	text and is answered with _Enter_ to paste or _Escape_ to cancel.

	Programs with bracketed paste, like most interactive shells and editors,
	get the text as one block and aren't asked about.

	Default: _true_

# MOUSE

This section documents the *[mouse]* table of the configuration file.

*hide_when_typing* = _true_ | _false_

	When this is _true_, the cursor is temporarily hidden when typing.

	Default: _false_

*bindings* = [{ *<mouse>*, *<mods>*, *<mode>*, *<command>* | *<chars>* | *<action>* },]

	See _keyboard.bindings_ for full documentation on _mods_, _mode_, _command_,
	_chars_, and _action_.

	When an application running within TankTerm captures the mouse, the `Shift`
	modifier can be used to suppress mouse reporting. If no action is found for
	the event, actions for the event without the `Shift` modifier are triggered
	instead.

	*mouse* = _"Middle"_ | _"Left"_ | _"Right"_ | _"Back"_ | _"Forward"_ | _"WheelUp"_ | _"WheelDown"_ | _<integer>_

		Mouse button which needs to be pressed to trigger this binding.

	*action* = *<keyboard.bindings.action>* | _"ExpandSelection"_

		*ExpandSelection*
			Expand the selection to the current mouse cursor location.

	Of the mouse bindings, only those of the middle and right buttons to the
	_Paste_ and _PasteSelection_ actions have an effect so far; a _mode_ isn't
	supported for them. The middle button pastes the selection by default.
	Binding either button to another action keeps it from pasting.

	Example:
		*[mouse]*++
bindings = [++
	{ mouse = _"Right"_, action = _"Paste"_ },++
]

# HINTS

This section documents the *[hints]* table of the configuration file.

Terminal hints can be used to find text or hyperlinks in the visible part of the
terminal and pipe it to other applications.

*alphabet* = _"<string>"_

	Keys used for the hint labels.

	Default: _"jfkdls;ahgurieowpq"_

*enabled* = [{ *<regex>*, *<hyperlinks>*, *<post_processing>*, *<persist>*, *<action>*, *<command>*, *<binding>*, *<mouse>* },]

Array with all available hints.

Each hint must have at least one of _regex_ or _hyperlinks_ and either an
_action_ or a _command_.

	*regex* = _"<string>"_

		Regex each line will be compared against.

	*hyperlinks* = _true_ | _false_

		When this is _true_, all OSC 8 escape sequence hyperlinks will be
		included in the hints.

	*post_processing* = _true_ | _false_

		When this is _true_, heuristics will be used to shorten the match if
		there are characters likely not to be part of the hint (e.g. a trailing
		_._). This is most useful for URIs and applies only to _regex_ matches.

	*persist* = _true_ | _false_

		When this is _true_, hints remain persistent after selection.

	*action* = _"Copy"_ | _"Paste"_ | _"Select"_ | _"MoveViModeCursor"_

		*Copy*
			Copy the hint's text to the clipboard.
		*Paste*
			Paste the hint's text to the terminal or search.
		*Select*
			Select the hint's text.
		*MoveViModeCursor*
			Move the vi mode cursor to the beginning of the hint.

	*command* = _"<string>"_ | { program = _"<string>"_, args = [_"<string>"_,] }

		Command which will be executed when the hint is clicked or selected with
		the _binding_.

		The hint's text is always attached as the last argument.

	*binding* = { key = _"<string>"_, mods = _"<string>"_, mode = _"<string>"_ }

		See _keyboard.bindings_ for documentation on available values.

		This controls which key binding is used to start the keyboard hint
		selection process.

	*mouse* = { mods = _"<string>"_, enabled = _true_ | _false_ }

		See _keyboard.bindings_ for documentation on available _mods_.

		The _enabled_ field controls if the hint should be underlined when
		hovering over the hint text with all _mods_ pressed.

	Default:
		*[[hints.enabled]]*++
command         = _"xdg-open"_ # On Linux/BSD++
# command       = _"open"_ # On macOS++
# command       = { program = _"cmd"_, args = [ _"/c"_, _"start"_, _""_ ] } # On Windows++
hyperlinks      = _true_++
post_processing = _true_++
persist         = _false_++
mouse.enabled   = _true_++
binding         = { key = _"O"_, mods = _"Control|Shift"_ }++
regex =

		```
		"(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)[^\\u0000-\\u001F\\u007F-\\u009F<>\\"\\\\s{-}\\\\^⟨⟩`\\\\\\\\]+"
		```

# KEYBOARD

This section documents the *[keyboard]* table of the configuration file.

*preset* = _"Tabby"_ | _"WezTerm"_ | _"Leader"_

	Which default keys tabs and panes get. The keys of every preset are listed
	in *tankTerm-bindings*(5). Entries in _bindings_ replace and unset them like
	any other default binding.

	*Tabby*
		Keys like Tabby's: _Alt_ and a digit selects a tab, _Control|Alt_ and an
		arrow key moves between panes. _Control|Shift_ and the arrow keys are
		left to the applications in the terminal.
	*WezTerm*
		Keys like WezTerm's, all on _Control|Shift_: with a digit to select a
		tab and with an arrow key to move between panes. _Alt_ and the digits
		are left to the applications in the terminal.
	*Leader*
		Keys like tmux's: only opening a tab and splitting have keys of their
		own. Everything else is reached by pressing the leader key and then a
		second key.

	On macOS the _Tabby_ and _WezTerm_ presets have the same _Command_ keys.

	Default: _"Tabby"_

*leader* = { key = _"<string>"_, mods = _"<string>"_, timeout = _<integer>_ }

	The leader key: the key pressed before the key of a binding with the
	_Leader_ mode. Neither the leader key nor the key after it reach the shell.
	A key without a _Leader_ binding ends the sequence, as does waiting longer
	than _timeout_ milliseconds for the second key. Pressing the leader key
	twice sends it to the shell once.

	_key_ and _mods_ are written like in _bindings_; _mods_ and _timeout_ are
	optional. While there are no bindings with the _Leader_ mode, the leader key
	is an ordinary key.

	Default: { key = _"A"_, mods = _"Control"_, timeout = _1000_ }

*bindings* = [{ *<key>*, *<mods>*, *<mode>*, *<command>* | *<chars>* | *<action>* },]

	To unset a default binding, you can use the action _"ReceiveChar"_ to remove
	it or _"None"_ to inhibit any action.

	Multiple keybindings can be triggered by a single key press and will be
	executed in the order they are defined in.

	*key* = _"<string>"_

		The regular keys like _"A"_, _"0"_, and _"Я"_ can be mapped directly
		without any special syntax. Full list of named keys like _"F1"_ and the
		syntax for dead keys (the _Dead_ variant of _Key_) can be found here:

		_https://docs.rs/winit/latest/winit/keyboard/enum.NamedKey.html_++
_https://docs.rs/winit/latest/winit/keyboard/enum.Key.html_

		Numpad keys are prefixed by _Numpad_: _"NumpadEnter"_ | _"NumpadAdd"_ |
		_"NumpadComma"_ | _"NumpadDecimal"_ | _"NumpadDivide"_ | _"NumpadEquals"_ |
		_"NumpadSubtract"_ | _"NumpadMultiply"_ | _"Numpad[0-9]"_.

		The _key_ field also supports using scancodes, which are specified as a
		decimal number.

	*mods* = _"Command"_ | _"Control"_ | _"Option"_ | _"Super"_ | _"Shift"_ | _"Alt"_

		Multiple modifiers can be combined using _|_, like this: _"Control |
		Shift"_.

	*mode* = _"AppCursor"_ | _"AppKeypad"_ | _"Search"_ | _"Alt"_ | _"Vi"_ | _"Leader"_

		This defines a terminal mode which must be active for this binding to
		have an effect.

		_Leader_ isn't a terminal mode: a binding with it takes effect when its
		key is pressed after the leader key (see _leader_). It is for the tab
		and pane actions and can't be combined with other modes.

		Prepending _~_ to a mode will require the mode to *not* = be active for
		the binding to take effect.

		Multiple modes can be combined using _|_, like this: _"~Vi|Search"_.

	*command* = _"<string>"_ | { program = _"<string>"_, args = [_"<string>"_,] }

		Fork and execute the specified command.

	*chars* = _"<string>"_

		Writes the specified string to the terminal.

	*help* = _"<string>"_

		Description shown for this binding in the key help (see _ToggleHelp_),
		like Bubbles' _key.WithHelp_. Common actions have a default description;
		bindings without one aren't listed.

	*action*

		*ReceiveChar*
			Allow receiving char input.
		*None*
			No action.
		*Paste*
			Paste contents of system clipboard.
		*Copy*
			Store current selection into clipboard.
		*IncreaseFontSize*
			Increase font size.
		*DecreaseFontSize*
			Decrease font size.
		*ResetFontSize*
			Reset font size to the config value.
		*ScrollPageUp*
			Scroll exactly one page up.
		*ScrollPageDown*
			Scroll exactly one page down.
		*ScrollHalfPageUp*
			Scroll half a page up.
		*ScrollHalfPageDown*
			Scroll half a page down.
		*ScrollLineUp*
			Scroll one line up.
		*ScrollLineDown*
			Scroll one line down.
		*ScrollToTop*
			Scroll all the way to the top.
		*ScrollToBottom*
			Scroll all the way to the bottom.
		*ClearHistory*
			Clear the display buffer(s) to remove history.
		*Hide*
			Hide the TankTerm window.
		*Minimize*
			Minimize the TankTerm window.
		*Quit*
			Quit TankTerm.
		*ClearLogNotice*
			Clear warning and error notices.
		*SpawnNewInstance*
			Spawn a new instance of TankTerm.
		*CreateNewWindow*
			Create a new TankTerm window.
		*ToggleFullscreen*
			Toggle fullscreen.
		*ToggleMaximized*
			Toggle maximized.
		*ClearSelection*
			Clear active selection.
		*ToggleViMode*
			Toggle vi mode.
		*ToggleHelp*
			Cycle the key help between hidden, a single line and full columns, like
			Charm's Bubbles help view. It's generated from the key bindings.
		*SearchForward*
			Start a forward buffer search.
		*SearchBackward*
			Start a backward buffer search.

		_Vi mode actions:_

		*Up*
			Move up.
		*Down*
			Move down.
		*Left*
			Move left.
		*Right*
			Move right.
		*First*
			First column, or beginning of the line when already at the first column.
		*Last*
			Last column, or beginning of the line when already at the last column.
		*FirstOccupied*
			First non-empty cell in this terminal row, or first non-empty cell
			of the line when already at the first cell of the row.
		*High*
			Move to top of screen.
		*Middle*
			Move to center of screen.
		*Low*
			Move to bottom of screen.
		*SemanticLeft*
			Move to start of semantically separated word.
		*SemanticRight*
			Move to start of next semantically separated word.
		*SemanticLeftEnd*
			Move to end of previous semantically separated word.
		*SemanticRightEnd*
			Move to end of semantically separated word.
		*WordLeft*
			Move to start of whitespace separated word.
		*WordRight*
			Move to start of next whitespace separated word.
		*WordLeftEnd*
			Move to end of previous whitespace separated word.
		*WordRightEnd*
			Move to end of whitespace separated word.
		*Bracket*
			Move to opposing bracket.
		*ParagraphUp*
			Move above the current paragraph.
		*ParagraphDown*
			Move below the current paragraph.
		*ToggleNormalSelection*
			Toggle normal vi selection.
		*ToggleLineSelection*
			Toggle line vi selection.
		*ToggleBlockSelection*
			Toggle block vi selection.
		*ToggleSemanticSelection*
			Toggle semantic vi selection.
		*SearchNext*
			Jump to the beginning of the next match.
		*SearchPrevious*
			Jump to the beginning of the previous match.
		*SearchStart*
			Jump to the next start of a match to the left of the origin.
		*SearchEnd*
			Jump to the next end of a match to the right of the origin.
		*Open*
			Launch the URL below the vi mode cursor.
		*CenterAroundViCursor*
			Centers the screen around the vi mode cursor.
		*InlineSearchForward*
			Search forward within the current line.
		*InlineSearchBackward*
			Search backward within the current line.
		*InlineSearchForwardShort*
			Search forward within the current line, stopping just short of the character.
		*InlineSearchBackwardShort*
			Search backward within the current line, stopping just short of the character.
		*InlineSearchNext*
			Jump to the next inline search match.
		*InlineSearchPrevious*
			Jump to the previous inline search match.
		*SemanticSearchForward*
			Search forward for selection or word under the cursor.
		*SemanticSearchBackward*
			Search backward for selection or word under the cursor.

		_Search actions:_

		*SearchFocusNext*
			Move the focus to the next search match.
		*SearchFocusPrevious*
			Move the focus to the previous search match.
		*SearchConfirm*
			Confirm the active search.
		*SearchCancel*
			Cancel the active search.
		*SearchClear*
			Reset the search regex.
		*SearchDeleteWord*
			Delete the last word in the search regex.
		*SearchHistoryPrevious*
			Go to the previous regex in the search history.
		*SearchHistoryNext*
			Go to the next regex in the search history.

		_Tab and pane actions:_

		*CreateNewTab*
			Open a new tab after the last one.
		*CloseTab*
			Close the current tab and all of its panes.
		*SelectNextTab*
			Select the next tab, or the first one after the last.
		*SelectPreviousTab*
			Select the previous tab, or the last one before the first.
		*SelectTab1*
			Select the first tab.
		*SelectTab2*
			Select the second tab.
		*SelectTab3*
			Select the third tab.
		*SelectTab4*
			Select the fourth tab.
		*SelectTab5*
			Select the fifth tab.
		*SelectTab6*
			Select the sixth tab.
		*SelectTab7*
			Select the seventh tab.
		*SelectTab8*
			Select the eighth tab.
		*SelectTab9*
			Select the ninth tab.
		*SelectLastTab*
			Select the last tab.
		*MoveTabLeft*
			Move the current tab one position to the left.
		*MoveTabRight*
			Move the current tab one position to the right.
		*SplitRight*
			Split the current pane and open a new terminal on its right.
		*SplitDown*
			Split the current pane and open a new terminal below it.
		*ClosePane*
			Close the current pane. Closing the last pane of a tab closes the tab.
		*FocusPaneLeft*
			Focus the pane left of the current one.
		*FocusPaneRight*
			Focus the pane right of the current one.
		*FocusPaneUp*
			Focus the pane above the current one.
		*FocusPaneDown*
			Focus the pane below the current one.
		*FocusNextPane*
			Focus the next pane of the tab, from left to right and top to bottom.
		*FocusPreviousPane*
			Focus the previous pane of the tab.
		*TogglePaneZoom*
			Let the current pane fill the tab, or show the other panes again.
			Focusing another pane or splitting also ends the zoom.
		*ResizePaneLeft*
			Move the nearest divider left or right of the current pane to the left.
		*ResizePaneRight*
			Move the nearest divider left or right of the current pane to the right.
		*ResizePaneUp*
			Move the nearest divider above or below the current pane up.
		*ResizePaneDown*
			Move the nearest divider above or below the current pane down.

		*ShowCommandPalette*
			Show the command palette, a list of the tab and pane actions with their
			key bindings. Typing filters the list, the arrow keys move the selection,
			_Enter_ runs the selected action and _Escape_ closes the list.
		*ShowProfiles*
			Show the profile selector, which opens the regular shell or one of the
			*[[profiles]]* in a new tab. It is used like the command palette.

		Tab and pane bindings can't be limited to a _mode_ other than
		_Leader_.

		_macOS exclusive:_

		*ToggleSimpleFullscreen*
			Enter fullscreen without occupying another space.
		*HideOtherApplications*
			Hide all windows other than TankTerm.

		_Linux/BSD exclusive:_

		*CopySelection*
			Copy from the selection buffer.
		*PasteSelection*
			Paste from the selection buffer.

Default: See *tankTerm-bindings*(5)

Example:
	*[keyboard]*++
bindings = [++
	{ key = _"N"_, mods = _"Control|Shift"_, action = _"CreateNewWindow"_ },++
	{ key = _"L"_, mods = _"Control|Shift"_, chars = _"l"_ },++
]


# DEBUG

This section documents the *[debug]* table of the configuration file.

Log lines follow the layout of Charm's log library: _<time> <LEVEL> <prefix>:
<message>_. On a terminal, the level and prefix are styled unless _$NO\_COLOR_
is set; log files are never styled. Set _$TANKTERM\_LOG\_FORMAT_ to _json_ or
_logfmt_ for structured logs.

Debug options are meant to help troubleshoot issues with TankTerm. These can
change or be removed entirely without warning, so their stability shouldn't be
relied upon.

*render_timer* = _true_ | _false_

	Display the time it takes to draw each frame.

	Default: _false_

*persistent_logging* = _true_ | _false_

	Keep the log file after quitting TankTerm.

	Default: _false_

*log_level* = _"Off"_ | _"Error"_ | _"Warn"_ | _"Info"_ | _"Debug"_ | _"Trace"_

	Default: _"Warn"_

	To add extra libraries to logging _TANKTERM_EXTRA_LOG_TARGETS_ variable
	can be used.

	Example:
		_TANKTERM_EXTRA_LOG_TARGETS="winit;vte" tankTerm -vvv_

*renderer* = _"glsl3"_ | _"gles2"_ | _"gles2pure"_ | _"None"_

	Force use of a specific renderer, _"None"_ will use the highest available
	one.

	Default: _"None"_

*print_events* = _true_ | _false_

	Log all received window events.

	Default: _false_

*highlight_damage* = _true_ | _false_

	Highlight window damage information.

	Default: _false_

*prefer_egl* = _true_ | _false_

	Use EGL as display API if the current platform allows it. Note that
	transparency may not work with EGL on Linux/BSD.

	Default: _false_

# SEE ALSO

*tankTerm*(1), *tankTerm-msg*(1), *tankTerm-bindings*(5), *tankTerm-escapes*(7)

# BUGS

Found a bug? Please report it at _https://git.infra.xitely.net/xitely/TankTerm/issues_.

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
