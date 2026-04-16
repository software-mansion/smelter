"""Smelter server lifecycle and API helpers."""

import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

SMELTER_PORT = 8081
SMELTER_API = f"http://127.0.0.1:{SMELTER_PORT}"
WHIP_WHEP_PORT = 9000
OUTPUT_ID = "output"
INPUT_ID = "input"
SIDE_CHANNEL_DELAY_MS = 5000


def api_post(path: str, body: dict | None = None):
    data = json.dumps(body).encode() if body is not None else b""
    req = urllib.request.Request(
        f"{SMELTER_API}{path}",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        error_body = e.read().decode()
        print(f"API error {e.code} on {path}: {error_body}", file=sys.stderr)
        raise


def wait_for_smelter():
    for _ in range(50):
        try:
            urllib.request.urlopen(f"{SMELTER_API}/status")
            return
        except Exception:
            time.sleep(1)
    raise RuntimeError("Smelter did not start in time")


def setup_pipeline(mp4_path: str | None):
    if mp4_path is not None:
        api_post(
            f"/api/input/{INPUT_ID}/register",
            {
                "type": "mp4",
                "path": mp4_path,
                "loop": True,
                "decoder_map": {
                    "h264": "ffmpeg_h264",
                },
                "side_channel": {
                    "audio": True,
                },
            },
        )
    else:
        api_post(
            f"/api/input/{INPUT_ID}/register",
            {
                "type": "whip_server",
                "bearer_token": "example",
                "video": {
                    "decoder_preferences": ["ffmpeg_h264"],
                },
                "side_channel": {
                    "audio": True,
                },
            },
        )
        print(f"WHIP endpoint: http://127.0.0.1:{WHIP_WHEP_PORT}/whip/{INPUT_ID}")

    api_post(
        f"/api/output/{OUTPUT_ID}/register",
        {
            "type": "whep_server",
            "bearer_token": "example",
            "video": {
                "resolution": {"width": 1920, "height": 1080},
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
                    "ffmpeg_options": {
                        "tune": "zerolatency",
                        "thread_type": "slice",
                    },
                },
                "initial": {
                    "root": {
                        "type": "rescaler",
                        "child": {
                            "type": "input_stream",
                            "input_id": INPUT_ID,
                        },
                    },
                },
            },
            "audio": {
                "encoder": {"type": "opus"},
                "initial": {
                    "inputs": [{"input_id": INPUT_ID}],
                },
            },
        },
    )

    api_post("/api/start")


def update_scene(text: str, schedule_time_ms: float):
    api_post(
        f"/api/output/{OUTPUT_ID}/update",
        {
            "video": {
                "root": {
                    "type": "rescaler",
                    "child": {
                        "type": "view",
                        "width": 1920,
                        "height": 1080,
                        "children": [
                            {
                                "type": "rescaler",
                                "child": {
                                    "type": "input_stream",
                                    "input_id": INPUT_ID,
                                },
                            },
                            {
                                "type": "view",
                                "background_color": "#000000EE",
                                "border_radius": 24,
                                "padding_horizontal": 80,
                                "left": 40,
                                "bottom": 40,
                                "width": 1920 - 2 * 160,
                                "height": 120,
                                "overflow": "hidden",
                                "children": [
                                    {
                                        "type": "rescaler",
                                        "child": {
                                            "type": "view",
                                            "width": (1920 - 2 * 160) * 4,
                                            "height": 4 * 120,
                                            "direction": "column",
                                            "children": [
                                                {"type": "view"},
                                                {
                                                    "type": "text",
                                                    "text": text,
                                                    "wrap": "word",
                                                    "font_size": 40 * 4,
                                                    "line_height": 50 * 4,
                                                    "width": (1920 - 2 * 160) * 4,
                                                    "color": "#FFFFFFFF",
                                                    "align": "center",
                                                },
                                                {"type": "view"},
                                            ],
                                        },
                                    },
                                ],
                            },
                        ],
                    },
                },
            },
            "audio": {
                "inputs": [{"input_id": INPUT_ID}],
            },
            "schedule_time_ms": schedule_time_ms,
        },
    )


def start_server() -> tuple[subprocess.Popen, str]:
    """Start smelter server. Returns (process, socket_dir)."""
    socket_dir = tempfile.mkdtemp(prefix="smelter_sockets_")
    env = {
        **os.environ,
        "SMELTER_API_PORT": str(SMELTER_PORT),
        "SMELTER_SIDE_CHANNEL_SOCKET_DIR": socket_dir,
        "SMELTER_SIDE_CHANNEL_DELAY_MS": str(SIDE_CHANNEL_DELAY_MS),
    }

    print(f"Starting smelter on port {SMELTER_PORT}...")
    server = subprocess.Popen(
        [os.environ["SMELTER_PATH"]]
        if "SMELTER_PATH" in os.environ
        else ["cargo", "run", "-p", "smelter", "--bin", "main_process"],
        env=env,
        stdout=sys.stdout,
        stderr=sys.stderr,
    )

    wait_for_smelter()
    print("Smelter is ready.")
    return server, socket_dir


def stop_server(server: subprocess.Popen):
    server.send_signal(signal.SIGTERM)
    server.wait(timeout=5)
