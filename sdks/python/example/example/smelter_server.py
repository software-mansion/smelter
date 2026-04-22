"""Smelter server lifecycle and API helpers."""

import json
import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from collections import deque

SMELTER_PORT = 8081
SMELTER_API = f"http://127.0.0.1:{SMELTER_PORT}"
WHIP_WHEP_PORT = 9000
OUTPUT_ID = "output"
OUTPUT_MP4_ID = "output_mp4"
MP4_OUTPUT_PATH = "/tmp/smelter_output.mp4"
INPUT_ID = "input"
SIDE_CHANNEL_DELAY_MS = 7000


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
                    "video": True,
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
                    "video": True,
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

    api_post(
        f"/api/output/{OUTPUT_MP4_ID}/register",
        {
            "type": "mp4",
            "path": MP4_OUTPUT_PATH,
            # Fragmented MP4 that stays playable if smelter is killed before writing the trailer.
            # `delay_moov` defers writing the initial moov until after the first packet, so the
            # muxer has seen the H.264 SPS/PPS and can populate avcC — without this, the upfront
            # moov lands with empty extradata and no player can find codec parameters.
            "ffmpeg_options": {
                "movflags": "frag_keyframe+empty_moov+default_base_moof+delay_moov",
            },
            "video": {
                "resolution": {"width": OUTPUT_W, "height": OUTPUT_H},
                "encoder": {
                    "type": "ffmpeg_h264",
                    "preset": "ultrafast",
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
                "encoder": {"type": "aac"},
                "initial": {
                    "inputs": [{"input_id": INPUT_ID}],
                },
            },
        },
    )

    api_post("/api/start")


OUTPUT_W = 1920
OUTPUT_H = 1080


def _subtitle_view(text: str) -> dict:
    return {
        "type": "view",
        "background_color": "#000000EE",
        "border_radius": 24,
        "padding_horizontal": 80,
        "left": 40,
        "bottom": 40,
        "width": OUTPUT_W - 2 * 160,
        "height": 120,
        "overflow": "hidden",
        "children": [
            {
                "type": "rescaler",
                "child": {
                    "type": "view",
                    "width": (OUTPUT_W - 2 * 160) * 4,
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
                            "width": (OUTPUT_W - 2 * 160) * 4,
                            "color": "#FFFFFFFF",
                            "align": "center",
                        },
                        {"type": "view"},
                    ],
                },
            },
        ],
    }


def _detection_box_view(det) -> dict:
    # `det` has normalized coordinates; map to the full output canvas.
    left = int(det.x * OUTPUT_W)
    top = int(det.y * OUTPUT_H)
    width = max(2, int(det.width * OUTPUT_W))
    height = max(2, int(det.height * OUTPUT_H))
    id_part = f"#{det.track_id} " if det.track_id is not None else ""
    label = f"{id_part}{det.class_name} {det.confidence:.2f}"
    # Stable per-track component id lets smelter interpolate the view's transform
    # (left/top/width/height) between consecutive update_scene calls.
    box_id = f"det-{det.track_id}" if det.track_id is not None else None
    view: dict = {
        "type": "view",
        "left": left,
        "top": top,
        "width": width,
        "height": height,
        "border_width": 4,
        "border_color": "#00FF88FF",
        "border_radius": 6,
        "transition": {"duration_ms": 200},
        "children": [
            {
                "type": "view",
                "left": 0,
                "top": 0,
                "width": min(width, 260),
                "height": 36,
                "background_color": "#00FF88EE",
                "padding_horizontal": 8,
                "children": [
                    {
                        "type": "text",
                        "text": label,
                        "font_size": 24,
                        "color": "#000000FF",
                    },
                ],
            },
        ],
    }
    if box_id is not None:
        view["id"] = box_id
    return view


def _build_scene_body(text: str, detections: list) -> dict:
    children: list[dict] = [
        {
            "type": "rescaler",
            "child": {
                "type": "input_stream",
                "input_id": INPUT_ID,
            },
        },
    ]
    children.extend(_detection_box_view(d) for d in detections)
    if text:
        children.append(_subtitle_view(text))
    return {
        "video": {
            "root": {
                "type": "view",
                "width": OUTPUT_W,
                "height": OUTPUT_H,
                "children": children,
            },
        },
        "audio": {
            "inputs": [{"input_id": INPUT_ID}],
        },
    }


