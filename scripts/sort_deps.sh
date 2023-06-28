#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
cargo install cargo-sort
cargo sort -w
