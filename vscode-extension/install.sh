#!/bin/bash
# Install HTBasic VS Code extension
EXT_DIR="$HOME/.vscode/extensions/htbasic.htbasic-0.1.0"
mkdir -p "$EXT_DIR"
cp -r "$(dirname "$0")"/* "$EXT_DIR/"
echo "HTBasic extension installed to $EXT_DIR"
echo "Restart VS Code to activate."
