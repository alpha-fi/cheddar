#!/bin/bash
set -e

RUSTFLAGS='-C link-arg=-s' cargo build --all --target wasm32-unknown-unknown --release

cp target/wasm32-unknown-unknown/release/cheddar_coin.wasm ./res
cp target/wasm32-unknown-unknown/release/p1_staking_pool_dyn.wasm ./res
