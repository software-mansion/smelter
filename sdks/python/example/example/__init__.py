"""Example: speech-to-text subtitles + YOLO object detection with smelter."""

import os
import sys
import threading

from example.detection import Detection, run_detection
from example.smelter_server import (
    OUTPUT_ID,
    WHIP_WHEP_PORT,
    UpdateCoordinator,
    setup_pipeline,
    start_server,
    stop_server,
)
from example.transcription import run_transcription


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

    coordinator = UpdateCoordinator()
    server, socket_dir = start_server()

    # Instead of each callback posting its own update_scene, events are pushed to a
    # coordinator that pairs audio+video entries in pts order once both ring buffers
    # have settled, so smelter always receives merged, monotonic scene updates.
    def on_segment(text: str, start_pts_nanos: float, end_pts_nanos: float):
        start_pts_ms = start_pts_nanos / 1e6
        end_pts_ms = end_pts_nanos / 1e6
        print(f"[{start_pts_ms / 1000:.3f}s-{end_pts_ms / 1000:.3f}s] {text}")
        coordinator.push_audio(start_pts_ms, end_pts_ms, text)

    def on_detection(detections: list[Detection], pts_nanos: int):
        coordinator.push_video(pts_nanos / 1e6, detections)

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
