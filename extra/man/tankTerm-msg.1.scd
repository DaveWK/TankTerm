TANKTERM-MSG(1)

# NAME

tankTerm-msg - Send messages to TankTerm.

# SYNOPSIS

This command communicates with running TankTerm instances through a socket,
making it possible to control TankTerm without directly accessing it.

# OPTIONS

*-s, --socket* _<SOCKET>_

	Path for IPC socket communication.

# MESSAGES

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

# MAINTAINERS

- Christian Duerr <contact@christianduerr.com>
- Kirill Chibisov <contact@kchibisov.com>
