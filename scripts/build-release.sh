#!/bin/sh
set -eu

PROJECT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
CARGO_HOME_DIR=${CARGO_HOME:-"$HOME/.cargo"}
REMAP_FLAGS="--remap-path-prefix=$PROJECT_DIR=. --remap-path-prefix=$CARGO_HOME_DIR=/cargo"

RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }$REMAP_FLAGS" npm run tauri build -- --bundles dmg
