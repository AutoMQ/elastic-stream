#!/bin/bash

docker build -t elasticstream/range-server:0.2.1 -f Dockerfile.rs .
docker push elasticstream/range-server:0.2.1
docker build -t elasticstream/pd-server:0.2.1 -f Dockerfile.pd .
docker push elasticstream/pd-server:0.2.1
