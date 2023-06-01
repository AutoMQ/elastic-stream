#!/bin/bash

try_install_flatc() {
    if [ ! -f /usr/local/bin/flatc ]; then
        sudo apt-get update
        sudo apt-get install -y unzip clang
        wget -O flatc.zip https://github.com/AutoMQ/flatbuffers/releases/download/v23.3.3/Linux.flatc.binary.g++-10.zip
        unzip flatc.zip
        sudo mv flatc /usr/local/bin/
        rm flatc.zip
    else
        echo "flatc exists"
    fi
}

try_install_sccache() {
    if [ ! -f /usr/local/bin/sccache ]; then
        wget -O sccache.tar.gz https://github.com/mozilla/sccache/releases/download/v0.3.3/sccache-v0.3.3-x86_64-unknown-linux-musl.tar.gz
        tar -xzvf sccache.tar.gz
        sudo mv sccache-v0.3.3-x86_64-unknown-linux-musl/sccache /usr/local/bin/
        rm sccache.tar.gz
        rm -fr sccache-v0.3.3-x86_64-unknown-linux-musl
    else
        echo "sccache exists"
    fi
}

try_install_flatc
