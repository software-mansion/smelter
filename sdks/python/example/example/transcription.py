"""Audio transcription via Whisper over smelter side channels."""

import time
from collections.abc import Callable

import numpy as np
from faster_whisper import WhisperModel
from smelter import SideChannelKind, SideChannelManager

WHISPER_SAMPLE_RATE = 16000
CHUNK_DURATION_S = 3


def run_transcription(socket_dir: str, on_segment: Callable[[str, float], None]):
    """Connect to audio side channel, transcribe, and call on_segment(text, pts_nanos)."""
    manager = SideChannelManager(socket_dir)

    print("Waiting for side channel sockets...")
    while True:
        channels = manager.list()
        audio_channels = [c for c in channels if c.kind == SideChannelKind.AUDIO]
        if audio_channels:
            break
        time.sleep(0.2)

    info = audio_channels[0]
    print(f"Connecting to audio channel for input '{info.input_id}'...")

    print("Loading Whisper model (base)...")
    model = WhisperModel("base", compute_type="int8")
    print("Model loaded. Listening...\n")

    with manager.connect(info) as conn:
        buffer = np.empty(0, dtype=np.float32)
        chunk_start_pts_nanos: int | None = None

        while True:
            batch = conn.recv()

            if chunk_start_pts_nanos is None:
                chunk_start_pts_nanos = batch.start_pts_nanos

            samples = np.array(batch.samples, dtype=np.float64)
            if batch.channel_count > 1:
                samples = samples.reshape(-1, batch.channel_count).mean(axis=1)
            samples = samples.astype(np.float32)

            if batch.sample_rate != WHISPER_SAMPLE_RATE:
                ratio = WHISPER_SAMPLE_RATE / batch.sample_rate
                target_len = int(len(samples) * ratio)
                indices = np.linspace(0, len(samples) - 1, target_len)
                samples = np.interp(indices, np.arange(len(samples)), samples)

            buffer = np.concatenate([buffer, samples])

            if len(buffer) >= WHISPER_SAMPLE_RATE * CHUNK_DURATION_S:
                segments, _ = model.transcribe(buffer, language="en")
                for segment in segments:
                    pts_nanos = chunk_start_pts_nanos + int(segment.start * 1e9)
                    text = segment.text.strip()
                    if text:
                        on_segment(text, pts_nanos)
                buffer = np.empty(0, dtype=np.float32)
                chunk_start_pts_nanos = None
