#!/usr/bin/env bash

cargo +nightly run --release --manifest-path header_chain_parser/Cargo.toml \
    resources/main_proof.bin \
    resources/main_public_input.json \
    resources/main_compiled.json \
    resources/main_proof.json \
    proof
