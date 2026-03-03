FROM ubuntu:noble-20250716

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ENV DEBIAN_FRONTEND=noninteractive
ENV NVIDIA_DRIVER_CAPABILITIES=compute,graphics,utility

RUN apt-get update -y -qq && \
  apt-get install -y \
    sudo adduser ffmpeg && \
  rm -rf /var/lib/apt/lists/*

ENV SMELTER_WEB_RENDERER_ENABLE=0
ENV SMELTER_WEB_RENDERER_GPU_ENABLE=0
ENV SMELTER_LOGGER_FORMAT=compact

ENTRYPOINT ["/bin/bash"]
