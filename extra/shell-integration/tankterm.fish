# TankTerm shell integration for fish.
#
# Marks prompts and commands (OSC 133) and reports the working directory (OSC 7), so TankTerm
# can jump between prompts, select a command's output, mark prompts and failed commands, notice
# finished commands and open new tabs in the right directory.
#
# TankTerm loads it into the fish it starts. Elsewhere, source it in ~/.config/fish/config.fish:
#
#     source /path/to/tankterm.fish
#
# fish 4 marks its prompts and commands itself; this only reports the directory there.

status is-interactive; or return 0
test "$TERM" != dumb; or return 0
set -q _tankterm_loaded; and return 0
set -g _tankterm_loaded 1

function _tankterm_directory --on-variable PWD
    printf '\e]7;file://%s%s\a' $hostname (string escape --style=url -- $PWD)
end

_tankterm_directory

string match -qr '^[0-3]\.' -- $version; or return 0

# Whether the function $argv[1] calls the copy $argv[2] of what it was: whether it is wrapped.
function _tankterm_wraps -a name copy
    string match -q "*$copy*" -- (functions $name 2> /dev/null)
end

# Wrap fish_mode_prompt and fish_prompt, so that the marks are part of the prompt and come back
# when fish draws it again, such as after a resize: the start before the mode prompt, or before
# the prompt without one, and the end after the prompt.
#
# A prompt is wrapped again when it was defined anew since, by a theme or by hand. The prompts
# run in a command substitution first, before anything else, so they still see the $status and
# $pipestatus of the command before.
function _tankterm_wrap_prompts
    # fish 3.0 has no `string collect` to keep the output of a prompt in one piece.
    string match -qr '^3\.0\.' -- $version; and return 1

    if functions -q fish_mode_prompt
        and not _tankterm_wraps fish_mode_prompt _tankterm_original_mode_prompt
        functions -e _tankterm_original_mode_prompt
        functions -c fish_mode_prompt _tankterm_original_mode_prompt
        function fish_mode_prompt
            set -l prompt (_tankterm_original_mode_prompt | string collect -N)
            printf '\e]133;A\a%s' $prompt
        end
    end

    functions -q fish_prompt; or return 1
    if not _tankterm_wraps fish_prompt _tankterm_original_prompt
        functions -e _tankterm_original_prompt
        functions -c fish_prompt _tankterm_original_prompt
        function fish_prompt
            set -l prompt (_tankterm_original_prompt | string collect -N)
            functions -q fish_mode_prompt; or printf '\e]133;A\a'
            printf '%s\e]133;B\a' $prompt
        end
    end
end

# Before a prompt. The prompts are wrapped here and not when this file is read: config.fish,
# which may set the prompt, runs after the files fish reads before it.
function _tankterm_prompt_start --on-event fish_prompt
    # Without a prompt to wrap, the start is marked here; a prompt drawn again loses it.
    if not _tankterm_wrap_prompts
        printf '\e]133;A\a'
    end
end

function _tankterm_output_start --on-event fish_preexec
    printf '\e]133;C\a'
end

function _tankterm_command_end --on-event fish_postexec
    printf '\e]133;D;%s\a' $status
end
