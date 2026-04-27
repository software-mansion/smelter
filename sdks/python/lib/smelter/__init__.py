"""Smelter SDK — Python client for the smelter media-pipeline server.

The SDK currently exposes the smelter "side channel": a Unix-socket stream of
decoded RGBA video frames and PCM audio batches that lets Python code run ML
inference (or any other per-frame work) and feed results back to smelter via
its HTTP API.

The default surface is synchronous and blocking::

    from smelter import subscribe_video_channel

    for frame in subscribe_video_channel("cam1"):
        run_inference(frame.rgba)

For asyncio, use :mod:`smelter.aio`::

    from smelter.aio import subscribe_video_channel

    async for frame in subscribe_video_channel("cam1"):
        await run_inference(frame.rgba)

Lower-level building blocks (``connect_video``, ``connect_audio``,
``wait_for_channel``, the ``VideoConnection`` / ``AudioConnection`` classes)
live in :mod:`smelter.sync` and :mod:`smelter.aio` for the rare cases where
the one-call ``subscribe_*_channel`` helpers don't fit.

See the package README for end-to-end examples.
"""

from .context import Context
from .errors import (
    ChannelNotFound,
    ConnectionClosed,
    ProtocolError,
    RecvTimeout,
    SmelterError,
)
from .sync import (
    list_channels,
    subscribe_audio_channel,
    subscribe_video_channel,
)
from .types import (
    AudioBatch,
    SideChannelInfo,
    SideChannelKind,
    VideoFrame,
)

__version__ = "0.1.0"

__all__ = [
    "AudioBatch",
    "ChannelNotFound",
    "ConnectionClosed",
    "Context",
    "ProtocolError",
    "RecvTimeout",
    "SideChannelInfo",
    "SideChannelKind",
    "SmelterError",
    "VideoFrame",
    "list_channels",
    "subscribe_audio_channel",
    "subscribe_video_channel",
]
