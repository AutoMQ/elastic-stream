#!/bin/sh

if ! dpkg -l "$(dpkg-deb -W --showformat '${Package}:${Architecture}' "$1")" | grep -q '^ii'; then
    sudo dpkg -i "$1"
fi
