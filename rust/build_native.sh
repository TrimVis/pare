#!/usr/bin/env bash

# Builds a non-portable binary
RUSTFLAGS='-C target-cpu=native' cargo build --release

