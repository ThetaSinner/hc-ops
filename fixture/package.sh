#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR=$(dirname "$0")

RUSTFLAGS="--cfg getrandom_backend=\"custom\"" cargo build --release --target wasm32-unknown-unknown --manifest-path "$SCRIPT_DIR/fixture/Cargo.toml"
RUSTFLAGS="--cfg getrandom_backend=\"custom\"" cargo build --release --target wasm32-unknown-unknown --manifest-path "$SCRIPT_DIR/fixture_integrity/Cargo.toml"

pushd "$SCRIPT_DIR/happ/dna" || exit 1
hc dna pack .
popd || exit 1

pushd "$SCRIPT_DIR/happ" || exit 1
hc app pack .
popd || exit 1

echo "Packaging complete"
