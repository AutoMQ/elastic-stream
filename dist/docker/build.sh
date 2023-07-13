#!/bin/bash
version=$(git describe --tags --always --dirty | sed -e 's/^[^0-9]*//')
cp ../*_amd64.deb .
cp ../../pd/distribution/*_amd64.deb .
docker build -t elasticstream/elastic-stream:${version} -f Dockerfile .
docker push elasticstream/elastic-stream:${version}
