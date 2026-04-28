"""Generic object detection over the smelter video side channel using YOLO."""

from collections.abc import Callable, Iterable
from dataclasses import dataclass

import cv2
from smelter import subscribe_video_channel
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


def _extract_detections(
    result,
    frame_w: int,
    frame_h: int,
    class_filter: set[str] | None,
    start_confidence: float,
    keep_confidence: float,
    active_tracks: set[int],
) -> list[Detection]:
    detections: list[Detection] = []
    if result.boxes is None:
        active_tracks.clear()
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
    still_active: set[int] = set()
    for (x1, y1, x2, y2), c, p, tid in zip(xyxy, cls, conf, ids, strict=True):
        # Hysteresis: a new/inactive track must clear `start_confidence` to appear,
        # while an already-shown track only needs `keep_confidence` to stay visible.
        # Untracked detections (tid is None) have no continuity, so always use start.
        threshold = (
            keep_confidence if tid is not None and tid in active_tracks else start_confidence
        )
        if p < threshold:
            continue
        name = names.get(int(c), str(c)) if isinstance(names, dict) else names[int(c)]
        if class_filter is not None and name not in class_filter:
            continue
        if tid is not None:
            still_active.add(int(tid))
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
    active_tracks.clear()
    active_tracks.update(still_active)
    return detections


def run_detection(
    model_path: str,
    on_detection: Callable[[list[Detection], int], None],
    class_filter: Iterable[str] | None = None,
    start_confidence: float = 0.6,
    keep_confidence: float = 0.4,
):
    """Connect to video side channel and run YOLO detection on every frame."""
    print("Waiting for video side channel...")

    print(f"Loading YOLO model from {model_path}...")
    model = YOLO(model_path)
    print("Detection ready.\n")

    filter_set = set(class_filter) if class_filter is not None else None
    active_tracks: set[int] = set()
    # Tracked detections missing from the current frame are kept for up to
    # MAX_LINGER_FRAMES so a brief miss doesn't make the box flicker off.
    lingering: dict[int, tuple[Detection, int]] = {}
    MAX_LINGER_FRAMES = 1

    for frame in subscribe_video_channel("input"):
        bgr = cv2.cvtColor(frame.rgba, cv2.COLOR_RGBA2BGR)
        results = model.track(bgr, persist=True, verbose=False)
        if not results:
            continue
        detections = _extract_detections(
            results[0],
            frame.width,
            frame.height,
            filter_set,
            start_confidence,
            keep_confidence,
            active_tracks,
        )
        seen_ids: set[int] = set()
        for d in detections:
            if d.track_id is not None:
                seen_ids.add(d.track_id)
                lingering[d.track_id] = (d, 0)
        for tid in list(lingering):
            det, age = lingering[tid]
            if tid in seen_ids:
                continue
            age += 1
            if age > MAX_LINGER_FRAMES:
                del lingering[tid]
            else:
                lingering[tid] = (det, age)
        # Keep lingering tracks in the hysteresis set so a reappearance at
        # low confidence still clears `keep_confidence`.
        active_tracks.update(lingering)
        output = list(detections)
        output.extend(det for tid, (det, age) in lingering.items() if age > 0)
        # Report the update 100 ms before the frame's actual pts. Detection box
        # views use a 200 ms transition (see _detection_box_view), so scheduling
        # the scene half a transition early means the animation lands on the new
        # position at the moment the frame itself is rendered.
        on_detection(output, frame.pts_nanos - 100_000_000)
