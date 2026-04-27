"""Asyncio-native client for the smelter side channel.

Mirrors :mod:`smelter.sync` one-for-one. Built on
:func:`asyncio.open_unix_connection`, so a side-channel reader fits cleanly
into an existing event loop without dedicating an OS thread per stream.

Example::

    from smelter.aio import subscribe_video_channel

    async for frame in subscribe_video_channel("cam1"):
        await process(frame)

ML inference inside an async loop will still block the loop — wrap heavy
synchronous calls in :func:`asyncio.to_thread` or
:meth:`asyncio.AbstractEventLoop.run_in_executor`.
"""

import asyncio
import contextlib
import os
from collections.abc import AsyncIterator
from types import TracebackType
from typing import Self

import numpy as np

from . import _discovery, _protocol
from .context import Context, resolve_context
from .errors import ChannelNotFound, ConnectionClosed, RecvTimeout
from .types import AudioBatch, SideChannelInfo, SideChannelKind, VideoFrame


async def list_channels(*, ctx: Context | None = None) -> list[SideChannelInfo]:
    """Return every side-channel socket currently visible to ``ctx``.

    Async only for API symmetry; the directory scan itself is synchronous and
    cheap (one ``listdir`` call).
    """
    return _discovery.scan_socket_dir(resolve_context(ctx).socket_dir)


async def wait_for_channel(
    *,
    ctx: Context | None = None,
    kind: SideChannelKind | None = None,
    input_id: str | None = None,
    timeout: float | None = None,
    poll_interval: float = 0.2,
) -> SideChannelInfo:
    """Async equivalent of :func:`smelter.sync.wait_for_channel`."""
    socket_dir = resolve_context(ctx).socket_dir
    loop = asyncio.get_running_loop()
    deadline = None if timeout is None else loop.time() + timeout
    while True:
        matches = _discovery.filter_channels(
            _discovery.scan_socket_dir(socket_dir),
            kind=kind,
            input_id=input_id,
        )
        if matches:
            return matches[0]
        if deadline is not None and loop.time() >= deadline:
            raise ChannelNotFound(
                f"no matching side-channel socket appeared in {socket_dir!s} "
                f"within {timeout}s (kind={kind}, input_id={input_id})"
            )
        await asyncio.sleep(poll_interval)


async def _open(path: str | os.PathLike[str]) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
    return await asyncio.open_unix_connection(os.fspath(path))


async def _recv_message(reader: asyncio.StreamReader, timeout: float | None) -> bytes:
    async def _read() -> bytes:
        try:
            length_bytes = await reader.readexactly(_protocol.LENGTH_PREFIX_SIZE)
        except asyncio.IncompleteReadError as e:
            if e.partial:
                raise ConnectionClosed(
                    f"peer closed mid-message: only {len(e.partial)}/"
                    f"{_protocol.LENGTH_PREFIX_SIZE} bytes of length prefix"
                ) from e
            raise ConnectionClosed("peer closed the side-channel socket") from e
        length = _protocol.parse_length_prefix(length_bytes)
        try:
            return await reader.readexactly(length)
        except asyncio.IncompleteReadError as e:
            raise ConnectionClosed(
                f"peer closed mid-message: only {len(e.partial)}/{length} bytes of payload"
            ) from e

    if timeout is None:
        return await _read()
    try:
        return await asyncio.wait_for(_read(), timeout)
    except TimeoutError as e:
        raise RecvTimeout(f"recv timed out after {timeout}s") from e


class _AsyncBaseConnection:
    __slots__ = ("_reader", "_timeout", "_writer")

    _reader: asyncio.StreamReader
    _writer: asyncio.StreamWriter
    _timeout: float | None

    def set_timeout(self, seconds: float | None) -> None:
        """Set or clear the per-``recv`` timeout."""
        self._timeout = seconds

    async def aclose(self) -> None:
        """Close the underlying socket. Idempotent."""
        with contextlib.suppress(OSError, RuntimeError):
            self._writer.close()
            await self._writer.wait_closed()

    async def __aenter__(self) -> Self:
        return self

    async def __aexit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        await self.aclose()


