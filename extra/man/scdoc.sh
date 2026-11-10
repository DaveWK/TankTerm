#!/bin/sh
# Build a man page from its scdoc source, writing roff to stdout.
#
# Usage: extra/man/scdoc.sh extra/man/tankTerm.1.scd > tankTerm.1
#
# The output is touched up so groff renders it without warnings:
#
# - scdoc turns hyphenation and justification off before `.TH`, but groff's man
#   macros reset both there. Setting man(7)'s `HY` and `AD` after `.TH` keeps
#   names like ScrollHalfPageUp from being split across lines.
# - scdoc writes every table cell as a tbl text block, which tbl first lays out
#   at a fifth of the page width. A cell holding one formatted word can't wrap,
#   so it's written as a plain entry instead; the rendered table is the same.

set -eu

roff=$(scdoc < "$1")
printf '%s\n' "$roff" | awk '
{ line[NR] = $0 }
END {
    for (i = 1; i <= NR; i++) {
        if (line[i] ~ /T\{$/ && line[i + 1] ~ /^\\f[IB][^ \t]*\\fR$/ && line[i + 2] ~ /^T\}/) {
            line[i + 2] = substr(line[i], 1, length(line[i]) - 2) line[i + 1] substr(line[i + 2], 3)
            i++
            continue
        }
        print line[i]
        if (line[i] ~ /^\.TH /) {
            print ".nr HY 0"
            print ".ds AD l"
        }
    }
}'
