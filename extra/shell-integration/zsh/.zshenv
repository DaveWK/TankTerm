# TankTerm starts zsh with ZDOTDIR naming this directory, so zsh reads this file in place of the
# user's .zshenv. It puts ZDOTDIR back, where zsh reads the user's other startup files from, sets
# the shell integration up to load before the first prompt, after .zshrc set the prompt, and reads
# the user's .zshenv last.
#
# A backslash before a command keeps an alias from replacing it.

if (( ${+TANKTERM_ZSH_ZDOTDIR} )); then
    \builtin export ZDOTDIR=$TANKTERM_ZSH_ZDOTDIR
    \builtin unset TANKTERM_ZSH_ZDOTDIR
else
    \builtin unset ZDOTDIR
fi

if [[ -o interactive ]]; then
    # %x is this file; the shell integration is two directories up. The function is defined
    # before the user's files run, so their aliases don't change it.
    \builtin typeset -g _tankterm_integration=${${(%):-%x}:A:h:h}/tankterm.zsh
    _tankterm_deferred_load() {
        precmd_functions=(${precmd_functions:#_tankterm_deferred_load})
        if [[ -r $_tankterm_integration ]]; then
            \builtin source -- $_tankterm_integration
            # Its own precmd functions start with the next prompt; this one is drawn now.
            (( ! ${+functions[_tankterm_precmd]} )) || _tankterm_precmd
            (( ! ${+functions[_tankterm_prompt]} )) || _tankterm_prompt
        fi
        \builtin unset _tankterm_integration
        \builtin unfunction _tankterm_deferred_load
    }
    \builtin typeset -ga precmd_functions
    precmd_functions+=(_tankterm_deferred_load)
fi

# Without a ZDOTDIR zsh reads its files from HOME, and leaves out those it can't read.
if [[ -r ${ZDOTDIR-$HOME}/.zshenv ]]; then
    \builtin source -- ${ZDOTDIR-$HOME}/.zshenv
fi
