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

try_install_rocksdb() {
    if [ ! -f rocksdb.deb ]; then
        wget https://github.com/lizhanhui/rocksdb/releases/download/rocksdb-v8.1.3/rocksdb.deb
    fi
    ./try_install.sh rocksdb.deb
    rm -f rocksdb.deb
}

BASEDIR=$(dirname "$0")
cd "$BASEDIR" || exit

try_install_flatc
try_install_rocksdb
