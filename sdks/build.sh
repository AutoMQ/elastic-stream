#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
cargo build -p frontend -Z unstable-options --out-dir=sdks/sdk-java/frontend/src/main/resources/META-INF/native/
cd sdks/sdk-java || exit
mvn package
