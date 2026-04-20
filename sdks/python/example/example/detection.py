"""Generic object detection over the smelter video side channel using YOLO."""

import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass

import cv2
import numpy as np
from smelter import SideChannelKind, SideChannelManager, VideoFrame
from ultralytics import YOLO


@dataclass
class Detection:
    class_id: int
    class_name: str
    confidence: float
    # Stable ID across frames assigned by the tracker, or None if untracked.
    track_id: int | None
    # Normalized coordinates in [0, 1] relative to the source frame.
    x: float
    y: float
    width: float
    height: float


def _yuv420_to_bgr(frame: VideoFrame) -> np.ndarray:
    y_plane, u_plane, v_plane = frame.planes
    h, w = frame.height, frame.width
    y = np.frombuffer(y_plane, dtype=np.uint8).reshape(h, w)
    u = np.frombuffer(u_plane, dtype=np.uint8).reshape(h // 2, w // 2)
    v = np.frombuffer(v_plane, dtype=np.uint8).reshape(h // 2, w // 2)
    yuv = np.concatenate([y.flatten(), u.flatten(), v.flatten()]).reshape(h * 3 // 2, w)
    return cv2.cvtColor(yuv, cv2.COLOR_YUV2BGR_I420)


def _extract_detections(
    result,
    frame_w: int,
    frame_h: int,
    class_filter: set[str] | None,
    min_confidence: float,
) -> list[Detection]:
    detections: list[Detection] = []
    if result.boxes is None:
        return detections
    names = result.names
    xyxy = result.boxes.xyxy.cpu().numpy()
    cls = result.boxes.cls.cpu().numpy().astype(int)
    conf = result.boxes.conf.cpu().numpy()
    ids_tensor = result.boxes.id
    ids = (
        ids_tensor.cpu().numpy().astype(int).tolist()
        if ids_tensor is not None
        else [None] * len(xyxy)
    )
    for (x1, y1, x2, y2), c, p, tid in zip(xyxy, cls, conf, ids, strict=True):
        if p < min_confidence:
            continue
        name = names.get(int(c), str(c)) if isinstance(names, dict) else names[int(c)]
        if class_filter is not None and name not in class_filter:
            continue
        detections.append(
            Detection(
                class_id=int(c),
                class_name=name,
                confidence=float(p),
                track_id=int(tid) if tid is not None else None,
                x=float(x1) / frame_w,
                y=float(y1) / frame_h,
                width=float(x2 - x1) / frame_w,
                height=float(y2 - y1) / frame_h,
            )
        )
    return detections


def run_detection(
    socket_dir: str,
    model_path: str,
    on_detection: Callable[[list[Detection], int], None],
    class_filter: Iterable[str] | None = None,
    min_confidence: float = 0.4,
    detect_interval_s: float = 0.2,
):
    """Connect to video side channel and run YOLO detection.

    Calls `on_detection(detections, pts_nanos)` each time a frame is processed.
    Frames are throttled by `detect_interval_s` to limit inference load.
    Assumes frames arrive as PlanarYuv420 — other formats are skipped.
    """
    manager = SideChannelManager(socket_dir)

    print("Waiting for video side channel...")
    while True:
        channels = manager.list()
        video_channels = [c for c in channels if c.kind == SideChannelKind.VIDEO]
        if video_channels:
            break
        time.sleep(0.2)

    info = video_channels[0]
    print(f"Connecting to video channel for input '{info.input_id}'...")

    print(f"Loading YOLO model from {model_path}...")
    model = YOLO(model_path)
    print("Detection ready.\n")

    filter_set = set(class_filter) if class_filter is not None else None
    last_run = 0.0

    with manager.connect(info) as conn:
        while True:
            frame = conn.recv()

            now = time.monotonic()
            if now - last_run < detect_interval_s:
                continue
            last_run = now

            if frame.format != "PlanarYuv420":
                continue

            bgr = _yuv420_to_bgr(frame)
            results = model.track(bgr, persist=True, verbose=False)
            if not results:
                continue
            detections = _extract_detections(
                results[0], frame.width, frame.height, filter_set, min_confidence
            )
            on_detection(detections, frame.pts_nanos)