class AsyncVideoConnection(_AsyncBaseConnection):
    """Asyncio-native client for one video side-channel socket.

    Construct via :func:`connect_video`.
    """

    __slots__ = ()

    def __init__(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
        *,
        timeout: float | None,
    ):
        self._reader = reader
        self._writer = writer
        self._timeout = timeout

    async def recv(self) -> VideoFrame:
        """Receive the next video frame.

        Raises:
            ConnectionClosed: peer closed the socket.
            RecvTimeout: no message arrived within the configured timeout.
            ProtocolError: payload did not match the wire format.
        """
        return _protocol.parse_video(await _recv_message(self._reader, self._timeout))

    def __aiter__(self) -> Self:
        return self

    async def __anext__(self) -> VideoFrame:
        try:
            return await self.recv()
        except ConnectionClosed:
            raise StopAsyncIteration from None


class AsyncAudioConnection(_AsyncBaseConnection):
    """Asyncio-native client for one audio side-channel socket.

    Construct via :func:`connect_audio`.
    """

    __slots__ = ("_dtype",)

    _dtype: np.dtype

    def __init__(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
        *,
        dtype: np.dtype | type,
        timeout: float | None,
    ):
        self._reader = reader
        self._writer = writer
        self._timeout = timeout
        self._dtype = np.dtype(dtype)

    async def recv(self) -> AudioBatch:
        """Receive the next audio batch.

        Raises:
            ConnectionClosed: peer closed the socket.
            RecvTimeout: no message arrived within the configured timeout.
            ProtocolError: payload did not match the wire format.
        """
        return _protocol.parse_audio(
            await _recv_message(self._reader, self._timeout),
            dtype=self._dtype,
        )

    def __aiter__(self) -> Self:
        return self

    async def __anext__(self) -> AudioBatch:
        try:
            return await self.recv()
        except ConnectionClosed:
            raise StopAsyncIteration from None


async def connect_video(
    info: SideChannelInfo,
    *,
    timeout: float | None = None,
) -> AsyncVideoConnection:
    """Open an :class:`AsyncVideoConnection` for the given channel."""
    if info.kind != SideChannelKind.VIDEO:
        raise ValueError(f"connect_video requires kind=VIDEO, got {info.kind}")
    reader, writer = await _open(info.path)
    return AsyncVideoConnection(reader, writer, timeout=timeout)


async def connect_audio(
    info: SideChannelInfo,
    *,
    dtype: np.dtype | type = np.float32,
    timeout: float | None = None,
) -> AsyncAudioConnection:
    """Open an :class:`AsyncAudioConnection`.

    See :func:`smelter.sync.connect_audio` for the ``dtype`` semantics.
    """
    if info.kind != SideChannelKind.AUDIO:
        raise ValueError(f"connect_audio requires kind=AUDIO, got {info.kind}")
    reader, writer = await _open(info.path)
    return AsyncAudioConnection(reader, writer, dtype=dtype, timeout=timeout)


async def subscribe_video_channel(
    input_id: str,
    *,
    ctx: Context | None = None,
    timeout: float | None = None,
) -> AsyncIterator[VideoFrame]:
    """Wait for a video side channel for ``input_id``, connect, yield frames.

    Async generator. See :func:`smelter.sync.subscribe_video_channel` for the ``ctx`` /
    ``timeout`` semantics.
    """
    info = await wait_for_channel(
        ctx=ctx, kind=SideChannelKind.VIDEO, input_id=input_id, timeout=timeout
    )
    conn = await connect_video(info)
    async with conn:
        async for frame in conn:
            yield frame


async def subscribe_audio_channel(
    input_id: str,
    *,
    ctx: Context | None = None,
    dtype: np.dtype | type = np.float32,
    timeout: float | None = None,
) -> AsyncIterator[AudioBatch]:
    """Wait for an audio side channel for ``input_id``, connect, yield batches.

    Async generator. See :func:`smelter.sync.subscribe_audio_channel` for parameter
    semantics.
    """
    info = await wait_for_channel(
        ctx=ctx, kind=SideChannelKind.AUDIO, input_id=input_id, timeout=timeout
    )
    conn = await connect_audio(info, dtype=dtype)
    async with conn:
        async for batch in conn:
            yield batch


__all__ = [
    "AsyncAudioConnection",
    "AsyncVideoConnection",
    "connect_audio",
    "connect_video",
    "list_channels",
    "subscribe_audio_channel",
    "subscribe_video_channel",
    "wait_for_channel",
]
