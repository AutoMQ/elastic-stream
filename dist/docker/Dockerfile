FROM ubuntu:22.04

ADD *.deb /root/

RUN apt-get update \
    && apt-get install -y wget telnet \
    && dpkg -R -i ./*.deb \
    && rm -f *.deb

CMD ["/usr/local/bin/pd", "--config", "/etc/pd/config.yaml"]
