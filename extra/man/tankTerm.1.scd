TANKTERM(1)

# NAME

TankTerm - A cross-platform terminal emulator with tabs and split panes.

# SYNOPSIS

TankTerm is a terminal emulator with tabs, split panes, a command palette and
profiles, set up through a TOML configuration file; see *tankTerm*(5).

# FLAGS

*-h, --help*

	Prints help information.

*--hold*

	Keep the first terminal open, with what it shows, after its program
	exits. Tabs and panes opened later close when their shell exits, as
	usual.

*--daemon*

	Do not spawn an initial window.

*--theme* _<name>_ | _<path>_

	Theme to use, by built-in name or file path; see *tankTerm*(5). Overrides
	the configuration's _theme_ and can also be set with _$TANKTERM\_THEME_.

*-q*

	Reduces the level of verbosity (the min level is *-qq*).

*-v*

	Increases the level of verbosity (the max level is *-vvv*).

*-V, --version*

	Prints version information.

# OPTIONS

*--class* _<GENERAL>_ | _<GENERAL>_,_<INSTANCE>_

	Defines the window class hint on Linux.

	When only the general class is passed, instance will be set to the same value.

	On Wayland the general class sets the _app\_id_, while the instance class is ignored.

	Default: _TankTerm,TankTerm_

*-e, --command* _<COMMAND>..._

	Command and args to execute (must be last argument).

*--config-file* _<CONFIG_FILE>_

	Specify alternative configuration file.

	TankTerm doesn't create the config file for you, but it looks for one in the
	following locations on UNIX systems:

		. _$XDG_CONFIG_HOME/tankterm/tankterm.toml_
		. _$XDG_CONFIG_HOME/tankterm.toml_
		. _$HOME/.config/tankterm/tankterm.toml_
		. _$HOME/.tankterm.toml_
		. _/etc/tankterm/tankterm.toml_

	On Windows, the config file will be looked for in:

		. _%APPDATA%\\tankterm\\tankterm.toml_

*-o, --option* _<OPTION>..._

	Override configuration file options.

	Example: _tankTerm -o 'cursor.style="Beam"'_

*--socket* _<SOCKET>_

	Path for IPC socket creation.

*-T, --title* _<TITLE>_

	Defines the window title.

	Default: _TankTerm_

*--working-directory* _<WORKING_DIRECTORY>_

	Start the shell in the specified working directory.

# SUBCOMMANDS

*msg*

	Send IPC socket messages (see *tankTerm-msg*(1)).

*ssh* [*--probe*|*--no-probe*] [*--reprobe*] _<SSH_ARGS>_...

	Run *ssh*(1) with a _TERM_ the server has a terminfo entry for (see
	*tankTerm-ssh*(1)). Not available on Windows.

