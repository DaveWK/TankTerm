TANKTERM-SSH(1)

# NAME

tankTerm-ssh - Run ssh with a TERM the server knows.

# SYNOPSIS

*tankTerm ssh* [*--probe*|*--no-probe*] [*--reprobe*] _<SSH_ARGS>_...

# DESCRIPTION

*tankTerm ssh* runs *ssh*(1) with the arguments it is given, unchanged, and
sets _TERM_ to a terminfo entry the server has. A server without an entry for
_TERM_ can't run full-screen programs properly, and _alacritty_, the entry
TankTerm uses, is missing from minimal Debian and Ubuntu installs.

Hosts that the _probe_ patterns of the *[ssh]* table list (see *tankTerm*(5))
are checked once. The check looks for the entries of _ssh.order_ in the
server's system terminfo directories, and the first one found becomes _TERM_.
A host with none of them, and every host the patterns don't list, gets
_ssh.fallback_, which is _xterm-256color_ by default.

The check runs a fixed shell command on the server that tests which terminfo
files exist. It runs over a shared connection (*ControlMaster*), which the
session then uses too, so the first connection authenticates once. Entries in
_~/.terminfo_ don't count, since root can't see them after *sudo*. Nothing is
installed on the server. When a Debian or Ubuntu server lacks the _alacritty_
entry, *tankTerm ssh* says that installing *ncurses-term* there adds it.

The answer is cached for the user, host and port that *ssh -G* resolves the
destination to, so aliases and the _User_ and _Port_ of _~/.ssh/config_ count.
Later connections use the cached answer and run nothing extra on the server.

If the check can't connect, *tankTerm ssh* exits with ssh's status, 255,
instead of trying again. Options that don't open a session, such as *-G*,
*-V*, *-Q* and *-O*, go to ssh without changing _TERM_. With *-N*, *-W*, *-M*
or *-S*, the host isn't checked.

To use it for a profile, make it the profile's shell:

	*[[profiles]]*++
name = _"Build server"_++
shell = { program = _"tankTerm"_, args = [_"ssh"_, _"build.lan"_] }

*tankTerm ssh* is available on Linux, BSD and macOS.

# OPTIONS

These options come before the ssh arguments; everything from the first ssh
argument on goes to ssh. Options of *tankTerm*(1) itself, such as
*--config-file* and *-o*, go before *ssh*.

*--probe*

	Treat the host as if the _probe_ patterns listed it: check it, or use its
	cached answer.

*--no-probe*

	Send _ssh.fallback_ without checking the host or reading the cache.

*--reprobe*

	Check the host again and replace its cached answer. Implies *--probe*.

*-h, --help*

	Print help information.

# FILES

_$XDG_STATE_HOME/tankterm/ssh-terms.json_

	The cached answers, by _user@host:port_. Deleting the file is always safe;
	hosts are checked again on their next connection.

_$XDG_RUNTIME_DIR/tankterm/ssh-%C_

	Sockets of the shared connections, or in _/tmp/tankterm-<uid>_ without
	_$XDG_RUNTIME_DIR_. A configuration with its own _ControlPath_ keeps it.
	A shared connection closes 30 seconds after its last session, unless the
	configuration sets _ControlPersist_.

# EXAMPLES

Check hosts on the local network and build machines, but not the NAS:

	*[ssh]*++
probe = [_"\*.lan"_, _"build-\*"_, _"pi@10.0.0.\*"_, _"!nas.lan"_]

Check a server again after installing *ncurses-term* on it:

	tankTerm ssh --reprobe build.lan

# SEE ALSO

*ssh*(1), *ssh_config*(5), *tankTerm*(1), *tankTerm-msg*(1), *tankTerm*(5)

# BUGS

Found a bug? Please report it at _https://github.com/DaveWK/TankTerm/issues_.
