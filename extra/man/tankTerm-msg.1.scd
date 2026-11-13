TANKTERM-MSG(1)

# NAME

tankTerm-msg - Send messages to TankTerm.

# SYNOPSIS

This command communicates with running TankTerm instances through a socket,
making it possible to control TankTerm without directly accessing it. The
commands that act on tabs and panes follow *wezterm cli*: they take the same
names and options, and *list* prints the same fields.

Every shell TankTerm starts finds the socket in *$TANKTERM_SOCKET* and the ID
of its own pane in *$TANKTERM_PANE_ID*. Commands that act on a pane act on the
pane in *$TANKTERM_PANE_ID* unless they are given *--pane-id*, and on the
focused pane of the tab that is shown without either. Pane and tab IDs stay the
same while the pane or tab is open.

Without *--socket*, the socket in *$TANKTERM_SOCKET* is used, or else the first
one in the runtime directory that still answers. *general.ipc_socket* turns
the socket off (see *tankTerm*(5)). Errors are printed and end *tankTerm msg*
with exit status 1.

# OPTIONS

*-s, --socket* _<SOCKET>_

	Path for IPC socket communication.

# MESSAGES

*list* [*--format* _table_|_json_]

	List the panes of all tabs, tab by tab. The table shows the tab and pane
	IDs, the size in cells, the title and the working directory. JSON adds
	the tab position (*tab_index*), the tab title, and whether the pane is
	focused in its tab (*is_active*), zoomed (*is_zoomed*) and in the tab that
	is shown (*is_active_tab*). *window_id* is always 0. The working directory
	is a path, and only known on Linux.

*spawn* [*--profile* _<NAME>_] [*--cwd* _<DIR>_] [_<PROGRAM>_ [_<ARGS>_...]]

	Open a new tab, show it and print the ID of its pane. It runs _PROGRAM_,
	or the shell, in _DIR_, or where a new tab would start. With *--profile*,
	the tab is opened from that profile.

	Example: _tankTerm msg spawn --cwd ~/src -- htop_

*split-pane* [*--pane-id* _<ID>_] [*--left*|*--right*|*--top*|*--bottom*|*--horizontal*] [*--percent* _<N>_] [*--cwd* _<DIR>_] [_<PROGRAM>_ [_<ARGS>_...]]

	Split a pane, focus the new pane and print its ID. The new pane goes
	below the pane unless another side is given; *--horizontal* is
	*--right*. It gets _N_ percent of the space, half by default.

*send-text* [*--pane-id* _<ID>_] [*--no-paste*] [_<TEXT>_]

	Send _TEXT_, or what is read from standard input, to a pane as if it was
	pasted: as a bracketed paste if the program in the pane asked for one.
	With *--no-paste* the text is sent as it is, like typing it.

	Example: _tankTerm msg send-text --no-paste $'make\\n'_

*get-text* [*--pane-id* _<ID>_] [*--start-line* _<N>_] [*--end-line* _<N>_]

	Print the lines of a pane from _start_ to _end_, both included. Line 0 is
	the top of the screen and negative lines are scrollback; by default the
	screen is printed. Lines a long line wrapped across are printed joined.

*activate-pane* [*--pane-id* _<ID>_]

	Show a pane's tab and focus the pane. A zoomed tab is unzoomed if
	another pane of it is zoomed.

*activate-tab* *--tab-id* _<ID>_ | *--tab-index* _<N>_ | *--tab-relative* _<N>_ [*--no-wrap*]

	Show a tab: the one with that ID, the one at that position (0 is the
	first tab, -1 the last), or the one _N_ tabs right of the shown one (left
	for negative _N_), wrapping around unless *--no-wrap* is given.

*kill-pane* [*--pane-id* _<ID>_]

	Close a pane, and its tab if it was the last pane. The window closes with
	the last tab.

*zoom-pane* [*--pane-id* _<ID>_] [*--zoom*|*--unzoom*|*--toggle*]

	Zoom a pane so that it fills its tab and focus it (the default), unzoom
	its tab, or zoom the pane unless it is zoomed already.

*set-tab-title* [*--pane-id* _<ID>_|*--tab-id* _<ID>_] _<TITLE>_

	Show _TITLE_ for a pane's tab, or the tab with that ID, in place of the
	title of its focused pane. An empty title takes it away again.

The following messages are not supported by the iced front end yet and end
with an error.

*create-window*

	Create a new window in the same TankTerm process.

	*FLAGS*
		*--hold*

			Remain open after child process exits.

	*OPTIONS*
		*--working-directory* _<WORKING_DIRECTORY>_

			Start the shell in the specified working directory.

		*-T, --title* _<TITLE>_

			Defines the window title.

			Default: _TankTerm_

		*--class* _<GENERAL>_ | _<GENERAL>_,_<INSTANCE>_

			Defines window class/app_id on X11/Wayland.

			Default: _TankTerm,TankTerm_

		*-o, --option* _<OPTION>..._

			Override configuration file options.

			Example: _tankTerm msg create-window -o 'cursor.style="Beam"'_

		*-e, --command* _<COMMAND>..._

			Command and args to execute (must be last argument).

*config*

	Update the TankTerm configuration.

	*ARGS*
		*<CONFIG_OPTIONS>...*

			Configuration file options.

			Example: _tankTerm msg config 'cursor.style="Beam"'_

	*FLAGS*
		*-r, --reset*

			Clear all runtime configuration changes.

	*OPTIONS*
		*-w, --window-id* _<WINDOW_ID>_

			Window ID for the new config.

			Use _-1_ to apply this change to all windows.

			Default: _$TANKTERM_WINDOW_ID_

*get-config*

	Read runtime TankTerm configuration.

	*OPTIONS*
		*-w, --window-id* _<WINDOW_ID>_

			Window ID for the config request.

			Use _-1_ to get the global config.

			Default: _$TANKTERM_WINDOW_ID_

# SEE ALSO

*tankTerm*(1), *tankTerm*(5), *tankTerm-bindings*(5), *tankTerm-escapes*(7)

# BUGS

Found a bug? Please report it at _https://git.infra.xitely.net/xitely/TankTerm/issues_.
