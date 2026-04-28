"""Helpers shared between tests: byte-buffer builders for the wire format."""

import struct

import numpy as np


def build_video_message(
    *,
    width: int,
    height: int,
    pts_nanos: int,
    rgba: bytes | None = None,
) -> bytes:
    """Build a complete framed video message (length prefix + payload).

    If ``rgba`` is None, generates a deterministic gradient.
    """
    if rgba is None:
        arr = np.zeros((height, width, 4), dtype=np.uint8)
        arr[..., 0] = (np.arange(width) % 256).astype(np.uint8)
        arr[..., 1] = (np.arange(height)[:, None] % 256).astype(np.uint8)
        arr[..., 3] = 255
        rgba = arr.tobytes()
    payload = struct.pack("!IIQ", width, height, pts_nanos) + rgba
    return struct.pack("!I", len(payload)) + payload


def build_audio_message(
    *,
    start_pts_nanos: int,
    sample_rate: int,
    channels: int,
    samples: np.ndarray,
) -> bytes:
    """Build a complete framed audio message.

    ``samples`` must have shape (sample_count, channels). Values are written as
    big-endian f64 regardless of the input dtype.
    """
    if samples.shape[1] != channels:
        raise ValueError(f"samples shape {samples.shape} != channels {channels}")
    sample_count = samples.shape[0]
    flat_be = samples.astype(">f8", copy=False).tobytes()
    header = struct.pack("!QIBI", start_pts_nanos, sample_rate, channels, sample_count)
    payload = header + flat_be
    return struct.pack("!I", len(payload)) + payload
