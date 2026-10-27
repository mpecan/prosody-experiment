#!/bin/sh
# Builds the static page and deploys it to prosody.rocks (or staging.prosody.rocks) as an
# assets-only Cloudflare Worker (wrangler.jsonc), by hand. A merge to main deploys to prosody.rocks
# and a push to staging deploys to staging.prosody.rocks (the `deploy` and `deploy-staging` jobs in
# .github/workflows/ci.yml); this is for a deploy from a working copy. Wrangler comes from mise
# (mise.toml), logged in with `wrangler login`.
#
#   ./scripts/deploy.sh                     build, check that the browser build answers as the native one, deploy production
#   ./scripts/deploy.sh --dry-run           build and check, and show what would be uploaded, without deploying
#   ./scripts/deploy.sh staging             build, check and deploy to staging.prosody.rocks
#   ./scripts/deploy.sh staging --dry-run   build, check and show staging upload without deploying
#
# Two checks come first, and either failing stops the deploy: `scripts/check-wasm.sh` (invariant I1
# across targets — a site whose compiler disagrees with the native one is not deployed) and
# `deploy/check-worker.mjs` (http and www reach https://prosody.rocks or https://staging.prosody.rocks).
set -eu

here=$(dirname "$0")
cd "$here/.."

env_arg=""
dry_run=""
for arg in "$@"; do
  case "$arg" in
    staging|--staging) env_arg="--env staging" ;;
    --dry-run) dry_run="--dry-run" ;;
  esac
done

./scripts/check-wasm.sh
node deploy/check-worker.mjs
# From empty, so nothing an older build left behind is uploaded with it.
rm -rf target/site
./scripts/build-site.sh target/site

if [ -n "$dry_run" ]; then
  if [ -n "$env_arg" ]; then
    mise exec -- wrangler deploy $env_arg --dry-run
  else
    mise exec -- wrangler deploy --dry-run
  fi
else
  if [ -n "$env_arg" ]; then
    mise exec -- wrangler deploy $env_arg
  else
    mise exec -- wrangler deploy
  fi
fi
