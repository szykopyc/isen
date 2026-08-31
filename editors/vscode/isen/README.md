# Isen for Visual Studio Code

Small, dependency-free syntax highlighting, hover, and snippets for `.is` files. It recognises
Isen declarations, control flow, types, built-in spaces, comments, strings,
the `@@` type marker, map literals, and `$ ... \$` blocks.

## Install from this checkout

Create a symbolic link in VS Code's local extensions directory:

```sh
mkdir -p ~/.vscode/extensions
ln -s /absolute/path/to/isen/editors/vscode/isen \
  ~/.vscode/extensions/isen-language
```

If that destination already exists, remove or rename the old extension first.
Restart VS Code, or run **Developer: Reload Window** from the command palette,
then open an `.is` file. The language selector in the status bar should say
`Isen`.

For VS Code Insiders, use `~/.vscode-insiders/extensions` instead. Compatible
editors may use a differently named extensions directory.

## Run it while developing

Open this `editors/vscode/isen` folder as the VS Code workspace, select **Run
Isen extension** from the Run and Debug view, then press F5. VS Code opens an
Extension Development Host window; open an `.is` file there to test the
extension. Pressing F5 while the main Isen repository is open instead tries to
debug the active `.is` file.

Build Isen once, then set `isen.executable` if `isen` is not on `PATH`. Hovering
over a documented declaration or Rust-native function uses the lightweight
`isen lsp` server. Consecutive `///` lines immediately above a declaration are
shown as documentation. `$` automatically pairs with `\$`.

Type a snippet prefix such as `given`, `form`, `problem`, `dec`, `if`, `each`,
or `attempt`, then accept the editor completion. These definitions are shared
with the Neovim plugin from `snippets/isen.json`.
