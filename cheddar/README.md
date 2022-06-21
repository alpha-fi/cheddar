# Cheddar Coin

Cheddar Coin is the power horse of the Cheddar Network. It's issued by through a collaboration in the Cheddar Common Farming.
Main features of Cheddar Coin are:

+ staking
+ governance
+ protocol utility (you will be able to use it in all dapps in our network)


## Technicalities

The Cheddar Coin implements the `NEP-141` standard. It's a fungible token.


### Compiling

You can build release version by running next scripts inside each contract folder:

```
rustup target add wasm32-unknown-unknown
RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/cheddar_coin.wasm res/cheddar_coin.wasm
```