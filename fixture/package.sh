#!/usr/bin/env bash

set -euo pipefail

cargo build --release --target wasm32-unknown-unknown -p fixture
cargo build --release --target wasm32-unknown-unknown -p fixture_integrity

nix develop --quiet --command bash -c "cd happ/dna && hc dna pack ."
nix develop --quiet --command bash -c "cd happ && hc app pack ."
