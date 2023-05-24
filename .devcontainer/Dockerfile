FROM mcr.microsoft.com/devcontainers/rust:0-1-buster
RUN apt-get update  \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y clang \ 
    && apt-get install -y default-jdk \
    && cd /tmp \
    && wget 'https://github.com/google/flatbuffers/releases/download/v23.5.9/Linux.flatc.binary.clang++-12.zip' \
    && unzip Linux.flatc.binary.clang++-12.zip && mv flatc /usr/local/bin/ \
    && wget 'https://dl.google.com/go/go1.20.4.linux-amd64.tar.gz' \
    && tar -C /usr/local -xzf go1.20.4.linux-amd64.tar.gz \
    && echo 'export PATH=$PATH:/usr/local/go/bin' >> /etc/profile

