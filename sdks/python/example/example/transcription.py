"""Audio transcription via Whisper over smelter side channels."""

import queue
import threading
from collections.abc import Callable

import numpy as np
from faster_whisper import WhisperModel
from smelter import subscribe_audio_channel

WHISPER_SAMPLE_RATE = 16000
CHUNK_DURATION_MS = 3000


def run_transcription(on_segment: Callable[[str, float, float], None]):
    """Connect to audio side channel, transcribe, call on_segment(text, start_ns, end_ns).

    Whisper inference is slower than real time on the first call (and variable afterwards),
    so socket reads and inference run on separate threads. If the main thread blocked on
    inference while the socket buffered, the smelter side channel would back up and start
    dropping batches — the "client channel full" warnings on the server side.
    """
    print("Waiting for side channel sockets...")

    print("Loading Whisper model (base)...")
    model = WhisperModel("base", compute_type="int8")
    print("Model loaded. Listening...\n")

    # Each item: (mono float32 chunk at WHISPER_SAMPLE_RATE, pts_nanos of first sample).
    chunk_queue: queue.Queue[tuple[np.ndarray, int]] = queue.Queue()

    def reader():
        buffer = np.empty(0, dtype=np.float32)
        chunk_start_pts_nanos: int | None = None
        for batch in subscribe_audio_channel("input"):
            if chunk_start_pts_nanos is None:
                chunk_start_pts_nanos = batch.start_pts_nanos

            samples = batch.to_mono()

            if batch.sample_rate != WHISPER_SAMPLE_RATE:
                ratio = WHISPER_SAMPLE_RATE / batch.sample_rate
                target_len = int(len(samples) * ratio)
                indices = np.linspace(0, len(samples) - 1, target_len)
                samples = np.interp(indices, np.arange(len(samples)), samples).astype(np.float32)

            buffer = np.concatenate([buffer, samples])

            if len(buffer) >= WHISPER_SAMPLE_RATE * CHUNK_DURATION_MS // 1000:
                chunk_queue.put((buffer, chunk_start_pts_nanos))
                buffer = np.empty(0, dtype=np.float32)
                chunk_start_pts_nanos = None

    reader_thread = threading.Thread(target=reader, daemon=True)
    reader_thread.start()

    while True:
        chunk, chunk_start_pts_nanos = chunk_queue.get()
        if chunk_queue.qsize() > 0:
            print(f"Whisper is behind — {chunk_queue.qsize()} chunk(s) queued")
        segments, _ = model.transcribe(chunk, language="en")
        for segment in segments:
            start_pts_nanos = chunk_start_pts_nanos + int(segment.start * 1e9)
            end_pts_nanos = chunk_start_pts_nanos + int(segment.end * 1e9)
            text = segment.text.strip()
            if text:
                on_segment(text, start_pts_nanos, end_pts_nanos)
