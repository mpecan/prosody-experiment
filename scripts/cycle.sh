#!/bin/sh
# The tuning cycle's measuring step: scores every example against a profile, one row per poem,
# then the mean overall — the number an iteration of the cycle must raise (`PLAN.md`, "The
# machine cycle").
#
#   ./scripts/cycle.sh            the machine-music profile
#   ./scripts/cycle.sh interest   the generic one
#   ./scripts/cycle.sh jazz       the small-group jazz profile (swing is palette, so not measured)
#   ./scripts/cycle.sh machine bass,outro
#                                 the machine profile with those candidate mappings left out
#   ./scripts/cycle.sh machine bass=off,period=off
#                                 the same, as variant choices: passed through to `--set`.
#                                 A list containing `=` goes to `--set`, one without to
#                                 `--without`. (The reading is not scored: it is not in the score.)
#
# Exits non-zero if any example fails to score.
set -eu

here=$(dirname "$0")
profile=${1:-machine}
choices=${2:-}
case $choices in
    *=*) flag=--set ;;
    *) flag=--without ;;
esac
cargo build --quiet --locked --manifest-path "$here/../Cargo.toml" -p prosody-cli
bin="$here/../target/debug/prosody"

printf '%-22s %6s %6s %6s %6s %6s %6s %6s | %6s %6s %6s\n' \
    poem layer recur pitch entr sync pulse steady inter legib overall
for poem in "$here"/../examples/*.poem; do
    name=$(basename "$poem" .poem | cut -c1-22)
    "$bin" --score --profile "$profile" ${choices:+$flag "$choices"} "$poem" | awk -v name="$name" '
        { v[$1] = $2 }
        END {
            printf "%-22s %6s %6s %6s %6s %6s %6s %6s | %6s %6s %6s\n", name,
                v["layering"], v["recurrence"], v["pitched"], v["pitch_entropy"],
                v["syncopation"], (v["pulse"] == "" ? "-" : v["pulse"]),
                (v["steadiness"] == "" ? "-" : v["steadiness"]),
                v["interest"], v["legibility"], v["overall"]
        }'
done | tee /dev/stderr | awk '{ sum += $NF; n++ } END { printf "mean overall %.3f over %d poems\n", sum / n, n }'
