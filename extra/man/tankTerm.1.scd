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

	Remain open after child process exits.

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

*migrate*

	Migrate the configuration file.

	*-c, --config-file* _<CONFIG_FILE>_

		Path to the configuration file.

	*-d, --dry-run*

		Only output TOML config to STDOUT.

	*-i, --skip-imports*

		Do not recurse over imports.

	*--skip-renames*

		Do not move renamed fields to their new location.

	*-s, --silent*

		Do not output to STDOUT.

	*-h, --help*

		Print help information.

# SEE ALSO

*tankTerm-msg*(1), *tankTerm*(5), *tankTerm-bindings*(5), *tankTerm-escapes*(7)

# BUGS

Found a bug? Please report it at _https://git.infra.xitely.net/xitely/TankTerm/issues_.
