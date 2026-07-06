#!/usr/bin/env bash
# Bootstrap a fresh git worktree / checkout:
# Installs git hooks and verifies Rust and Cargo.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "Checking for Rust/Cargo installation..."
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"

if ! command -v cargo &> /dev/null; then
  echo "ERROR: Cargo is not installed or not in /opt/homebrew/opt/rustup/bin." >&2
  exit 1
fi

echo "Found Cargo: $(cargo --version)"

# Install pre-commit hooks if pre-commit is installed
if command -v pre-commit &> /dev/null; then
  echo "Installing git pre-commit/pre-push hooks..."
  pre-commit install
  pre-commit install --hook-type pre-push
else
  echo "WARNING: pre-commit command not found. Please install pre-commit to enable automated checks."
fi

echo "Worktree bootstrap completed successfully."
