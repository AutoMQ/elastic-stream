FROM mcr.microsoft.com/devcontainers/base:bullseye
RUN apt-get update  \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y cmake && apt-get install -y clang\ 
    && cd /tmp && wget https://github.com/google/flatbuffers/archive/refs/tags/v23.3.3.zip \
    && unzip v23.3.3.zip && cd flatbuffers-23.3.3 && /usr/bin/cmake -G "Unix Makefiles" && make -j && mv flatc /usr/local/bin/ \
    && wget 'https://dl.google.com/go/go1.20.4.linux-amd64.tar.gz' \
    && tar -C /usr/local -xzf go1.20.4.linux-amd64.tar.gz && echo 'export PATH=$PATH:/usr/local/go/bin' >> /etc/profile