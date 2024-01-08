#!/usr/bin/env bash

# TODO: add linting and unit test run.

# Build
#cargo build --release --package booktoken_api --bin booktoken_api --bin mint_ws_server --bin mint_queue_watcher
cargo build --release --package scrolls

# Lint

# Run unit tests

exit 0
