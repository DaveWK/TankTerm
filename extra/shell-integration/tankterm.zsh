# TankTerm shell integration for zsh.
#
# Marks prompts and commands (OSC 133) and reports the working directory (OSC 7), so TankTerm
# can jump between prompts, select a command's output, mark prompts and failed commands, notice
# finished commands and open new tabs in the right directory.
#
# Source it at the end of ~/.zshrc, after anything else that sets the prompt:
#
#     source /path/to/tankterm.zsh

[[ -o interactive && $TERM != dumb && -z ${_tankterm_loaded-} ]] || return 0
typeset -g _tankterm_loaded=1 _tankterm_running=

# Percent-encode $1 for a file URL, into REPLY.
_tankterm_encode() {
    emulate -L zsh
    setopt no_multibyte
    local char
    REPLY=
    for char in ${(s::)1}; do
        if [[ $char == [-/._~A-Za-z0-9] ]]; then
            REPLY+=$char
        else
            REPLY+=%${(l:2::0:)$(( [##16] #char ))}
        fi
    done
}

# First precmd function: the command ended, and the shell is in $PWD.
_tankterm_precmd() {
    local ret=$?
    if [[ -n $_tankterm_running ]]; then
        printf '\e]133;D;%s\a' $ret
        _tankterm_running=
    fi

    local REPLY
    _tankterm_encode $PWD
    printf '\e]7;file://%s%s\a' $HOST $REPLY
    return $ret
}

# Last precmd function: mark where the prompt starts and ends, in a prompt that was set anew.
_tankterm_prompt() {
    local ret=$?
    if [[ $PS1 != *$'\e]133;A\a'* ]]; then
        PS1=$'%{\e]133;A\a%}'$PS1$'%{\e]133;B\a%}'
    fi
    return $ret
}

_tankterm_preexec() {
    printf '\e]133;C\a'
    _tankterm_running=1
}

precmd_functions=(_tankterm_precmd $precmd_functions _tankterm_prompt)
preexec_functions+=(_tankterm_preexec)
