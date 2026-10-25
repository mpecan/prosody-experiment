#!/bin/sh
# Checks that the page's WebAssembly compiler answers byte-for-byte what the native server does,
# for every example under several settings (invariant I1 across targets). Needs node.
#
#   ./scripts/check-wasm.sh
set -eu

here=$(dirname "$0")
cd "$here/.."
port=${PROSODY_CHECK_PORT:-4799}

./scripts/build-site.sh target/site >/dev/null
cargo build --quiet --release --locked -p prosody-web
target_dir=$(cargo metadata --format-version 1 --no-deps |
  grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)

"$target_dir/release/prosody-web" "$port" >/dev/null &
server=$!
trap 'kill $server 2>/dev/null' EXIT INT TERM
# The server prints once it is bound; poll rather than guess how long that takes.
tries=0
until curl -s -o /dev/null "http://127.0.0.1:$port/site.json"; do
  tries=$((tries + 1))
  if [ "$tries" -gt 100 ]; then echo "check-wasm: the server did not start" >&2; exit 1; fi
  sleep 0.1
done

node scripts/check-wasm.mjs target/site "http://127.0.0.1:$port" examples
