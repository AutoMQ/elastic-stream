#!/bin/bash

try_install_flatc() {
    if [ ! -f /usr/local/bin/flatc ]; then
        sudo apt-get update
        sudo apt-get install -y unzip clang
        arch=$(uname -m)
        if [ "$arch" == "aarch64" ]; then
            echo "Host arch is aarch64"
            wget -O flatc.zip https://github.com/AutoMQ/flatbuffers/releases/download/v23.3.3/Linux.flatc.binary.g++-10-aarch64.zip
        elif [ "$arch" == "x86_64" ]; then
            echo "Host arch is amd64"
            wget -O flatc.zip https://github.com/AutoMQ/flatbuffers/releases/download/v23.3.3/Linux.flatc.binary.g++-10.zip
        else
            echo "Unsupported arch"
            exit 1
        fi
        unzip flatc.zip
        sudo mv flatc /usr/local/bin/
        rm flatc.zip
    else
        echo "flatc exists"
    fi
}

try_install_rocksdb() {
    if [ ! -f rocksdb.deb ]; then
        arch=$(uname -m)
        if [ "$arch" == "aarch64" ]; then
            echo "Host arch is aarch64"
            wget -O rocksdb.deb https://github.com/AutoMQ/rocksdb/releases/download/rocksdb-v8.1.1b0/rocksdb-aarch64.deb
        elif [ "$arch" == "x86_64" ]; then
            echo "Host arch is amd64"
            wget -O rocksdb.deb https://github.com/AutoMQ/rocksdb/releases/download/rocksdb-v8.1.1b0/rocksdb-amd64.deb
        else
            echo "Unsupported arch"
            exit 1
        fi
    fi
    ./try_install.sh rocksdb.deb
    rm -f rocksdb.deb
}

BASEDIR=$(dirname "$0")
cd "$BASEDIR" || exit

try_install_flatc
try_install_rocksdb