*migrate*

	Convert another terminal's configuration into TankTerm's. The source files
	are never changed. The result is written to
	_$XDG_CONFIG_HOME/tankterm/tankterm.toml_ (_%APPDATA%\\tankterm\\tankterm.toml_
	on Windows), with converted imports next to it, and a summary lists what
	was converted and what was skipped, with the reason.

	If any of these files exists, migrate lists them and asks before replacing
	them; each is copied to _<name>.bak_ first (_<name>.bak.1_ and so on if that
	exists). Without a terminal to ask on, it writes nothing unless *--force* is
	given.

	Without *--from* and *--config-file*, migrate looks in the standard
	locations of every source. If it finds one configuration it converts it; if
	it finds several it asks which one, or without a terminal names them and
	stops.

	TankTerm doesn't convert anything by itself. When it starts without a
	configuration file and finds one of these, it logs a line suggesting
	*tankTerm migrate*.

	Sources:

	_alacritty_
		_alacritty.toml_ or the older _alacritty.yml_, in Alacritty's order
		of locations. The options are the same, so they are copied with
		Alacritty's renames applied, and the options and key bindings
		TankTerm doesn't have left out. Imports are converted too, into the same places relative
		to TankTerm's configuration directory, or into _imports/_ for files
		outside Alacritty's.

	_tabby_
		_config.yaml_: the color scheme in use, font, cursor, bell,
		opacity and blur, local and SSH profiles, and the hotkeys whose
		actions TankTerm has. The _Tabby_ key preset is selected, and only
		keys that differ from it are written.

	_wezterm_
		_wezterm.lua_, read without running it: literal assignments to the
		configuration table and a returned table, _wezterm.font_ and
		_wezterm.font_with_fallback_, _colors_, color schemes defined in the
		file or in a _colors/_ directory, window settings, the launch menu,
		SSH domains, the leader key, _keys_ and _key_tables_. The _WezTerm_
		key preset is selected. Anything computed or conditional is skipped
		with its line.

	_ghostty_
		_config_ and _config.ghostty_ with their _config-file_ includes:
		fonts, colors and _palette_, _theme_ (from Ghostty's theme
		directories, with _light:_/_dark:_ themes as light/dark pairs),
		window settings, _command_ and _keybind_ lines.

	_crush_
		The theme Crush uses: _options.tui.active_theme_ in Crush's
		_crush.json_ files (_$XDG_CONFIG_HOME/crush/_, and
		_~/.local/share/crush/_, where its Themes dialog saves it) and the
		theme's file in _$XDG_CONFIG_HOME/crush/themes/_, or a theme file
		given with *--config-file*. Its base colors and 16 ANSI colors are
		converted; Crush's default theme, which themes without a _base_
		inherit from, becomes TankTerm's _charm_ theme. Nothing else of
		these files is read, and _crushrc_ has no theme setting.

	*--from* _<SOURCE>_

		The terminal to convert from: _alacritty_, _tabby_, _wezterm_,
		_ghostty_ or _crush_.

	*-c, --config-file* _<CONFIG_FILE>_

		The configuration file to convert. Its source is told from its name
		and contents, unless *--from* names it.

	*-f, --force*

		Replace existing TankTerm files without asking, after backing them up.

	*-d, --dry-run*

		Print the files that would be written and the summary, and write
		nothing.

	*-i, --skip-imports*

		Keep Alacritty imports pointing at the original files instead of
		converting them.

	*-s, --silent*

		Do not output to STDOUT.

	*-h, --help*

		Print help information.

# SHELL INTEGRATION

Shells can mark their prompts, command lines and command output (OSC 133) and
report their working directory (OSC 7). On Linux, BSD and macOS, TankTerm
starts bash, zsh and fish with scripts that do this, unless
*terminal.shell_integration* in *tankTerm*(5) is _"None"_. The shells still
read their own startup files.

The scripts are written to
_$XDG_RUNTIME_DIR/tankterm/shell-integration/<version>_, or to a directory in
the temporary directory without a runtime directory, and the shells find it in
*TANKTERM_SHELL_INTEGRATION_DIR*. Shells TankTerm doesn't start that way, such
as a shell started from another one, can source _tankterm.bash_,
_tankterm.zsh_ or _tankterm.fish_ from there, or from _extra/shell-integration_
of the source code, at the end of their startup file. fish 4 marks its prompts
itself. The bash script needs bash 4.4 to mark commands, and isn't loaded into
the bash 3.2 of macOS.

TankTerm then marks where every prompt starts at the left edge of the terminal,
in red when the command typed at it failed. The _ScrollToPreviousPrompt_,
_ScrollToNextPrompt_ and _SelectCommandOutput_ actions of *tankTerm*(5) move
between prompts and select a command's output; see *tankTerm-bindings*(5) for
their keys. Finished commands are noticed from the marks, and new tabs and panes
start in the reported directory; without the marks, TankTerm watches the
shell's processes for both, where it can.

# SEE ALSO

*tankTerm-msg*(1), *tankTerm-ssh*(1), *tankTerm*(5), *tankTerm-bindings*(5),
*tankTerm-escapes*(7)

# BUGS

Found a bug? Please report it at _https://github.com/DaveWK/TankTerm/issues_.
