"""Pure parsing tests against hand-crafted byte buffers."""

import numpy as np
import pytest
from smelter._protocol import (
    AUDIO_HEADER_SIZE,
    LENGTH_PREFIX_SIZE,
    VIDEO_HEADER_SIZE,
    parse_audio,
    parse_length_prefix,
    parse_video,
)
from smelter.errors import ProtocolError

from ._helpers import build_audio_message, build_video_message


def test_length_prefix_roundtrip():
    assert parse_length_prefix(b"\x00\x00\x00\x05") == 5
    assert parse_length_prefix(b"\x00\x01\x00\x00") == 65536


def test_length_prefix_rejects_truncation():
    with pytest.raises(ProtocolError):
        parse_length_prefix(b"\x00\x00")


def test_parse_video_basic():
    msg = build_video_message(width=4, height=3, pts_nanos=1_000_000_000)
    payload = msg[LENGTH_PREFIX_SIZE:]
    frame = parse_video(payload)
    assert frame.width == 4
    assert frame.height == 3
    assert frame.pts_nanos == 1_000_000_000
    assert frame.pts_seconds == pytest.approx(1.0)
    assert frame.rgba.shape == (3, 4, 4)
    assert frame.rgba.dtype == np.uint8


def test_parse_video_rgba_is_writable():
    msg = build_video_message(width=2, height=2, pts_nanos=0)
    payload = msg[LENGTH_PREFIX_SIZE:]
    frame = parse_video(payload)
    # Should not raise.
    frame.rgba[0, 0, 0] = 42
    assert frame.rgba[0, 0, 0] == 42


def test_parse_video_short_header():
    with pytest.raises(ProtocolError, match="too short"):
        parse_video(b"\x00" * (VIDEO_HEADER_SIZE - 1))


def test_parse_video_size_mismatch():
    # Header claims 10x10 but payload only carries 1 byte.
    bad = bytes.fromhex("0000000a0000000a0000000000000000") + b"\x00"
    with pytest.raises(ProtocolError, match="size mismatch"):
        parse_video(bad)


def test_parse_audio_mono_default_dtype():
    samples = np.array([[0.0], [0.5], [-0.5], [1.0]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=42,
        sample_rate=48000,
        channels=1,
        samples=samples,
    )
    batch = parse_audio(msg[LENGTH_PREFIX_SIZE:])
    assert batch.start_pts_nanos == 42
    assert batch.sample_rate == 48000
    assert batch.channels == 1
    assert batch.sample_count == 4
    assert batch.samples.shape == (4, 1)
    assert batch.samples.dtype == np.float32  # default
    np.testing.assert_allclose(batch.samples[:, 0], [0.0, 0.5, -0.5, 1.0])


def test_parse_audio_stereo_interleaved():
    samples = np.array([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=0,
        sample_rate=44100,
        channels=2,
        samples=samples,
    )
    batch = parse_audio(msg[LENGTH_PREFIX_SIZE:])
    assert batch.channels == 2
    assert batch.sample_count == 3
    np.testing.assert_allclose(batch.samples[:, 0], [0.1, 0.3, 0.5], rtol=1e-6)
    np.testing.assert_allclose(batch.samples[:, 1], [0.2, 0.4, 0.6], rtol=1e-6)


def test_parse_audio_dtype_float64_keeps_precision():
    # A value that can't be represented exactly in f32 but can in f64.
    val = 1.0 + 1e-10
    samples = np.array([[val]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=0,
        sample_rate=48000,
        channels=1,
        samples=samples,
    )
    batch = parse_audio(msg[LENGTH_PREFIX_SIZE:], dtype=np.float64)
    assert batch.samples.dtype == np.float64
    assert batch.samples[0, 0] == val


def test_parse_audio_to_mono_helper():
    samples = np.array([[0.1, 0.3], [0.2, 0.4]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=0,
        sample_rate=48000,
        channels=2,
        samples=samples,
    )
    batch = parse_audio(msg[LENGTH_PREFIX_SIZE:])
    mono = batch.to_mono()
    assert mono.shape == (2,)
    np.testing.assert_allclose(mono, [0.2, 0.3], rtol=1e-6)


def test_parse_audio_short_header():
    with pytest.raises(ProtocolError, match="too short"):
        parse_audio(b"\x00" * (AUDIO_HEADER_SIZE - 1))


def test_parse_audio_invalid_channel_count():
    # 17 bytes header with channels=3.
    bad = bytes.fromhex("0000000000000000" + "0000bb80" + "03" + "00000000")
    with pytest.raises(ProtocolError, match="channel_count"):
        parse_audio(bad)


def test_parse_audio_size_mismatch():
    # Header claims 4 mono samples (32 bytes) but body has zero.
    bad = bytes.fromhex("0000000000000000" + "0000bb80" + "01" + "00000004")
    with pytest.raises(ProtocolError, match="size mismatch"):
        parse_audio(bad)


def test_audio_batch_end_pts_and_duration():
    samples = np.zeros((480, 1), dtype=np.float64)  # 10 ms at 48k
    msg = build_audio_message(
        start_pts_nanos=1_000_000,
        sample_rate=48000,
        channels=1,
        samples=samples,
    )
    batch = parse_audio(msg[LENGTH_PREFIX_SIZE:])
    assert batch.duration_seconds == pytest.approx(0.01)
    assert batch.end_pts_nanos == 1_000_000 + 10_000_000
