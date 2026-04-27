"""Tests for the sync VideoConnection / AudioConnection over a real Unix socket."""

import socket
import threading
from pathlib import Path

import numpy as np
import pytest
from smelter import (
    ConnectionClosed,
    RecvTimeout,
    SideChannelInfo,
    SideChannelKind,
)
from smelter.sync import AudioConnection, VideoConnection

from ._helpers import build_audio_message, build_video_message


def _video_info(path: Path) -> SideChannelInfo:
    return SideChannelInfo(path=path, kind=SideChannelKind.VIDEO, input_id="t")


def _audio_info(path: Path) -> SideChannelInfo:
    return SideChannelInfo(path=path, kind=SideChannelKind.AUDIO, input_id="t")


def _spawn_server(socket_path: Path, send_fn) -> threading.Thread:
    """Listen on socket_path. On accept, call send_fn(client_sock) and close."""
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(socket_path))
    server.listen(1)

    def run():
        with server:
            client, _ = server.accept()
            with client:
                send_fn(client)

    t = threading.Thread(target=run, daemon=True)
    t.start()
    return t


def test_video_connection_recv(tmp_path: Path):
    sock_path = tmp_path / "video.sock"
    msg = build_video_message(width=4, height=2, pts_nanos=123)

    def send(client: socket.socket):
        client.sendall(msg + msg)

    server_thread = _spawn_server(sock_path, send)
    with VideoConnection(_video_info(sock_path)) as conn:
        f1 = conn.recv()
        f2 = conn.recv()
    assert f1.width == 4 and f1.height == 2 and f1.pts_nanos == 123
    assert f2.pts_nanos == 123
    server_thread.join(timeout=1.0)


def test_video_connection_iteration_stops_on_eof(tmp_path: Path):
    sock_path = tmp_path / "video_iter.sock"
    msg = build_video_message(width=2, height=2, pts_nanos=0)

    def send(client: socket.socket):
        client.sendall(msg + msg + msg)

    _spawn_server(sock_path, send)
    with VideoConnection(_video_info(sock_path)) as conn:
        frames = list(conn)
    assert len(frames) == 3


def test_video_connection_eof_raises_connectionclosed(tmp_path: Path):
    sock_path = tmp_path / "video_eof.sock"

    def send(_client: socket.socket):
        pass  # immediately close

    _spawn_server(sock_path, send)
    with VideoConnection(_video_info(sock_path)) as conn, pytest.raises(ConnectionClosed):
        conn.recv()


def test_video_connection_truncated_message(tmp_path: Path):
    sock_path = tmp_path / "video_trunc.sock"
    msg = build_video_message(width=2, height=2, pts_nanos=0)

    def send(client: socket.socket):
        # Send only the length prefix; close before the payload.
        client.sendall(msg[:6])

    _spawn_server(sock_path, send)
    with VideoConnection(_video_info(sock_path)) as conn, pytest.raises(ConnectionClosed):
        conn.recv()


def test_video_connection_timeout(tmp_path: Path):
    sock_path = tmp_path / "video_timeout.sock"

    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(sock_path))
    server.listen(1)

    def run():
        client, _ = server.accept()
        # Hold the socket open without sending anything.
        try:
            import time as _t

            _t.sleep(0.5)
        finally:
            client.close()
            server.close()

    threading.Thread(target=run, daemon=True).start()

    with (
        VideoConnection(_video_info(sock_path), timeout=0.05) as conn,
        pytest.raises(RecvTimeout),
    ):
        conn.recv()


def test_audio_connection_recv_default_dtype(tmp_path: Path):
    sock_path = tmp_path / "audio.sock"
    samples = np.array([[0.1, -0.1], [0.2, -0.2]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=99,
        sample_rate=48000,
        channels=2,
        samples=samples,
    )

    def send(client: socket.socket):
        client.sendall(msg)

    _spawn_server(sock_path, send)
    with AudioConnection(_audio_info(sock_path)) as conn:
        batch = conn.recv()
    assert batch.start_pts_nanos == 99
    assert batch.channels == 2
    assert batch.sample_count == 2
    assert batch.samples.dtype == np.float32
    np.testing.assert_allclose(batch.samples[:, 0], [0.1, 0.2], rtol=1e-6)


def test_audio_connection_dtype_float64(tmp_path: Path):
    sock_path = tmp_path / "audio_f64.sock"
    samples = np.array([[0.5]], dtype=np.float64)
    msg = build_audio_message(
        start_pts_nanos=0,
        sample_rate=48000,
        channels=1,
        samples=samples,
    )

    def send(client: socket.socket):
        client.sendall(msg)

    _spawn_server(sock_path, send)
    with AudioConnection(_audio_info(sock_path), dtype=np.float64) as conn:
        batch = conn.recv()
    assert batch.samples.dtype == np.float64


def test_video_connection_rejects_audio_info(tmp_path: Path):
    sock_path = tmp_path / "wrong.sock"
    with pytest.raises(ValueError, match="kind=VIDEO"):
        VideoConnection(_audio_info(sock_path))


def test_audio_connection_rejects_video_info(tmp_path: Path):
    sock_path = tmp_path / "wrong.sock"
    with pytest.raises(ValueError, match="kind=AUDIO"):
        AudioConnection(_video_info(sock_path))
