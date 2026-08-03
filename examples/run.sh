#!/bin/sh
# Compiles every example. Any arguments are passed through to `prosody`, so
#
#   ./examples/run.sh            compiles each to Strudel pattern text
#   ./examples/run.sh --explain  annotates each poem token by token
#   ./examples/run.sh --osc      schedules each as OSC messages
#
# Exits non-zero if any example fails to compile, so this doubles as a smoke test.
set -eu

here=$(dirname "$0")
status=0

for poem in "$here"/*.poem; do
    printf '\n\033[1m==> %s\033[0m\n' "$(basename "$poem")"
    if ! cargo run --quiet --manifest-path "$here/../Cargo.toml" -p prosody-cli -- "$@" "$poem"; then
        status=1
    fi
done

exit "$status"
