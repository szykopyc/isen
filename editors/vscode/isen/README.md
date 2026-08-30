# Isen for Visual Studio Code

Small, dependency-free syntax highlighting for `.is` files. It recognises
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

This extension intentionally provides syntax and basic editing behavior only;
it does not run Isen diagnostics or include a language server.
