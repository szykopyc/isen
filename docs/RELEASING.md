# Releasing Isen

The release workflow runs only when a Git tag matching `v*` is pushed. It
requires the tag to equal the package version, so version `0.1.0` must be
released as `v0.1.0`.

```sh
git tag -a v0.1.0 -m "Isen v0.1.0"
git push origin v0.1.0
```

## GitHub CLI flow

Authenticate once, then use `gh` to watch and inspect the release. The
workflow creates the release itself; do not run `gh release create` manually.

```sh
gh auth login
gh auth status
```

After pushing the tag, find the newest `Release` workflow run and watch its
numeric ID until it exits successfully:

```sh
gh run list --workflow Release --limit 1
gh run watch RUN_ID --exit-status --compact
```

Inspect the published release in the terminal or browser:

```sh
gh release view v0.1.0
gh release view v0.1.0 --web
```

The workflow runs the complete Linux release gate before packaging anything.
It then builds and starts two release binaries:

- `isen-linux-x86_64.tar.gz` — a statically linked `x86_64-unknown-linux-musl`
  executable, the Isen standard library, and `LICENSE`.
- `isen-macos-arm64.tar.gz` — a native Apple-Silicon executable, the Isen
  standard library, and `LICENSE`.

It publishes both archives and `SHA256SUMS` to the GitHub Release. Verify a
download before extracting it:

```sh
sha256sum -c SHA256SUMS
tar -xzf isen-linux-x86_64.tar.gz
./isen --version
```

The bundled `stdlib/` directory must remain beside the `isen` executable.
Imports beginning with `stdlib/` then work from programmes in any directory.
To exercise the same outside-the-distribution check used by the release jobs:

```sh
sh scripts/release-smoke.sh isen-linux-x86_64.tar.gz
```

The GitHub CLI can download selected release assets:

```sh
gh release download v0.1.0 \
  --pattern "isen-linux-x86_64.tar.gz" \
  --pattern "SHA256SUMS"
sha256sum -c SHA256SUMS
```

On macOS, download `isen-macos-arm64.tar.gz` and use
`shasum -a 256 -c SHA256SUMS` instead. The release workflow is idempotent:
rerunning a failed tag workflow replaces its assets and checksum file rather
than creating a second release.
