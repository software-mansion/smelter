"""Tests for the asyncio AsyncVideoConnection / AsyncAudioConnection."""

import asyncio
import contextlib
from pathlib import Path

import numpy as np
import pytest
from smelter import Context, SideChannelInfo, SideChannelKind
from smelter.aio import connect_audio, connect_video, subscribe_video_channel
from smelter.errors import ConnectionClosed, RecvTimeout

from ._helpers import build_audio_message, build_video_message

pytestmark = pytest.mark.asyncio


def _video_info(path: Path) -> SideChannelInfo:
    return SideChannelInfo(path=path, kind=SideChannelKind.VIDEO, input_id="t")


def _audio_info(path: Path) -> SideChannelInfo:
    return SideChannelInfo(path=path, kind=SideChannelKind.AUDIO, input_id="t")


async def _spawn_server(socket_path: Path, payload: bytes, hold_open: bool = False):
    """Listen on socket_path. On connect, write payload then close (or hold)."""

    async def handler(reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        try:
            writer.write(payload)
            await writer.drain()
            if hold_open:
                await asyncio.sleep(0.5)
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()

    server = await asyncio.start_unix_server(handler, str(socket_path))
    return server


async def test_async_video_recv_and_iter(tmp_path: Path):
    sock_path = tmp_path / "video.sock"
    msg = build_video_message(width=4, height=2, pts_nanos=42)
    server = await _spawn_server(sock_path, msg + msg + msg)
    try:
        conn = await connect_video(_video_info(sock_path))
        async with conn:
            frames = [frame async for frame in conn]
        assert len(frames) == 3
        assert frames[0].pts_nanos == 42
    finally:
        server.close()
        await server.wait_closed()


async def test_async_video_eof_raises(tmp_path: Path):
    sock_path = tmp_path / "video_eof.sock"
    server = await _spawn_server(sock_path, b"")
    try:
        conn = await connect_video(_video_info(sock_path))
        async with conn:
            with pytest.raises(ConnectionClosed):
                await conn.recv()
    finally:
        server.close()
        await server.wait_closed()


async def test_async_video_timeout(tmp_path: Path):
    sock_path = tmp_path / "video_timeout.sock"
    server = await _spawn_server(sock_path, b"", hold_open=True)
    try:
        conn = await connect_video(_video_info(sock_path), timeout=0.05)
        async with conn:
            with pytest.raises(RecvTimeout):
                await conn.recv()
    finally:
        server.close()
        await server.wait_closed()


async def test_async_audio_recv_default_dtype(tmp_path: Path):
    sock_path = tmp_path / "audio.sock"
    samples = np.array([[0.1], [0.2], [0.3]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=7,
        sample_rate=48000,
        channels=1,
        samples=samples,
    )
    server = await _spawn_server(sock_path, msg)
    try:
        conn = await connect_audio(_audio_info(sock_path))
        async with conn:
            batch = await conn.recv()
    finally:
        server.close()
        await server.wait_closed()
    assert batch.samples.dtype == np.float32
    assert batch.sample_count == 3


async def test_async_subscribe_video_channel_picks_up_socket(tmp_path: Path):
    sock_path = tmp_path / "video_cam.sock"
    msg = build_video_message(width=2, height=2, pts_nanos=0)
    server = await _spawn_server(sock_path, msg + msg)
    try:
        frames = []
        ctx = Context(socket_dir=tmp_path)
        async for frame in subscribe_video_channel("cam", ctx=ctx, timeout=1.0):
            frames.append(frame)
        assert len(frames) == 2
    finally:
        server.close()
        await server.wait_closed()


async def test_async_connect_video_rejects_audio_info(tmp_path: Path):
    sock_path = tmp_path / "x.sock"
    with pytest.raises(ValueError, match="kind=VIDEO"):
        await connect_video(_audio_info(sock_path))


async def test_async_connect_audio_rejects_video_info(tmp_path: Path):
    sock_path = tmp_path / "x.sock"
    with pytest.raises(ValueError, match="kind=AUDIO"):
        await connect_audio(_video_info(sock_path))
