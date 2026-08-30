# Development checks

Install the repository's versioned Git hooks after cloning:

```sh
sh scripts/install-git-hooks.sh
```

The installer changes only this checkout's local Git configuration. It points
`core.hooksPath` at `.githooks`, so updates to the hooks arrive with ordinary
pulls.

`pre-commit` checks Rust formatting, Rust types across all features, Clippy with
warnings denied, Isen formatting, whole-tree Isen diagnostics with optional ML
enabled, and generated reference freshness. It never rewrites files.

`pre-push` runs `scripts/release-check.sh`, the same complete gate used by CI:
default and optional-ML Rust tests in debug and release modes, Clippy, Isen test
profiles, Labyrinth, formatting, diagnostics, profiles, and the editor check
when Neovim is available.

Use Git's normal escape hatch only deliberately:

```sh
git commit --no-verify
git push --no-verify
```
