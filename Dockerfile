# Builder image
FROM ubuntu:noble-20250716 AS builder

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG USERNAME=smelter
ARG RUST_VERSION=1.89

ENV DEBIAN_FRONTEND=noninteractive

ENV NODE_VERSION=24.6.0

RUN apt-get update -y -qq && \
  apt-get install -y \
    build-essential curl pkg-config libssl-dev libclang-dev git sudo \
    libegl1-mesa-dev libgl1-mesa-dri libxcb-xfixes0-dev mesa-vulkan-drivers \
    ffmpeg libavcodec-dev libavformat-dev libavfilter-dev libavdevice-dev libopus-dev && \
  rm -rf /var/lib/apt/lists/*

RUN ARCH= && dpkgArch="$(dpkg --print-architecture)" \
  && case "${dpkgArch##*-}" in \
    amd64) ARCH='x64';; \
    ppc64el) ARCH='ppc64le';; \
    s390x) ARCH='s390x';; \
    arm64) ARCH='arm64';; \
    armhf) ARCH='armv7l';; \
    i386) ARCH='x86';; \
    *) echo "unsupported architecture"; exit 1 ;; \
  esac \
  && curl -fsSLO --compressed "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-$ARCH.tar.xz" \
  && tar -xJf "node-v$NODE_VERSION-linux-$ARCH.tar.xz" -C /usr/local --strip-components=1 --no-same-owner \
  && ln -s /usr/local/bin/node /usr/local/bin/nodejs \
  && node --version \
  && npm --version \
  && rm -rf /tmp/*

RUN npm install -g pnpm

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
RUN source ~/.cargo/env && rustup install $RUST_VERSION && rustup default $RUST_VERSION

COPY . /root/project

WORKDIR /root/project/ts
RUN pnpm install && pnpm build:node-sdk && pnpm -C examples/smelter-app build

WORKDIR /root/project
RUN source ~/.cargo/env && cargo build --release --no-default-features

# Runtime image
FROM ubuntu:noble-20250716

LABEL org.opencontainers.image.source=https://github.com/software-mansion/smelter

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG USERNAME=smelter

ENV DEBIAN_FRONTEND=noninteractive
ENV NVIDIA_DRIVER_CAPABILITIES=compute,graphics,utility
ENV NODE_VERSION=24.6.0

RUN apt-get update -y -qq && \
  apt-get install -y \
    sudo build-essential curl adduser ffmpeg streamlink && \
  rm -rf /var/lib/apt/lists/*

RUN ARCH= && dpkgArch="$(dpkg --print-architecture)" \
  && case "${dpkgArch##*-}" in \
    amd64) ARCH='x64';; \
    ppc64el) ARCH='ppc64le';; \
    s390x) ARCH='s390x';; \
    arm64) ARCH='arm64';; \
    armhf) ARCH='armv7l';; \
    i386) ARCH='x86';; \
    *) echo "unsupported architecture"; exit 1 ;; \
  esac \
  && curl -fsSLO --compressed "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-$ARCH.tar.xz" \
  && tar -xJf "node-v$NODE_VERSION-linux-$ARCH.tar.xz" -C /usr/local --strip-components=1 --no-same-owner \
  && ln -s /usr/local/bin/node /usr/local/bin/nodejs \
  && node --version \
  && npm --version \
  && rm -rf /tmp/*

RUN useradd -ms /bin/bash $USERNAME && adduser $USERNAME sudo
RUN echo '%sudo ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers
USER $USERNAME
RUN mkdir -p /home/$USERNAME/smelter
WORKDIR /home/$USERNAME/smelter

COPY --from=builder --chown=$USERNAME:$USERNAME /root/project/target/release/main_process /home/$USERNAME/smelter/main_process
COPY --from=builder --chown=$USERNAME:$USERNAME /root/project/ts /home/$USERNAME/smelter/ts

ENV SMELTER_WEB_RENDERER_ENABLE=0
ENV SMELTER_WEB_RENDERER_GPU_ENABLE=0
ENV SMELTER_PATH=/home/smelter/smelter/main_process

WORKDIR /home/$USERNAME/smelter/ts/examples/smelter-app

EXPOSE 3001

ENTRYPOINT ["node", "./dist/index.js"]
