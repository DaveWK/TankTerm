# TankTerm starts fish with its shell integration directory first in XDG_DATA_DIRS, so that fish
# reads this file with the other configuration snippets. It takes the directory out of
# XDG_DATA_DIRS again, so that programs started from fish don't see it, and loads the shell
# integration.

if set -q TANKTERM_SHELL_INTEGRATION_DIR
    set -l dirs (string split : -- "$XDG_DATA_DIRS")
    set -l rest (string match -v -- $TANKTERM_SHELL_INTEGRATION_DIR $dirs)
    if set -q TANKTERM_FISH_UNSET_XDG_DATA_DIRS
        and test "$rest" = '/usr/local/share /usr/share'
        set -e XDG_DATA_DIRS
    else
        set -gx XDG_DATA_DIRS (string join : -- $rest)
    end
    set -e TANKTERM_FISH_UNSET_XDG_DATA_DIRS

    source $TANKTERM_SHELL_INTEGRATION_DIR/tankterm.fish
end
