#!/bin/sh
cargo clean -p eth_pairings
RAYON_NUM_THREADS=4 GAS_METERING=1 cargo test --release --features=gas_metering_mode -- --nocapture --ignored run_deterministic_parallel_search_no_filtering