"""SDK-wide configuration handle.

A :class:`Context` bundles every piece of configuration the SDK needs to
locate side-channel sockets. Today it carries one value — the directory the
smelter server writes its sockets into — but is the natural home for future
options (HTTP API base URL, default timeouts, etc.).

Public API entry points (``subscribe_video_channel``, ``subscribe_audio_channel``,
``list_channels``, ``wait_for_channel``, …) accept an optional ``ctx`` keyword.
When omitted, a fresh default context is constructed per call:

1. If ``SMELTER_SIDE_CHANNEL_SOCKET_DIR`` is set, that directory is used.
2. Otherwise the current working directory is used.

Construct one explicitly when the env var isn't right for your process::

    ctx = Context(socket_dir="/var/run/smelter")
    for frame in subscribe_video_channel("cam1", ctx=ctx):
        ...
"""

import os
from pathlib import Path

ENV_SOCKET_DIR = "SMELTER_SIDE_CHANNEL_SOCKET_DIR"


class Context:
    """Configuration bundle passed to SDK entry points.

    Attributes:
        socket_dir: Directory the smelter server writes side-channel unix
            sockets into.
    """

    __slots__ = ("socket_dir",)

    socket_dir: Path

    def __init__(self, socket_dir: str | os.PathLike[str] | None = None) -> None:
        if socket_dir is None:
            env = os.environ.get(ENV_SOCKET_DIR)
            socket_dir = env if env else os.getcwd()
        self.socket_dir = Path(socket_dir)

    def __repr__(self) -> str:
        return f"Context(socket_dir={str(self.socket_dir)!r})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Context):
            return NotImplemented
        return self.socket_dir == other.socket_dir

    def __hash__(self) -> int:
        return hash(self.socket_dir)


def resolve_context(ctx: Context | None) -> Context:
    """Internal helper: return ``ctx`` if given, else a fresh default Context."""
    return ctx if ctx is not None else Context()
