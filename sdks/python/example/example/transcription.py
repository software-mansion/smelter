"""Audio transcription via Whisper, gated by Silero VAD, over smelter side channels."""

import queue
import threading
from collections import deque
from collections.abc import Callable, Iterator

import numpy as np
import torch
from faster_whisper import WhisperModel
from silero_vad import VADIterator, load_silero_vad
from smelter import subscribe_audio_channel

WHISPER_SAMPLE_RATE = 16000

# Threshold is below silero's 0.5 default: at 0.5 short/quiet words ("yes", "ok")
# often never trigger and get dropped. speech_pad_ms=0 so reported start/end mark
# the exact windows where speech begins/ends.
VAD_THRESHOLD = 0.3
VAD_MIN_SILENCE_MS = 200
VAD_WINDOW = 512
# Pre-roll: seed a new segment with the last ~10 * 32 ms ≈ 320 ms so the first word
# isn't clipped before silero's onset detection triggers. Doesn't shift start pts.
VAD_PREROLL_WINDOWS = 10
# Force-flush continuous speech this often so a subtitle appears without waiting for
# a pause. Text is only emitted once a segment ends, so this also caps how far it
# lags the start pts: must stay under SETTLE_MS (minus whisper), which itself fits
# inside SIDE_CHANNEL_DELAY_MS.
VAD_MAX_SEGMENT_MS = 5000


def run_transcription(on_segment: Callable[[str, float, float], None]):
    """Connect to audio side channel, transcribe, call on_segment(text, start_ns, end_ns).

    Whisper inference is slower than real time on the first call (and variable afterwards),
    so socket reads + VAD run on a reader thread while inference runs on the main thread.
    If the main thread blocked on inference while the socket buffered, the smelter side
    channel would back up and start dropping batches — the "client channel full" warnings
    on the server side.
    """
    print("Waiting for side channel sockets...")

    print("Loading Whisper + Silero VAD models...")
    model = WhisperModel("base", compute_type="int8")
    vad_model = load_silero_vad()
    print("Models loaded. Listening...\n")

    # Each item: (mono float32 segment @ WHISPER_SAMPLE_RATE, start_pts_nanos, end_pts_nanos).
    segment_queue: queue.Queue[tuple[np.ndarray, int, int]] = queue.Queue()

    def reader():
        vad_iter = VADIterator(
            vad_model,
            threshold=VAD_THRESHOLD,
            sampling_rate=WHISPER_SAMPLE_RATE,
            min_silence_duration_ms=VAD_MIN_SILENCE_MS,
            speech_pad_ms=0,
        )
        pre_buffer: deque[np.ndarray] = deque(maxlen=VAD_PREROLL_WINDOWS)
        speech_windows: list[np.ndarray] = []
        # `speech_start_pts_nanos is not None` doubles as the "in speech" flag.
        speech_start_pts_nanos: int | None = None

        for window, chunk_pts_nanos in _stream_16k_windows():
            pre_buffer.append(window)

            match vad_iter(torch.from_numpy(window), return_seconds=True):
                case {"start": _}:
                    speech_start_pts_nanos = chunk_pts_nanos
                    speech_windows = list(pre_buffer)

                case {"end": _} if speech_start_pts_nanos is not None:
                    segment_queue.put(
                        (np.concatenate(speech_windows), speech_start_pts_nanos, chunk_pts_nanos)
                    )
                    speech_start_pts_nanos = None
                    speech_windows = []

                case _ if speech_start_pts_nanos is not None:
                    speech_windows.append(window)

            # Force-flush if the segment outgrew VAD_MAX_SEGMENT_MS without an 'end'.
            if (
                speech_start_pts_nanos is not None
                and chunk_pts_nanos - speech_start_pts_nanos >= VAD_MAX_SEGMENT_MS * 1_000_000
            ):
                segment_queue.put(
                    (np.concatenate(speech_windows), speech_start_pts_nanos, chunk_pts_nanos)
                )
                # Mid-speech cut, not an onset, so start fresh without pre-roll.
                speech_start_pts_nanos = chunk_pts_nanos
                speech_windows = []

    reader_thread = threading.Thread(target=reader, daemon=True)
    reader_thread.start()

    while True:
        audio, start_pts_nanos, end_pts_nanos = segment_queue.get()
        if segment_queue.qsize() > 0:
            print(f"Whisper is behind — {segment_queue.qsize()} segment(s) queued")
        segments, _ = model.transcribe(audio, language="en")
        text = " ".join(segment.text.strip() for segment in segments).strip()
        if text:
            on_segment(text, start_pts_nanos, end_pts_nanos)


def _stream_16k_windows() -> Iterator[tuple[np.ndarray, int]]:
    """Yield (window, chunk_pts_nanos) for every 512-sample 16 kHz mono window.

    The pts is the incoming chunk's `start_pts_nanos`, so a transcribed segment's
    timestamp matches its source chunk with no sample-counter drift. Residual
    samples (<512) carry across chunks so windowing stays gap-free.
    """
    residual = np.empty(0, dtype=np.float32)

    for batch in subscribe_audio_channel("input"):
        mono = batch.to_mono()
        if mono.size == 0:
            continue
        # Per-batch rate, not cached: a mid-stream rate change would distort the
        # resample and misalign the pts.
        chunk = _resample_to_16k(mono, batch.sample_rate).astype(np.float32, copy=False)
        if chunk.size == 0:
            continue

        audio = np.concatenate([residual, chunk]) if residual.size else chunk
        n_windows = audio.size // VAD_WINDOW

        for i in range(n_windows):
            # Copy: `audio` is rotated via `residual`, so the slice would otherwise
            # alias memory the next chunk overwrites.
            window = audio[i * VAD_WINDOW : (i + 1) * VAD_WINDOW].copy()
            yield window, batch.start_pts_nanos

        residual = audio[n_windows * VAD_WINDOW :].copy()


def _resample_to_16k(audio: np.ndarray, sample_rate: int) -> np.ndarray:
    if sample_rate == WHISPER_SAMPLE_RATE:
        return audio
    target_len = int(len(audio) * WHISPER_SAMPLE_RATE / sample_rate)
    if target_len <= 0:
        # Too few samples to resample; return empty so the caller skips this batch
        # instead of feeding source-rate audio downstream.
        return np.empty(0, dtype=np.float32)
    indices = np.linspace(0, len(audio) - 1, target_len)
    return np.interp(indices, np.arange(len(audio)), audio).astype(np.float32)
