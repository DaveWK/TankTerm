# TankTerm shell integration for fish.
#
# Marks prompts and commands (OSC 133) and reports the working directory (OSC 7), so TankTerm
# can jump between prompts, select a command's output, mark prompts and failed commands, notice
# finished commands and open new tabs in the right directory.
#
# Source it at the end of ~/.config/fish/config.fish, after anything else that sets the prompt:
#
#     source /path/to/tankterm.fish

status is-interactive; or return 0
test "$TERM" != dumb; or return 0
set -q _tankterm_loaded; and return 0
set -g _tankterm_loaded 1

function _tankterm_directory --on-variable PWD
    printf '\e]7;file://%s%s\a' $hostname (string escape --style=url -- $PWD)
end

function _tankterm_prompt_start --on-event fish_prompt
    printf '\e]133;A\a'
end

function _tankterm_output_start --on-event fish_preexec
    printf '\e]133;C\a'
end

function _tankterm_command_end --on-event fish_postexec
    printf '\e]133;D;%s\a' $status
end

_tankterm_directory
