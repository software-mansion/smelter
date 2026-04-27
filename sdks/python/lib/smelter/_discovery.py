"""Side-channel socket discovery — shared between sync and async APIs.

A smelter pipeline configured with ``side_channel: { video: true, audio: true }``
on an input creates two unix sockets in a directory: ``video_<input_id>.sock``
and ``audio_<input_id>.sock``. The directory path is set on the server side via
the ``SMELTER_SIDE_CHANNEL_SOCKET_DIR`` env var (or returned by the SDK's own
server-launch helpers).
"""

import os
from pathlib import Path

from .types import SideChannelInfo, SideChannelKind

_SUFFIX = ".sock"


def scan_socket_dir(socket_dir: str | os.PathLike[str]) -> list[SideChannelInfo]:
    """List every side-channel socket currently present in ``socket_dir``.

    Returns an empty list if the directory does not exist (yet) — the smelter
    server may not have created it. Filenames that don't match the
    ``<kind>_<input_id>.sock`` convention are silently ignored.
    """
    dir_path = Path(socket_dir)
    try:
        names = os.listdir(dir_path)
    except FileNotFoundError:
        return []

    results: list[SideChannelInfo] = []
    for name in names:
        if not name.endswith(_SUFFIX):
            continue
        stem = name[: -len(_SUFFIX)]
        for kind in SideChannelKind:
            prefix = f"{kind.value}_"
            if stem.startswith(prefix):
                input_id = stem[len(prefix) :]
                results.append(
                    SideChannelInfo(
                        path=dir_path / name,
                        kind=kind,
                        input_id=input_id,
                    )
                )
                break
    return results


def filter_channels(
    channels: list[SideChannelInfo],
    *,
    kind: SideChannelKind | None,
    input_id: str | None,
) -> list[SideChannelInfo]:
    """Apply optional ``kind`` and ``input_id`` filters."""
    out = channels
    if kind is not None:
        out = [c for c in out if c.kind == kind]
    if input_id is not None:
        out = [c for c in out if c.input_id == input_id]
    return out
