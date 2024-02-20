#!/usr/bin/env zsh

cargo +nightly run -p sandstorm-cli -r -F parallel -- \
    --program example/array-sum.json \
    --air-public-input example/air-public-input.json \
    prove \
    --air-private-input example/air-private-input.json \
    --output example/array-sum.proof
