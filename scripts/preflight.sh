#!/usr/bin/env bash
# scripts/preflight.sh — local reproduction of the CI gates before `git push`.
# Every command is the one CI runs. A step this file does not cover is a step
# that can only fail remotely — when a workflow step is added, add it here in
# the same commit. Run `--quick` before every push.
#
# usage: scripts/preflight.sh [--quick]   (--quick skips the test / bench suites)
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
[[ "${1:-}" == "--quick" ]] && quick=1

step() { printf '\n\033[1;34m== %s\033[0m\n' "$*"; }
# `cargo clippy` reuses fresh `cargo check` artifacts and then lints nothing;
# touching the crate roots invalidates only this repo's fingerprints.
relint() { git ls-files | grep -E '(^|/)src/(lib|main)\.rs$' | xargs -r touch; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing tool: $1 ($2)" >&2; exit 1; }; }
has_toolchain() { rustup toolchain list | grep -q "^$1"; }

# Steps CI runs that this file cannot reproduce locally (they can only fail remotely):
#   - ci.yml:mobile-build:Install mobile targets on pinned toolchain (no cargo / grep)

need actionlint "brew install actionlint"

step "ci.yml / test: Format check"
( export CARGO_TERM_COLOR="always"; cargo fmt -- --check )

step "ci.yml / test: Clippy (default features)"
relint
( export CARGO_TERM_COLOR="always"; cargo clippy -- -W clippy::all )

step "ci.yml / test: Clippy (full features)"
relint
( export CARGO_TERM_COLOR="always"; cargo clippy --features full -- -W clippy::all )

step "ci.yml / test: Doc check (RUSTDOCFLAGS=-Dwarnings)"
( export CARGO_TERM_COLOR="always" RUSTDOCFLAGS="-Dwarnings"; cargo doc --no-deps --features full )

step "ci.yml / mobile-build: cargo check (iOS aarch64)"
rustup target list --installed | grep -q '^aarch64-apple-ios$' || rustup target add aarch64-apple-ios
( export CARGO_TERM_COLOR="always"; cargo check --target aarch64-apple-ios --no-default-features )

step "ci.yml / mobile-build: cargo check (Android aarch64)"
rustup target list --installed | grep -q '^aarch64-linux-android$' || rustup target add aarch64-linux-android
( export CARGO_TERM_COLOR="always"; cargo check --target aarch64-linux-android --no-default-features )

step "ci.yml / actionlint: actionlint"
actionlint .github/workflows/*.yml

if [[ $quick -eq 1 ]]; then
  echo; echo "preflight --quick OK (test / bench suites skipped)"; exit 0
fi

step "ci.yml / test: Test (default features)"
( export CARGO_TERM_COLOR="always"; cargo test )

step "ci.yml / test: Test (full features)"
( export CARGO_TERM_COLOR="always"; cargo test --features full )

step "ci.yml / test: Doc-tests"
( export CARGO_TERM_COLOR="always"; cargo test --doc --features full )

echo; echo "preflight OK"
