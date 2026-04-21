"""Example: speech-to-text subtitles + YOLO object detection with smelter."""

import os
import sys
import threading

from example.detection import Detection, run_detection
from example.smelter_server import (
    OUTPUT_ID,
    WHIP_WHEP_PORT,
    setup_pipeline,
    start_server,
    stop_server,
    update_scene,
)
from example.transcription import run_transcription


class SceneState:
    def __init__(self):
        self._lock = threading.Lock()
        self.text: str = ""
        self.detections: list[Detection] = []

    def set_text(self, text: str) -> tuple[str, list[Detection]]:
        with self._lock:
            self.text = text
            return self.text, list(self.detections)

    def set_detections(self, detections: list[Detection]) -> tuple[str, list[Detection]]:
        with self._lock:
            self.detections = detections
            return self.text, list(self.detections)


def main():
    if len(sys.argv) > 2:
        print(f"Usage: {sys.argv[0]} [path_to_mp4]")
        sys.exit(1)

    mp4_path = None
    if len(sys.argv) == 2:
        mp4_path = os.path.abspath(sys.argv[1])
        if not os.path.isfile(mp4_path):
            print(f"File not found: {mp4_path}")
            sys.exit(1)

    # YOLO model path. For face detection, point this at e.g. yolov8n-face.pt.
    # Any ultralytics-compatible YOLO model works; narrow results with DETECT_CLASSES.
    model_path = os.environ.get("YOLO_MODEL", "yolov8n.pt")
    class_filter_env = os.environ.get("DETECT_CLASSES")
    class_filter = (
        [c.strip() for c in class_filter_env.split(",") if c.strip()]
        if class_filter_env
        else ["person"]
    )

    state = SceneState()
    server, socket_dir = start_server()

    def on_segment(text: str, pts_nanos: float):
        pts_s = pts_nanos / 1e9
        print(f"[{pts_s:.3f}s] {text}")
        text, detections = state.set_text(text)
        update_scene(text, detections, pts_nanos / 1e6)

    def on_detection(detections: list[Detection], pts_nanos: int):
        text, detections = state.set_detections(detections)
        update_scene(text, detections, (pts_nanos / 1e6) - 250)

    try:
        setup_pipeline(mp4_path)
        print(f"WHEP endpoint: http://127.0.0.1:{WHIP_WHEP_PORT}/whep/{OUTPUT_ID}")

        detection_thread = threading.Thread(
            target=run_detection,
            args=(socket_dir, model_path, on_detection),
            kwargs={"class_filter": class_filter},
            daemon=True,
        )
        detection_thread.start()

        run_transcription(socket_dir, on_segment)
    except KeyboardInterrupt:
        print("\nShutting down...")
    except ConnectionError:
        print("Connection closed")
    finally:
        stop_server(server)
