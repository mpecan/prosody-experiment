#!/bin/sh
# Prints the path of a `wasm-bindgen` that is exactly the version of the `wasm-bindgen` crate in
# Cargo.lock — the generator and the crate must match — installing it beside the build (the target
# directory's tools/) once, rather than over whatever `wasm-bindgen` is on PATH, which other projects
# may need at another version. What installing says goes to stderr: stdout is only the path.
#
#   bindgen=$(./scripts/wasm-bindgen.sh)
set -eu

here=$(dirname "$0")
cd "$here/.."

target_dir=$(cargo metadata --format-version 1 --no-deps |
  grep -o '"target_directory":"[^"]*"' | cut -d'"' -f4)
version=$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;}' Cargo.lock)
tools="$target_dir/tools"
bindgen="$tools/bin/wasm-bindgen"
if [ ! -x "$bindgen" ] || [ "$("$bindgen" --version 2>/dev/null)" != "wasm-bindgen $version" ]; then
  if command -v wasm-bindgen >/dev/null 2>&1 && [ "$(wasm-bindgen --version 2>/dev/null)" = "wasm-bindgen $version" ]; then
    mkdir -p "$tools/bin"
    ln -sf "$(command -v wasm-bindgen)" "$bindgen"
  elif command -v cargo-binstall >/dev/null 2>&1 && cargo binstall --no-confirm --root "$tools" "wasm-bindgen-cli@$version" >&2; then
    : # prebuilt wasm-bindgen-cli installed into $tools
  else
    echo "wasm-bindgen: installing wasm-bindgen-cli $version into $tools (once)" >&2
    cargo install --quiet --locked --root "$tools" wasm-bindgen-cli --version "$version" >&2
  fi
fi
echo "$bindgen"
