#!/bin/sh
# Builds the static page and deploys it to prosody.rocks as an assets-only Cloudflare Worker
# (wrangler.jsonc). Wrangler comes from mise (mise.toml), logged in with `wrangler login`.
#
#   ./scripts/deploy.sh             build, check that the browser build answers as the native one, deploy
#   ./scripts/deploy.sh --dry-run   build and check, and show what would be uploaded, without deploying
#
# The check is `scripts/check-wasm.sh` (invariant I1 across targets): a site whose compiler disagrees
# with the native one is not deployed.
set -eu

here=$(dirname "$0")
cd "$here/.."

./scripts/check-wasm.sh
# From empty, so nothing an older build left behind is uploaded with it.
rm -rf target/site
./scripts/build-site.sh target/site

if [ "${1:-}" = "--dry-run" ]; then
  mise exec -- wrangler deploy --dry-run
else
  mise exec -- wrangler deploy
fi
