#!/usr/bin/env bash

source .venv/bin/activate && \
cairo-compile \
  resources/main.cairo \
  --output resources/main_compiled.json \
  --proof_mode && \
deactivate
