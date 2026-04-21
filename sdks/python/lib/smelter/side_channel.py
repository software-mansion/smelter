"""Client for smelter side channel unix sockets.

Connects to a video or audio side channel and parses incoming messages.

Wire format (both video and audio):
  Each message is prefixed with a 4-byte big-endian u32 length.

Video frame payload:
  u32  width
  u32  height
  u64  pts_nanos
  u8   format
  u8   plane_count
  Per plane:
    u32  plane_len
    bytes[plane_len]

Audio batch payload:
  u64  start_pts_nanos
  u32  sample_rate
  u8   channel_count (1=mono, 2=stereo)
  u32  sample_count
  f64[sample_count * channel_count]  interleaved samples
"""

import os
import socket
import struct
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

FRAME_FORMATS = {
    0: "PlanarYuv420",
    1: "PlanarYuv422",
    2: "PlanarYuv444",
    3: "PlanarYuvJ420",
    4: "InterleavedUyvy422",
    5: "InterleavedYuyv422",
    6: "Nv12",
    7: "Bgra",
    8: "Argb",
}


class SideChannelKind(Enum):
    VIDEO = "video"
    AUDIO = "audio"


@dataclass
class VideoFrame:
    width: int
    height: int
    pts_nanos: int
    format: str
    planes: list[bytes]


@dataclass
class AudioBatch:
    start_pts_nanos: int
    sample_rate: int
    channel_count: int
    sample_count: int
    samples: list[float]


@dataclass
class SideChannelInfo:
    path: str
    kind: SideChannelKind
    input_id: str


def _recv_exact(sock: socket.socket, n: int) -> bytes:
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("socket closed")
        buf.extend(chunk)
    return bytes(buf)


def _recv_message(sock: socket.socket) -> bytes:
    length_bytes = _recv_exact(sock, 4)
    (length,) = struct.unpack("!I", length_bytes)
    return _recv_exact(sock, length)


def _parse_video_frame(data: bytes) -> VideoFrame:
    # u32 + u32 + u64 + u8 + u8 = 18 bytes header
    width, height = struct.unpack_from("!II", data, 0)
    (pts_nanos,) = struct.unpack_from("!Q", data, 8)
    fmt = data[16]
    plane_count = data[17]
    offset = 18

    planes = []
    for _ in range(plane_count):
        (plane_len,) = struct.unpack_from("!I", data, offset)
        offset += 4
        planes.append(data[offset : offset + plane_len])
        offset += plane_len

    return VideoFrame(
        width=width,
        height=height,
        pts_nanos=pts_nanos,
        format=FRAME_FORMATS.get(fmt, f"Unknown({fmt})"),
        planes=planes,
    )


def _parse_audio_batch(data: bytes) -> AudioBatch:
    # u64 + u32 + u8 + u32 = 17 bytes header
    (start_pts_nanos,) = struct.unpack_from("!Q", data, 0)
    (sample_rate,) = struct.unpack_from("!I", data, 8)
    channel_count = data[12]
    (sample_count,) = struct.unpack_from("!I", data, 13)

    total = sample_count * channel_count
    samples = list(struct.unpack_from(f"!{total}d", data, 17))

    return AudioBatch(
        start_pts_nanos=start_pts_nanos,
        sample_rate=sample_rate,
        channel_count=channel_count,
        sample_count=sample_count,
        samples=samples,
    )


def _connect_socket(socket_path: str) -> socket.socket:
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(socket_path)
    return sock


class VideoConnection:
    """Connection to a video side channel socket."""

    def __init__(self, socket_path: str):
        self._sock = _connect_socket(socket_path)

    def recv(self) -> VideoFrame:
        data = _recv_message(self._sock)
        return _parse_video_frame(data)

    def close(self):
        self._sock.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


class AudioConnection:
    """Connection to an audio side channel socket."""

    def __init__(self, socket_path: str):
        self._sock = _connect_socket(socket_path)

    def recv(self) -> AudioBatch:
        data = _recv_message(self._sock)
        return _parse_audio_batch(data)

    def close(self):
        self._sock.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


class SideChannelManager:
    """Manages side channel sockets in a directory."""

    def __init__(self, socket_dir: str):
        self._socket_dir = socket_dir

    def list(self) -> list[SideChannelInfo]:
        results = []
        for name in os.listdir(self._socket_dir):
            if not name.endswith(".sock"):
                continue
            stem = name.removesuffix(".sock")
            for kind in SideChannelKind:
                prefix = kind.value + "_"
                if stem.startswith(prefix):
                    input_id = stem[len(prefix) :]
                    path = str(Path(self._socket_dir) / name)
                    results.append(SideChannelInfo(path=path, kind=kind, input_id=input_id))
                    break
        return results

    def connect(self, info: SideChannelInfo) -> VideoConnection | AudioConnection:
        if info.kind == SideChannelKind.VIDEO:
            return VideoConnection(info.path)
        else:
            return AudioConnection(info.path)
