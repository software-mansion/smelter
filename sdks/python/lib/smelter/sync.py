"""Synchronous (blocking) client for the smelter side channel.

The functions and classes here are re-exported from the top-level ``smelter``
package — most users should write::

    from smelter import subscribe_video_channel, subscribe_audio_channel

    for frame in subscribe_video_channel("cam1"):
        ...

A :class:`~smelter.Context` carries the side-channel socket directory and may
be passed via ``ctx=``. Without one, every call resolves a default context
from ``SMELTER_SIDE_CHANNEL_SOCKET_DIR`` (falling back to the current working
directory).

Use :mod:`smelter.aio` for the asyncio equivalents.
"""

import contextlib
import os
import socket
import time
from collections.abc import Iterator
from types import TracebackType
from typing import Self

import numpy as np

from . import _discovery, _protocol
from .context import Context, resolve_context
from .errors import ChannelNotFound, ConnectionClosed, RecvTimeout
from .types import AudioBatch, SideChannelInfo, SideChannelKind, VideoFrame


def list_channels(*, ctx: Context | None = None) -> list[SideChannelInfo]:
    """Return every side-channel socket currently visible to ``ctx``.

    Sockets that don't follow the ``<kind>_<input_id>.sock`` convention are
    skipped. A missing directory yields an empty list (the server may not yet
    have created it).
    """
    return _discovery.scan_socket_dir(resolve_context(ctx).socket_dir)


def wait_for_channel(
    *,
    ctx: Context | None = None,
    kind: SideChannelKind | None = None,
    input_id: str | None = None,
    timeout: float | None = None,
    poll_interval: float = 0.2,
) -> SideChannelInfo:
    """Block until a matching side-channel socket appears, then return it.

    Args:
        ctx: SDK context; resolved from env / cwd if omitted.
        kind: If set, only match sockets of this kind.
        input_id: If set, only match sockets for this smelter input id.
        timeout: Total seconds to wait. ``None`` waits forever.
        poll_interval: Seconds between directory scans.

    Raises:
        ChannelNotFound: ``timeout`` elapsed before any matching socket
            appeared.
    """
    socket_dir = resolve_context(ctx).socket_dir
    deadline = None if timeout is None else time.monotonic() + timeout
    while True:
        matches = _discovery.filter_channels(
            _discovery.scan_socket_dir(socket_dir),
            kind=kind,
            input_id=input_id,
        )
        if matches:
            return matches[0]
        if deadline is not None and time.monotonic() >= deadline:
            raise ChannelNotFound(
                f"no matching side-channel socket appeared in {socket_dir!s} "
                f"within {timeout}s (kind={kind}, input_id={input_id})"
            )
        time.sleep(poll_interval)


def _connect_path(path: str | os.PathLike[str], timeout: float | None) -> socket.socket:
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    if timeout is not None:
        sock.settimeout(timeout)
    sock.connect(os.fspath(path))
    return sock


def _recv_exact(sock: socket.socket, n: int) -> bytearray:
    buf = bytearray(n)
    view = memoryview(buf)
    got = 0
    while got < n:
        try:
            received = sock.recv_into(view[got:], n - got)
        except TimeoutError as e:
            raise RecvTimeout(f"recv timed out after {got}/{n} bytes") from e
        if received == 0:
            if got == 0:
                raise ConnectionClosed("peer closed the side-channel socket")
            raise ConnectionClosed(
                f"peer closed mid-message after {got}/{n} bytes — stream truncated"
            )
        got += received
    return buf


def _recv_message(sock: socket.socket) -> bytearray:
    length_buf = _recv_exact(sock, _protocol.LENGTH_PREFIX_SIZE)
    length = _protocol.parse_length_prefix(length_buf)
    return _recv_exact(sock, length)


class _BaseConnection:
    __slots__ = ("_sock",)

    _sock: socket.socket

    def close(self) -> None:
        """Close the underlying socket. Idempotent."""
        with contextlib.suppress(OSError):
            self._sock.close()

    def set_timeout(self, seconds: float | None) -> None:
        """Set or clear the per-``recv`` timeout. ``None`` blocks forever."""
        self._sock.settimeout(seconds)

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        self.close()


