"""Example: speech-to-text subtitles with smelter."""

import os
import sys

from example.smelter_server import (
    OUTPUT_ID,
    WHIP_WHEP_PORT,
    setup_pipeline,
    start_server,
    stop_server,
    update_scene,
)
from example.transcription import run_transcription


def on_segment(text: str, pts_nanos: float):
    pts_s = pts_nanos / 1e9
    print(f"[{pts_s:.3f}s] {text}")
    update_scene(text, pts_nanos / 1e6)


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

    server, socket_dir = start_server()

    try:
        setup_pipeline(mp4_path)
        print(f"WHEP endpoint: http://127.0.0.1:{WHIP_WHEP_PORT}/whep/{OUTPUT_ID}")

        run_transcription(socket_dir, on_segment)
    except KeyboardInterrupt:
        print("\nShutting down...")
    except ConnectionError:
        print("Connection closed")
    finally:
        stop_server(server)
