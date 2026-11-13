# TankTerm shell integration for bash.
#
# Marks prompts and commands (OSC 133) and reports the working directory (OSC 7), so TankTerm
# can jump between prompts, select a command's output, mark prompts and failed commands, notice
# finished commands and open new tabs in the right directory.
#
# Source it at the end of ~/.bashrc, after anything else that sets PS1 or PROMPT_COMMAND:
#
#     source /path/to/tankterm.bash
#
# Commands are only marked from bash 4.4 on, which has PS0; older versions get prompt marks and
# the working directory.

[[ $- == *i* && $TERM != dumb && -z ${_tankterm_loaded-} ]] || return 0
_tankterm_loaded=1
_tankterm_running=

# Percent-encode $1 for a file URL, into _tankterm_url.
_tankterm_encode() {
    local LC_ALL=C path=$1 char i
    _tankterm_url=
    for ((i = 0; i < ${#path}; i++)); do
        char=${path:i:1}
        case $char in
            [-/._~A-Za-z0-9]) _tankterm_url+=$char ;;
            *)
                printf -v char '%%%02X' "'$char"
                _tankterm_url+=$char
                ;;
        esac
    done
}

# First in PROMPT_COMMAND: the command ended, and the shell is in $PWD.
_tankterm_precmd() {
    local status=$?
    if [[ -n $_tankterm_running ]]; then
        printf '\e]133;D;%s\a' "$status"
        _tankterm_running=
    fi

    _tankterm_encode "$PWD"
    printf '\e]7;file://%s%s\a' "$HOSTNAME" "$_tankterm_url"
    return "$status"
}

# Last in PROMPT_COMMAND: mark where the prompt starts and ends, in a PS1 that was set anew.
_tankterm_prompt() {
    local status=$?
    if [[ $PS1 != *'\e]133;A\a'* ]]; then
        PS1='\[\e]133;A\a\]'$PS1'\[\e]133;B\a\]'
    fi
    return "$status"
}

if [[ $(declare -p PROMPT_COMMAND 2> /dev/null) == 'declare -a'* ]]; then
    PROMPT_COMMAND=(_tankterm_precmd "${PROMPT_COMMAND[@]}" _tankterm_prompt)
else
    # shellcheck disable=SC2178 # Only reached when PROMPT_COMMAND is not an array.
    PROMPT_COMMAND=$'_tankterm_precmd\n'${PROMPT_COMMAND-}$'\n_tankterm_prompt'
fi

# PS0 is shown when a command starts. It also notes that one did, so that the next prompt
# reports how it ended; an empty command line runs nothing.
if ((BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4))); then
    # shellcheck disable=SC2016 # Bash expands PS0 when it shows it.
    PS0='\e]133;C\a${_tankterm_running:0:$((_tankterm_running = 1, 0))}'${PS0-}
fi
