#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 RELEASE_ARCHIVE" >&2
  exit 2
fi

archive=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d "${TMPDIR:-/tmp}/isen-release-smoke.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

mkdir "$work/distribution" "$work/programme"
tar -xzf "$archive" -C "$work/distribution"

test -x "$work/distribution/isen"
test -f "$work/distribution/stdlib/logging/human.is"

printf '%s\n' \
  'borrow logger from "stdlib/logging/human.is"' \
  'dec log = logger("info", true, naught)' \
  'say("bundled stdlib loaded")' \
  > "$work/programme/main.is"

output=$(cd "$work/programme" && "$work/distribution/isen" main.is)
test "$output" = "bundled stdlib loaded"
