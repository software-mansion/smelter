"""Wire-format constants and pure parsing functions for the side-channel.

The wire is documented (and produced) on the Rust side at
``smelter-core/src/queue/side_channel/serialize.rs``. Both video and audio
messages are framed with a 4-byte big-endian u32 length prefix.

Video payload (always RGBA)::

    u32 width
    u32 height
    u64 pts_nanos
    [u8; width * height * 4] rgba_data  (row-major, channel order R G B A)

Audio payload::

    u64 start_pts_nanos
    u32 sample_rate
    u8  channel_count   (1 = mono, 2 = stereo)
    u32 sample_count    (samples per channel)
    [f64; sample_count * channel_count] samples
        mono:   [s0, s1, s2, ...]
        stereo: [l0, r0, l1, r1, ...]

All numeric values are big-endian.
"""

import struct

import numpy as np

from .errors import ProtocolError
from .types import AudioBatch, VideoFrame

LENGTH_PREFIX_SIZE = 4
VIDEO_HEADER_SIZE = 16  # u32 width + u32 height + u64 pts_nanos
AUDIO_HEADER_SIZE = 17  # u64 pts + u32 sr + u8 ch + u32 sc

_BE_F64 = np.dtype(">f8")


def parse_length_prefix(buf: bytes | bytearray | memoryview) -> int:
    """Decode a 4-byte big-endian u32 length prefix."""
    if len(buf) < LENGTH_PREFIX_SIZE:
        raise ProtocolError(f"length prefix truncated: got {len(buf)} bytes, expected 4")
    return int.from_bytes(memoryview(buf)[:LENGTH_PREFIX_SIZE], "big", signed=False)


def parse_video(payload: bytes | bytearray) -> VideoFrame:
    """Parse a single video message payload (without the length prefix).

    Returns a :class:`VideoFrame` whose ``rgba`` array is a writable view onto
    a freshly allocated ``bytearray`` — callers may mutate it in place.
    """
    if len(payload) < VIDEO_HEADER_SIZE:
        raise ProtocolError(
            f"video payload too short: got {len(payload)} bytes, expected at least "
            f"{VIDEO_HEADER_SIZE}"
        )

    width, height, pts_nanos = struct.unpack_from("!IIQ", payload, 0)
    expected = width * height * 4
    body = payload[VIDEO_HEADER_SIZE:]
    if len(body) != expected:
        raise ProtocolError(
            f"video payload size mismatch: header says {width}x{height} "
            f"({expected} bytes), got {len(body)}"
        )

    # bytearray gives us a writable buffer numpy can wrap zero-copy.
    body_buf = bytearray(body) if not isinstance(body, bytearray) else body
    rgba = np.frombuffer(body_buf, dtype=np.uint8).reshape(height, width, 4)
    return VideoFrame(rgba=rgba, pts_nanos=int(pts_nanos))


def parse_audio(
    payload: bytes | bytearray,
    *,
    dtype: np.dtype | type = np.float32,
) -> AudioBatch:
    """Parse a single audio message payload (without the length prefix).

    Args:
        payload: The bytes following the 4-byte length prefix.
        dtype: Output numpy dtype. Defaults to ``float32``. Pass ``np.float64``
            to keep the wire's full f64 precision.
    """
    if len(payload) < AUDIO_HEADER_SIZE:
        raise ProtocolError(
            f"audio payload too short: got {len(payload)} bytes, expected at least "
            f"{AUDIO_HEADER_SIZE}"
        )

    start_pts_nanos, sample_rate, channel_count, sample_count = struct.unpack_from(
        "!QIBI", payload, 0
    )
    if channel_count not in (1, 2):
        raise ProtocolError(f"audio payload has invalid channel_count {channel_count}")

    total = sample_count * channel_count
    expected_body_bytes = total * 8
    body = memoryview(payload)[AUDIO_HEADER_SIZE:]
    if len(body) != expected_body_bytes:
        raise ProtocolError(
            f"audio payload size mismatch: header says {sample_count} samples x "
            f"{channel_count} ch ({expected_body_bytes} bytes), got {len(body)}"
        )

    target_dtype = np.dtype(dtype)
    # frombuffer over the raw big-endian f64 then cast in one pass. astype(copy=False)
    # is a no-op when target == >f8, otherwise it allocates exactly once.
    flat = np.frombuffer(body, dtype=_BE_F64).astype(target_dtype, copy=False)
    samples = flat.reshape(sample_count, channel_count)
    return AudioBatch(
        samples=samples,
        sample_rate=int(sample_rate),
        start_pts_nanos=int(start_pts_nanos),
    )
