#!/bin/sh
set -eu

cargo fmt -- --check
cargo test --locked
cargo test --locked --features ml-kernels
cargo clippy --all-targets --all-features -- -D warnings
cargo test --release --locked
cargo test --release --locked --features ml-kernels
cargo build --release --locked --features ml-kernels
./isen --diagnostics .

# Leave the shipped artifact free of the optional neural kernels before any
# command-line acceptance checks use it.
cargo build --release --locked

./isen test --profile all
./isen test --profile stdlib
./isen test --profile logging
(cd examples/labyrinth && ../../isen test --profile all && ../../isen test --profile fast)
./isen examples/labyrinth/labyrinth.is -- 7 5 --seed=1848 --solve-instant >/dev/null 2>/dev/null
./isen --format --check .
./isen --reference --check
./isen --diagnostics examples/tour.is tests stdlib examples/labyrinth

profile_report=$(mktemp "${TMPDIR:-/tmp}/isen-profile.XXXXXX")
trap 'rm -f "$profile_report"' EXIT HUP INT TERM
./isen --profile --json "$profile_report" examples/exit.is >/dev/null 2>/dev/null
grep -q '"format": "isen-profile-v1"' "$profile_report"
rm -f "$profile_report"
trap - EXIT HUP INT TERM

if command -v nvim >/dev/null 2>&1; then
  nvim --headless -n -i NONE -u NONE \
    '+set rtp+=editors/nvim/isen.nvim' \
    '+runtime plugin/isen.lua' \
    '+edit examples/tour.is' \
    '+IsenDiagnostics' \
    '+sleep 500m' \
    '+lua assert(#vim.diagnostic.get(0) == 0)' \
    '+qa!'
fi
