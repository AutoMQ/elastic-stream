FROM ubuntu:22.04

WORKDIR /root

ADD *.deb /root/

RUN apt-get update \
    && apt-get install -y wget telnet \
    && dpkg --recursive --install /root \
    && rm -f *.deb

CMD ["/usr/local/bin/pd", "--config", "/etc/pd/config.yaml"]
