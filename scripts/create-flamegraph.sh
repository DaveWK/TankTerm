#!/usr/bin/env bash

# Make sure perf is available.
if [ ! -x "$(command -v perf)" ]
then
    echo "Cannot find perf, please make sure it's installed."
    exit 1
fi

# Install cargo-flamegraph
installed_flamegraph=0
if [ ! -x "$(command -v cargo-flamegraph)" ]; then
    echo "cargo-flamegraph not installed; installing ..."
    cargo install flamegraph
    installed_flamegraph=1
fi

# Create flamegraph
cargo flamegraph --bin=tankTerm -- "$@"

# Uninstall cargo-flamegraph if it has been installed with this script
if [ "$installed_flamegraph" -eq 1 ]; then
    read -p "Would you like to uninstall cargo-flamegraph? [Y/n] " -n 1 -r
    echo
    if [[ "$REPLY" =~ ^[^Nn]*$ ]]; then
        cargo uninstall flamegraph
    fi
fi
