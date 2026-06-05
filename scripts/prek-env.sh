#!/usr/bin/env bash
#
# Run a cargo-based prek hook through a consistent Rust toolchain.
#
# Why this exists: some machines have a distro rust package whose `rustc` and
# `clippy-driver` carry mismatched version metadata, so `cargo clippy` fails with
# E0514 ("found crate compiled by an incompatible version of rustc"). When a
# rustup `stable` toolchain is available we prepend its bin dir to PATH so cargo,
# rustc and clippy-driver all resolve to the same (consistent) toolchain. We also
# build into a hook-private CARGO_TARGET_DIR so this doesn't fight a different
# toolchain's artifacts in a shared/global target dir. On machines without rustup
# (or without a `stable` toolchain) this is a no-op and the ambient cargo is used.
#
# Usage: scripts/prek-env.sh <command> [args...]
#   e.g. scripts/prek-env.sh cargo clippy -- -D warnings
#        scripts/prek-env.sh ./scripts/run-tests.sh
set -euo pipefail

if command -v rustup >/dev/null 2>&1; then
    stable_rustc="$(rustup which --toolchain stable rustc 2>/dev/null || true)"
    if [ -n "$stable_rustc" ] && [ -x "$stable_rustc" ]; then
        export PATH="$(dirname "$stable_rustc"):$PATH"
    fi
fi

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/prek}"

exec "$@"
