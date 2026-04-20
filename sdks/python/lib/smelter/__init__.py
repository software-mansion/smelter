"""Smelter SDK - Client library for interacting with Smelter."""

__version__ = "0.1.0"

from .side_channel import (
    AudioBatch,
    AudioConnection,
    SideChannelInfo,
    SideChannelKind,
    SideChannelManager,
    VideoConnection,
    VideoFrame,
)

__all__ = [
    "AudioBatch",
    "AudioConnection",
    "SideChannelInfo",
    "SideChannelKind",
    "SideChannelManager",
    "VideoConnection",
    "VideoFrame",
]