class VideoConnection(_BaseConnection):
    """Blocking client for one video side-channel socket.

    Construct via :func:`connect_video` (which validates the channel kind)
    rather than calling this directly.
    """

    __slots__ = ()

    def __init__(self, info: SideChannelInfo, *, timeout: float | None = None):
        if info.kind != SideChannelKind.VIDEO:
            raise ValueError(f"VideoConnection requires kind=VIDEO, got {info.kind}")
        self._sock = _connect_path(info.path, timeout)

    def recv(self) -> VideoFrame:
        """Receive the next video frame.

        Raises:
            ConnectionClosed: peer closed the socket.
            RecvTimeout: no message arrived within the configured timeout.
            ProtocolError: payload did not match the wire format.
        """
        return _protocol.parse_video(_recv_message(self._sock))

    def __iter__(self) -> Iterator[VideoFrame]:
        try:
            while True:
                yield self.recv()
        except ConnectionClosed:
            return


class AudioConnection(_BaseConnection):
    """Blocking client for one audio side-channel socket.

    Construct via :func:`connect_audio`.
    """

    __slots__ = ("_dtype",)

    _dtype: np.dtype

    def __init__(
        self,
        info: SideChannelInfo,
        *,
        dtype: np.dtype | type = np.float32,
        timeout: float | None = None,
    ):
        if info.kind != SideChannelKind.AUDIO:
            raise ValueError(f"AudioConnection requires kind=AUDIO, got {info.kind}")
        self._sock = _connect_path(info.path, timeout)
        self._dtype = np.dtype(dtype)

    def recv(self) -> AudioBatch:
        """Receive the next audio batch.

        Raises:
            ConnectionClosed: peer closed the socket.
            RecvTimeout: no message arrived within the configured timeout.
            ProtocolError: payload did not match the wire format.
        """
        return _protocol.parse_audio(_recv_message(self._sock), dtype=self._dtype)

    def __iter__(self) -> Iterator[AudioBatch]:
        try:
            while True:
                yield self.recv()
        except ConnectionClosed:
            return


def connect_video(info: SideChannelInfo, *, timeout: float | None = None) -> VideoConnection:
    """Open a :class:`VideoConnection` for the given discovered channel.

    ``info`` must come from :func:`list_channels` or :func:`wait_for_channel`.
    """
    return VideoConnection(info, timeout=timeout)


def connect_audio(
    info: SideChannelInfo,
    *,
    dtype: np.dtype | type = np.float32,
    timeout: float | None = None,
) -> AudioConnection:
    """Open an :class:`AudioConnection`.

    Args:
        info: Channel handle from :func:`list_channels` or
            :func:`wait_for_channel`.
        dtype: Sample dtype to expose. Defaults to ``np.float32``; pass
            ``np.float64`` to keep the wire's full precision.
        timeout: Per-``recv`` timeout in seconds. ``None`` blocks forever.
    """
    return AudioConnection(info, dtype=dtype, timeout=timeout)


def subscribe_video_channel(
    input_id: str,
    *,
    ctx: Context | None = None,
    timeout: float | None = None,
) -> Iterator[VideoFrame]:
    """Wait for a video side channel for ``input_id``, connect, yield frames.

    The one-call form for the most common use case::

        for frame in subscribe_video_channel("cam1"):
            run_inference(frame.rgba)

    Args:
        input_id: The smelter input id whose video to consume.
        ctx: SDK context; resolved from env / cwd if omitted.
        timeout: How long to wait for the socket to appear. Applied to the
            initial discovery only; ``recv`` itself blocks forever once
            connected. Use :meth:`VideoConnection.set_timeout` for per-recv
            timeouts.

    Raises:
        ChannelNotFound: no matching socket appeared within ``timeout``.
    """
    info = wait_for_channel(ctx=ctx, kind=SideChannelKind.VIDEO, input_id=input_id, timeout=timeout)
    with connect_video(info) as conn:
        yield from conn


def subscribe_audio_channel(
    input_id: str,
    *,
    ctx: Context | None = None,
    dtype: np.dtype | type = np.float32,
    timeout: float | None = None,
) -> Iterator[AudioBatch]:
    """Wait for an audio side channel for ``input_id``, connect, yield batches.

    See :func:`subscribe_video_channel` for ``ctx`` / ``timeout`` semantics. ``dtype`` is
    forwarded to :func:`connect_audio`.
    """
    info = wait_for_channel(ctx=ctx, kind=SideChannelKind.AUDIO, input_id=input_id, timeout=timeout)
    with connect_audio(info, dtype=dtype) as conn:
        yield from conn


__all__ = [
    "AudioConnection",
    "VideoConnection",
    "connect_audio",
    "connect_video",
    "list_channels",
    "subscribe_audio_channel",
    "subscribe_video_channel",
    "wait_for_channel",
]
