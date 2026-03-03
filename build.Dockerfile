FROM ubuntu:noble-20250716

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG USERNAME=smelter
ARG RUST_VERSION=1.93

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update -y -qq && \
  apt-get install -y \
    build-essential curl pkg-config libssl-dev libclang-dev git sudo \
    libegl1-mesa-dev libgl1-mesa-dri libxcb-xfixes0-dev mesa-vulkan-drivers \
    ffmpeg libavcodec-dev libavformat-dev libavfilter-dev libavdevice-dev libopus-dev rsync && \
  rm -rf /var/lib/apt/lists/*

RUN mkdir /smelter
RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
RUN source ~/.cargo/env && rustup install $RUST_VERSION && rustup default $RUST_VERSION

ENTRYPOINT ["/bin/bash"]
