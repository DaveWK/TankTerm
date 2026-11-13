# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_tankTerm_global_optspecs
    string join \n config-file= socket= q v daemon theme= working-directory= hold e/command= T/title= class= o/option= h/help V/version
end

function __fish_tankTerm_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_tankTerm_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_tankTerm_using_subcommand
    set -l cmd (__fish_tankTerm_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c tankTerm -n "__fish_tankTerm_needs_command" -l config-file -d 'Specify alternative configuration file [default: $XDG_CONFIG_HOME/tankterm/tankterm.toml]' -r -F
complete -c tankTerm -n "__fish_tankTerm_needs_command" -l socket -d 'Path for IPC socket creation' -r -F
complete -c tankTerm -n "__fish_tankTerm_needs_command" -l theme -d 'Theme to use, by built-in name or file path. Overrides the configuration\'s theme' -r
complete -c tankTerm -n "__fish_tankTerm_needs_command" -l working-directory -d 'Start the shell in the specified working directory' -r -F
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s e -l command -d 'Command and args to execute (must be last argument)' -r
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s T -l title -d 'Defines the window title [default: TankTerm]' -r
complete -c tankTerm -n "__fish_tankTerm_needs_command" -l class -d 'Defines window class/app_id on X11/Wayland [default: TankTerm]' -r
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s o -l option -d 'Override configuration file options [example: \'cursor.style="Beam"\']' -r
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s q -d 'Reduces the level of verbosity (the min level is -qq)'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s v -d 'Increases the level of verbosity (the max level is -vvv)'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -l daemon -d 'Do not spawn an initial window'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -l hold -d 'Remain open after child process exit'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -s V -l version -d 'Print version'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -f -a "msg" -d 'Send a message to the TankTerm socket'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -f -a "migrate" -d 'Migrate the configuration file'
complete -c tankTerm -n "__fish_tankTerm_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -s s -l socket -d 'IPC socket connection path override' -r -F
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "list" -d 'List the tabs and their panes'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "spawn" -d 'Open a new tab and print the ID of its pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "split-pane" -d 'Split a pane and print the ID of the new pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "send-text" -d 'Send text to a pane, as if it was pasted'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "get-text" -d 'Print the text of a pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "activate-pane" -d 'Focus a pane and show its tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "activate-tab" -d 'Show a tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "kill-pane" -d 'Close a pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "zoom-pane" -d 'Zoom a pane, so that it fills its tab, or unzoom it'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "set-tab-title" -d 'Name a tab, in place of the title of its focused pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "create-window" -d 'Create a new window in the same TankTerm process (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "config" -d 'Update the TankTerm configuration (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "get-config" -d 'Read runtime TankTerm configuration (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and not __fish_seen_subcommand_from list spawn split-pane send-text get-text activate-pane activate-tab kill-pane zoom-pane set-tab-title create-window config get-config help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from list" -l format -d 'How to print the panes' -r -f -a "table\t'A table, one pane per line'
json\t''"
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from spawn" -l profile -d 'Open the tab from this profile' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from spawn" -l cwd -d 'Start the program in this directory [default: as for a new tab or pane]' -r -f -a "(__fish_complete_directories)"
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from spawn" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l pane-id -d 'Pane to split [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l percent -d 'Share of the space the new pane gets, in percent [default: 50]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l cwd -d 'Start the program in this directory [default: as for a new tab or pane]' -r -f -a "(__fish_complete_directories)"
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l horizontal -d 'The same as `--right`'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l left -d 'Put the new pane left of the pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l right -d 'Put the new pane right of the pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l top -d 'Put the new pane above the pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -l bottom -d 'Put the new pane below the pane (the default)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from split-pane" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from send-text" -l pane-id -d 'Pane to send the text to [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from send-text" -l no-paste -d 'Send the text as it is, instead of pasting it'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from send-text" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from get-text" -l pane-id -d 'Pane to read [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from get-text" -l start-line -d 'First line to print: 0 is the top of the screen, negative lines are scrollback [default: 0]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from get-text" -l end-line -d 'Last line to print [default: the bottom of the screen]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from get-text" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-pane" -l pane-id -d 'The pane [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-pane" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-tab" -l tab-id -d 'The tab with this ID' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-tab" -l tab-index -d 'The tab at this position, counting from 0; negative positions count from the last tab, which is -1' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-tab" -l tab-relative -d 'The tab this many tabs right of the shown one, or left of it for negative numbers, wrapping around at the ends' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-tab" -l no-wrap -d 'Stop at the first or last tab instead of wrapping around'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from activate-tab" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from kill-pane" -l pane-id -d 'The pane [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from kill-pane" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from zoom-pane" -l pane-id -d 'The pane [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from zoom-pane" -l zoom -d 'Zoom the pane (the default)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from zoom-pane" -l unzoom -d 'Unzoom the pane\'s tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from zoom-pane" -l toggle -d 'Zoom the pane, or unzoom its tab if it is zoomed'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from zoom-pane" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from set-tab-title" -l pane-id -d 'A pane of the tab [default: $TANKTERM_PANE_ID, or the focused pane]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from set-tab-title" -l tab-id -d 'The tab with this ID, in place of a pane\'s tab' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from set-tab-title" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -l working-directory -d 'Start the shell in the specified working directory' -r -F
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s e -l command -d 'Command and args to execute (must be last argument)' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s T -l title -d 'Defines the window title [default: TankTerm]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -l class -d 'Defines window class/app_id on X11/Wayland [default: TankTerm]' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s o -l option -d 'Override configuration file options [example: \'cursor.style="Beam"\']' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -l hold -d 'Remain open after child process exit'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from create-window" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from config" -s w -l window-id -d 'Window ID for the new config' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from config" -s r -l reset -d 'Clear all runtime configuration changes'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from config" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from get-config" -s w -l window-id -d 'Window ID for the config request' -r
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from get-config" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "list" -d 'List the tabs and their panes'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "spawn" -d 'Open a new tab and print the ID of its pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "split-pane" -d 'Split a pane and print the ID of the new pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "send-text" -d 'Send text to a pane, as if it was pasted'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "get-text" -d 'Print the text of a pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "activate-pane" -d 'Focus a pane and show its tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "activate-tab" -d 'Show a tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "kill-pane" -d 'Close a pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "zoom-pane" -d 'Zoom a pane, so that it fills its tab, or unzoom it'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "set-tab-title" -d 'Name a tab, in place of the title of its focused pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "create-window" -d 'Create a new window in the same TankTerm process (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "config" -d 'Update the TankTerm configuration (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "get-config" -d 'Read runtime TankTerm configuration (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand msg; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand migrate" -s c -l config-file -d 'Path to the configuration file' -r -F
complete -c tankTerm -n "__fish_tankTerm_using_subcommand migrate" -s d -l dry-run -d 'Only output TOML config to STDOUT'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand migrate" -s i -l skip-imports -d 'Do not recurse over imports'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand migrate" -l skip-renames -d 'Do not move renamed fields to their new location'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand migrate" -s s -l silent -d 'Do not output to STDOUT'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand migrate" -s h -l help -d 'Print help'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and not __fish_seen_subcommand_from msg migrate help" -f -a "msg" -d 'Send a message to the TankTerm socket'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and not __fish_seen_subcommand_from msg migrate help" -f -a "migrate" -d 'Migrate the configuration file'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and not __fish_seen_subcommand_from msg migrate help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "list" -d 'List the tabs and their panes'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "spawn" -d 'Open a new tab and print the ID of its pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "split-pane" -d 'Split a pane and print the ID of the new pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "send-text" -d 'Send text to a pane, as if it was pasted'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "get-text" -d 'Print the text of a pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "activate-pane" -d 'Focus a pane and show its tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "activate-tab" -d 'Show a tab'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "kill-pane" -d 'Close a pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "zoom-pane" -d 'Zoom a pane, so that it fills its tab, or unzoom it'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "set-tab-title" -d 'Name a tab, in place of the title of its focused pane'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "create-window" -d 'Create a new window in the same TankTerm process (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "config" -d 'Update the TankTerm configuration (not supported yet)'
complete -c tankTerm -n "__fish_tankTerm_using_subcommand help; and __fish_seen_subcommand_from msg" -f -a "get-config" -d 'Read runtime TankTerm configuration (not supported yet)'
