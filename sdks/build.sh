#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
cargo build -p frontend 
cp target/debug/libfrontend.so sdks/java/lib/src/main/resources/META-INF/native/libfrontend.so
cd sdks/java || exit
./gradlew build -x signArchives