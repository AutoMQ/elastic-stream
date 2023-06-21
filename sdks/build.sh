#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
cargo build -p frontend --release -Z unstable-options --out-dir=sdks/frontend-java/client/src/main/resources/META-INF/native/
cd sdks/frontend-java || exit
mvn -DargLine="--add-opens=java.base/java.nio=ALL-UNNAMED" package
