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
    wget https://github.com/lizhanhui/rocksdb/releases/download/rocksdb-v8.1.2/rocksdb.deb
    ./try_install.sh rocksdb.deb
}

try_install_flatc
try_install_rocksdb