def post_scene(body: dict, schedule_time_ms: float, text: str, detections: list):
    print(
        f"update_scene schedule_time_ms={schedule_time_ms:.0f} "
        f"detections={len(detections)} text={text!r}"
    )
    update_body = {**body, "schedule_time_ms": schedule_time_ms}
    api_post(f"/api/output/{OUTPUT_ID}/update", update_body)
    api_post(f"/api/output/{OUTPUT_MP4_ID}/update", update_body)


# Must be large enough to include desync between audio and video, so larger
# than CHUNK_DURATION_MS
SETTLE_MS = 4000
CLEANUP_MS = 10000


class UpdateCoordinator:
    """Merge audio (transcription) and video (detection) events before posting.

    Each push records `last_pts_ms = max` across both tracks. To emit an update we
    pick the oldest buffered entry whose pts is strictly greater than the last
    scheduled pts, and only fire if that candidate has settled — i.e. its pts is
    older than `last_pts_ms - SETTLE_MS`. The candidate is paired with the
    closest-pts entry from the other buffer (or a neutral value if that buffer is
    empty). Entries are NOT removed on emit: keeping text/detection entries around
    lets subsequent detection ticks keep pairing with the same text (so subtitles
    persist across many frames) and vice versa. Entries older than
    `last_pts_ms - CLEANUP_MS` are garbage-collected.

    schedule_time_ms is always the anchor's own pts, and anchors only move forward
    (strictly greater than the previous), so smelter sees monotonic schedule values.
    """

    def __init__(self):
        self._lock = threading.Lock()
        # Audio entries carry a [start, end] pts range; the subtitle only shows
        # while a scene's anchor pts falls inside that range.
        self._audio: deque[tuple[float, float, str]] = deque()
        self._video: deque[tuple[float, list]] = deque()
        self._last_pts_ms: float = float("-inf")
        self._last_scheduled_pts: float = float("-inf")
        # JSON of the last scene body we posted (excluding schedule_time_ms).
        # Skip posts where only the timestamp would change — smelter would render
        # the same frame either way, and YOLO output often repeats across frames
        # with only float-level jitter before reaching the built scene.
        self._last_body_json: str | None = None

    def push_audio(self, start_pts_ms: float, end_pts_ms: float, text: str):
        with self._lock:
            self._audio.append((start_pts_ms, end_pts_ms, text))
            self._last_pts_ms = max(self._last_pts_ms, end_pts_ms)
            ready = list(self._drain_ready())
            self._cleanup()
        self._flush(ready)

    def push_video(self, pts_ms: float, detections: list):
        with self._lock:
            self._video.append((pts_ms, detections))
            self._last_pts_ms = max(self._last_pts_ms, pts_ms)
            ready = list(self._drain_ready())
            self._cleanup()
        self._flush(ready)

    def _flush(self, ready: list):
        for pts, text, detections in ready:
            body = _build_scene_body(text, detections)
            body_json = json.dumps(body, sort_keys=True)
            if body_json == self._last_body_json:
                continue
            self._last_body_json = body_json
            post_scene(body, pts, text, detections)

    def _drain_ready(self):
        # Produce scene updates to fire. On each iteration:
        #
        # 1. In each buffer, find the oldest entry with pts > `_last_scheduled_pts`.
        #    Anything at-or-below that threshold already anchored a previous update —
        #    skipping keeps emitted schedule_time_ms strictly monotonic.
        # 2. Of the two candidates, the one with the smaller pts becomes the anchor
        #    (the next un-scheduled piece of information to fire).
        # 3. Stop if the anchor is newer than `last_pts_ms - SETTLE_MS` — the track
        #    we haven't heard from recently might still deliver something older,
        #    and we'd rather wait than emit and then have to rewind.
        # 4. Pair the anchor with the closest-pts entry from the opposite buffer
        #    (or empty text / no detections if that side is empty). Neither buffer is
        #    popped — entries stay around so future anchors from the other track can
        #    re-pair with them; `_cleanup` ages them out after CLEANUP_MS.
        # 5. Advance `_last_scheduled_pts` to the anchor so step 1 skips it next time.
        cutoff = self._last_pts_ms - SETTLE_MS
        while True:
            # Anchor = oldest entry across both buffers with pts > _last_scheduled_pts.
            # Gather whichever of the two (if any) qualifies from each buffer; the
            # globally smallest pts among them wins.
            candidates: list[tuple[str, tuple, float]] = []
            # Each audio entry contributes TWO candidate anchor pts: the start
            # (when the subtitle appears) and the end (when it should clear).
            # Without an end-anchor, a quiet period between segments would leave
            # the previous subtitle on screen until the next video tick.
            a_cand = self._oldest_audio_anchor_after(self._last_scheduled_pts)
            if a_cand is not None:
                candidates.append(("a", a_cand, a_cand[0]))
            v_cand = self._oldest_after(self._video, self._last_scheduled_pts)
            if v_cand is not None:
                candidates.append(("v", v_cand, v_cand[0]))
            if not candidates:
                break
            anchor_kind, anchor_entry, anchor_pts = min(candidates, key=lambda kv: kv[2])

            # Not old enough yet — stop and wait for more events before committing.
            if anchor_pts >= cutoff:
                break

            # Build the merged update: keep the anchor's own payload, and look up
            # the best match in the opposite buffer (any entry, including ones we've
            # already used — they're kept around until `_cleanup` drops them).
            if anchor_kind == "a":
                # Audio-anchored. `is_end` distinguishes the two anchor kinds:
                #   - start-anchor: segment just began, show its text.
                #   - end-anchor:   segment just ended. Skip — we don't want to
                #                   re-emit just to clear text; the next video
                #                   tick past the range will clear it via the
                #                   range lookup below.
                _, is_end, entry_text = anchor_entry
                if is_end:
                    self._last_scheduled_pts = anchor_pts
                    continue
                text = entry_text
                detections = self._closest(self._video, anchor_pts) or []
            else:
                # Video-anchored: use these detections, plus the text from the
                # audio segment whose [start, end) range covers this pts. Outside
                # any range the subtitle has expired, so text is empty.
                _, detections = anchor_entry
                text = self._audio_text_at(anchor_pts)

            # Advance the watermark so the next iteration skips this anchor.
            self._last_scheduled_pts = anchor_pts
            yield anchor_pts, text, detections

    def _cleanup(self):
        cutoff = self._last_pts_ms - CLEANUP_MS
        while self._audio and self._audio[0][0] < cutoff:
            self._audio.popleft()
        while self._video and self._video[0][0] < cutoff:
            self._video.popleft()

    @staticmethod
    def _oldest_after(buf: deque, after_pts: float):
        for entry in buf:
            if entry[0] > after_pts:
                return entry
        return None

    @staticmethod
    def _closest(buf: deque, target_pts: float):
        if not buf:
            return None
        best = min(buf, key=lambda e: abs(e[0] - target_pts))
        return best[1]

    def _audio_text_at(self, target_pts: float) -> str:
        # Return the text of the audio segment whose [start, end) range contains
        # target_pts, or "" if none does. End is exclusive so a pts *at* an end
        # clears rather than re-displaying the subtitle.
        for start, end, text in self._audio:
            if start <= target_pts < end:
                return text
        return ""

    def _oldest_audio_anchor_after(self, after_pts: float):
        # Each audio entry yields two candidate anchors: its start and its end.
        # Return (anchor_pts, is_end, text) for the smallest pts strictly greater
        # than after_pts, or None.
        best: tuple[float, bool, str] | None = None
        for start, end, text in self._audio:
            for cand_pts, is_end in ((start, False), (end, True)):
                if cand_pts > after_pts and (best is None or cand_pts < best[0]):
                    best = (cand_pts, is_end, text)
        return best


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
        else ["cargo", "run", "-p", "smelter", "-r", "--bin", "main_process"],
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
