#!/bin/bash
set -e

RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release

cp target/wasm32-unknown-unknown/release/cheddar_coin.wasm ./res
