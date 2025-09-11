# Builder image
FROM ubuntu:noble-20250716 as builder

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG USERNAME=smelter
ARG RUST_VERSION=1.89

ENV DEBIAN_FRONTEND=noninteractive
ENV NVIDIA_DRIVER_CAPABILITIES=compute,graphics,utility

RUN apt-get update -y -qq && \
  apt-get install -y \
    build-essential curl pkg-config libssl-dev libclang-dev git sudo \
    libnss3 libatk1.0-0 libatk-bridge2.0-0 libgdk-pixbuf2.0-0 libgtk-3-0 \
    libegl1-mesa-dev libgl1-mesa-dri libxcb-xfixes0-dev mesa-vulkan-drivers \
    ffmpeg libavcodec-dev libavformat-dev libavfilter-dev libavdevice-dev libopus-dev && \
  rm -rf /var/lib/apt/lists/*

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
RUN source ~/.cargo/env && rustup install $RUST_VERSION && rustup default $RUST_VERSION

RUN git clone https://github.com/software-mansion/smelter.git /root/project
WORKDIR /root/project
RUN git fetch && git checkout @wkozyra95/bench-in-docker

RUN source ~/.cargo/env && cargo build --release

ENTRYPOINT bash 
