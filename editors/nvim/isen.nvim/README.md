# Isen for Neovim

Small, hand-written syntax highlighting, hover, snippets, and diagnostics for `.is` files. It
knows about Isen's `@@` type marker and the entirely sensible `$ ... \$`
blocks. Neovim 0.10 or newer is required for asynchronous diagnostics.

## LazyVim

Create `~/.config/nvim/lua/plugins/isen.lua` with:

```lua
return {
  {
    dir = "/absolute/path/to/isen/editors/nvim/isen.nvim",
    name = "isen.nvim",
    lazy = false,
    priority = 1000,
    init = function()
      vim.g.isen_executable = "/absolute/path/to/isen/isen"
    end,
  },
}
```

Restart Neovim and open an `.is` file. Check with `:set filetype?`; it should
say `filetype=isen`. The file above is ordinary Lua; do not paste the
surrounding Markdown fences into it.

Build the release binary once with `cargo build --release`, restart Neovim, and
open an `.is` file. Run `:IsenDiagnostics`, then inspect `:lua
vim.diagnostic.open_float()` or the normal diagnostic signs/underlines. If
nothing appears, deliberately introduce a type error and check `:messages`.

Diagnostics run on open and save through Isen's generic
`--diagnostics` JSON interface. The plugin first uses `vim.g.isen_executable`,
then looks upward for the repository launcher, then tries an `isen` executable
on `PATH`. Set the executable in Lazy's `init` callback, as above, so it exists
before the plugin loads. For a non-Lazy setup, set it before the `packadd` or
plugin-manager setup call:

```lua
vim.g.isen_executable = "/absolute/path/to/isen/isen"
```

Run `:IsenDiagnostics` to refresh manually. Set
`vim.g.isen_diagnostics = false` before loading the plugin to keep syntax
highlighting without diagnostics.

Build Isen and press `K` over a user declaration or native function to show
hover information from `isen lsp`. Consecutive `///` lines immediately above a
declaration appear with its signature. Typing `$` inserts its matching `\$`;
set `vim.g.isen_pairs = false` before loading to disable that behavior.

Shared snippets are available through `:IsenSnippet`, for example
`:IsenSnippet given` or `:IsenSnippet form`. The snippet definitions are the
same JSON file consumed by the VS Code extension, so the editor templates stay
in sync.

The highlighting is deliberately a normal Vim syntax file, not a Tree-sitter
grammar. The diagnostics transport is editor-neutral; Neovim is simply the
first consumer.
