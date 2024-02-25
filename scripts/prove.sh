#!/usr/bin/env bash

cargo +nightly run -p sandstorm-cli -r -F parallel -- \
    --program resources/main_compiled.json \
    --air-public-input resources/main_public_input.json \
    prove \
    --air-private-input resources/main_private_input.json \
    --output resources/main_proof.json
