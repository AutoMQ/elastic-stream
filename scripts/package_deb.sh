#!/bin/bash

cp ../target/release/range-server range-server/usr/local/bin/
cp ../etc/*.yaml range-server/etc/range-server/
rm range-server/usr/local/bin/.gitignore
rm range-server/etc/range-server/.gitignore
dpkg-deb --build range-server
