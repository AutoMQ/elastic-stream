#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
cargo build -p frontend --release -Z unstable-options --out-dir=sdks/frontend-java/client/src/main/resources/META-INF/native/
cd sdks/frontend-java || exit
mvn package
