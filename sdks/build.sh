#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
cargo build -p frontend 
cp target/debug/libfrontend.so sdks/sdk-java/frontend/src/main/resources/META-INF/native/libfrontend.so
cd sdks/sdk-java || exit
mvn package