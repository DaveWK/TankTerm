# TankTerm starts bash in POSIX mode with ENV naming this file. An interactive bash in POSIX mode
# reads ENV in place of its startup files, so this file leaves POSIX mode, puts back what the
# start changed, reads the startup files bash would have read, and then the shell integration.
#
# Which files bash reads is described under INVOCATION in bash(1). The files are read here at the
# top level, not in a function, so that what they declare stays global.

# What TankTerm left out of the command line: " --norc", " --noprofile", and the rcfile.
builtin declare _tankterm_options=${TANKTERM_BASH_INJECT-} _tankterm_rcfile=${TANKTERM_BASH_RCFILE-}
builtin unset TANKTERM_BASH_INJECT TANKTERM_BASH_RCFILE

# ENV is the user's again, or gone.
if [[ ${TANKTERM_BASH_ENV+set} == set ]]; then
    builtin export ENV=$TANKTERM_BASH_ENV
else
    builtin unset ENV
fi
builtin unset TANKTERM_BASH_ENV

# Leaving POSIX mode keeps inherit_errexit, which POSIX mode turned on.
builtin set +o posix
builtin shopt -u inherit_errexit 2> /dev/null

# TankTerm set HISTFILE so that the history doesn't go to ~/.sh_history; bash doesn't export it.
if [[ -n ${TANKTERM_BASH_UNEXPORT_HISTFILE-} ]]; then
    builtin unset TANKTERM_BASH_UNEXPORT_HISTFILE
    builtin export -n HISTFILE
fi

# The startup files, in the order bash reads them. A login shell reads the first of its own files
# that it can read; distributions whose bash has a system-wide bashrc keep it in
# /etc/bash.bashrc, and others, like Fedora, have ~/.bashrc read theirs.
_tankterm_files=()
if [[ $- != *i* ]]; then
    :
elif builtin shopt -q login_shell; then
    if [[ $_tankterm_options != *--noprofile* ]]; then
        _tankterm_files=(/etc/profile)
        for _tankterm_file in ~/.bash_profile ~/.bash_login ~/.profile; do
            if [[ -r $_tankterm_file ]]; then
                _tankterm_files+=("$_tankterm_file")
                builtin break
            fi
        done
    fi
elif [[ $_tankterm_options != *--norc* ]]; then
    _tankterm_files=(/etc/bash.bashrc "${_tankterm_rcfile:-$HOME/.bashrc}")
fi
builtin unset _tankterm_options _tankterm_rcfile

for _tankterm_file in "${_tankterm_files[@]}"; do
    [[ -r $_tankterm_file ]] && builtin source "$_tankterm_file"
done
builtin unset _tankterm_file _tankterm_files

builtin source "${BASH_SOURCE[0]%/*}/../tankterm.bash"
