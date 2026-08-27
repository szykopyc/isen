#!/bin/sh
set -eu

root=$(git rev-parse --show-toplevel)
hooks="$root/.githooks"

if [ ! -d "$hooks" ]; then
  echo "missing versioned hooks directory: $hooks" >&2
  exit 1
fi

git config --local core.hooksPath "$hooks"
printf 'installed Isen Git hooks from %s\n' "$hooks"
