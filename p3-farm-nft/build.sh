#!/bin/bash
set -e

RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
cp ../target/wasm32-unknown-unknown/release/*.wasm ./res/

near dev-deploy -f --wasmFile res/p3_farm_nft.wasm
