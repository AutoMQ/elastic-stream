#!/usr/bin/env bash

BASEDIR=$(dirname "$0")
cd "$BASEDIR/.." || exit
flatc --java -o 'sdks/sdk-java/frontend/src/main/generated' --java-package-prefix 'com.automq.elasticstream.client.flatc' --gen-object-api components/protocol/fbs/rpc.fbs components/protocol/fbs/model.fbs
cargo build -p frontend 
cp target/debug/libfrontend.so sdks/sdk-java/frontend/src/main/resources/META-INF/native/libfrontend.so
cd sdks/sdk-java || exit
mvn package
