"""Smelter SDK - Client library for interacting with Smelter."""

__version__ = "0.1.0"

from .side_channel import (
    AudioBatch,
    SideChannelConnection,
    VideoFrame,
)

__all__ = [
    "AudioBatch",
    "SideChannelConnection",
    "VideoFrame",
]
