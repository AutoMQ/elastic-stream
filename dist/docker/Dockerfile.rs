FROM ubuntu:22.04

RUN apt-get update \
    && apt-get install -y wget \
    && wget -q -O /tmp/range-server.deb https://github.com/AutoMQ/elastic-stream/releases/download/v0.2.1/range-server_0.2.1_amd64.deb \
    && dpkg -i /tmp/range-server.deb \
    && rm /tmp/range-server.deb

COPY config/range-server.yaml /etc/range-server/

ENTRYPOINT ["/usr/local/bin/range-server", "-c", "/etc/range-server/range-server.yaml", "-l", "/etc/range-server/range-server-log.yaml"]
