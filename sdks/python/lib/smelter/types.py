"""Public data types exposed by the smelter SDK."""

from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path

import numpy as np


class SideChannelKind(StrEnum):
    """The media kind carried by a side-channel socket."""

    VIDEO = "video"
    AUDIO = "audio"


@dataclass(frozen=True, slots=True)
class SideChannelInfo:
    """A discovered side-channel socket.

    Attributes:
        path: Filesystem path to the unix socket.
        kind: Whether the socket carries video frames or audio batches.
        input_id: The smelter ``input_id`` this socket belongs to. Multiple
            inputs in the same pipeline produce multiple sockets.
    """

    path: Path
    kind: SideChannelKind
    input_id: str


@dataclass(frozen=True, slots=True)
class VideoFrame:
    """A single decoded RGBA video frame from a smelter input.

    Attributes:
        rgba: Pixel data with shape ``(height, width, 4)`` and dtype
            ``uint8``. The array is writable and owns its buffer, so it is
            safe to mutate in place (e.g. ``cv2.rectangle``) without copying.
            Channel order is R, G, B, A.
        pts_nanos: Presentation timestamp in nanoseconds, in the smelter
            queue's clock (zero is the queue start; values increase
            monotonically per input).
    """

    rgba: np.ndarray
    pts_nanos: int

    @property
    def height(self) -> int:
        return int(self.rgba.shape[0])

    @property
    def width(self) -> int:
        return int(self.rgba.shape[1])

    @property
    def pts_seconds(self) -> float:
        """``pts_nanos`` expressed in seconds (lossy: float64)."""
        return self.pts_nanos / 1e9


@dataclass(frozen=True, slots=True)
class AudioBatch:
    """A batch of decoded PCM audio samples from a smelter input.

    Attributes:
        samples: Sample data with shape ``(sample_count, channels)``. Stereo
            uses column 0 for left and column 1 for right. The default dtype
            is ``float32``; pass ``dtype=np.float64`` to ``connect_audio`` to
            preserve the wire's f64 precision. Sample values are in the range
            ``[-1.0, 1.0]``.
        sample_rate: Source sample rate in Hz. The SDK does not resample.
        start_pts_nanos: PTS of the first sample in this batch, in the smelter
            queue's clock (nanoseconds).
    """

    samples: np.ndarray
    sample_rate: int
    start_pts_nanos: int

    @property
    def channels(self) -> int:
        return int(self.samples.shape[1])

    @property
    def sample_count(self) -> int:
        return int(self.samples.shape[0])

    @property
    def start_pts_seconds(self) -> float:
        return self.start_pts_nanos / 1e9

    @property
    def duration_seconds(self) -> float:
        return self.sample_count / self.sample_rate

    @property
    def end_pts_nanos(self) -> int:
        """PTS one sample past the last sample in this batch."""
        return self.start_pts_nanos + (self.sample_count * 1_000_000_000) // self.sample_rate

    def to_mono(self) -> np.ndarray:
        """Return a 1-D array of mono samples.

        For multi-channel audio the channels are averaged. The returned dtype
        matches ``samples.dtype``.
        """
        if self.channels == 1:
            return self.samples[:, 0]
        return self.samples.mean(axis=1, dtype=self.samples.dtype)
