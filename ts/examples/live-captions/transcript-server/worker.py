import numpy
import threading
import logging
import torch
import queue
import asyncio
import torchaudio
import math
import uuid
from fastapi import WebSocket
from typing import NamedTuple
from transformers import AutoModelForSpeechSeq2Seq, AutoProcessor
from transformers.pipelines import pipeline


WHISPER_MODEL_SAMPLE_RATE = 16000


logger = logging.getLogger(__name__)
logging.basicConfig(level=logging.DEBUG)


class TranscriptionHandler:
    def __init__(self):
        self.async_loop = asyncio.get_event_loop()
        self.transcription_service = TranscriptionService()
        self.transcription_worker = TranscriptionWorker(self.transcription_service, self.async_loop)
        self.transcription_worker.start()

    async def accept(self, websocket: WebSocket) -> None:
        """
        Accepts websocket connection and starts transcript streaming.
        """

        await websocket.accept()

        connection_id = str(uuid.uuid4())
        request_queue = self.transcription_worker.get_request_queue(connection_id)
        result_queue: asyncio.Queue[str] = asyncio.Queue()

        asyncio.create_task(self.__send_results(websocket, result_queue))
        audio_chunks = await asyncio.to_thread(self.__load_audio_chunks_blocking)

        for chunk in audio_chunks:
            transcription_request = TranscriptionRequest(
                audio=chunk,
                result_queue=result_queue
            )

            try:
                request_queue.put_nowait(transcription_request)
            except queue.Full as e:
                logger.error(f"Failed to enqueue the transcription request: {e}")

    def __load_audio_chunks_blocking(self) -> list[numpy.ndarray]:
        """
        Loads audio from file, resamples it to 16kHz if needed and
        extracts single audio channel. Returns list of transcription ready chunks.
        """

        audio, sample_rate = torchaudio.load("./static/audio.wav")

        if sample_rate != WHISPER_MODEL_SAMPLE_RATE:
            resampler = torchaudio.transforms.Resample(orig_freq=sample_rate, new_freq=WHISPER_MODEL_SAMPLE_RATE)
            audio = resampler(audio)

        if audio.shape[0] > 1:
            audio = torch.mean(audio, dim=0, keepdim=True)

        waveform = audio.squeeze().numpy().astype("float32")

        chunk_duration = 15
        chunk_samples = chunk_duration * WHISPER_MODEL_SAMPLE_RATE
        chunk_shift_samples = (chunk_duration - 1) * WHISPER_MODEL_SAMPLE_RATE

        nchunks = math.ceil(len(audio) / chunk_shift_samples)
        chunks: list[numpy.ndarray] = []

        for i in range(nchunks):
            start = i * chunk_shift_samples
            end = start + chunk_samples
            chunk = waveform[start:end]
            chunks.append(chunk)

        return chunks


    async def __send_results(self, websocket: WebSocket, result_queue: asyncio.Queue) -> None:
        while True:
            try:
                transcription = await result_queue.get()
                await websocket.send_text(transcription)
            except Exception as e:
                logger.error(e)


class TranscriptionRequest(NamedTuple):
    audio: numpy.ndarray
    result_queue: asyncio.Queue[str]


class TranscriptionService:
    model_id = "openai/whisper-large-v3-turbo"

    def __init__(self):
        device = self.__negotiate_device()
        torch_dtype = self.__negotiate_torch_dtype()

        model = AutoModelForSpeechSeq2Seq.from_pretrained(
            self.model_id,
            torch_dtype=torch_dtype,
            low_cpu_mem_usage=True,
            use_safetensors=True
        )
        model.to(device)

        processor = AutoProcessor.from_pretrained(self.model_id)

        self.pipe = pipeline(
            "automatic-speech-recognition",
            model=model,
            device=device,
            torch_dtype=torch_dtype,
            tokenizer=processor.tokenizer,
            feature_extractor=processor.feature_extractor,
        )

    def transcribe(self, audio: numpy.ndarray) -> str:
        """
        Transcribes the given waveform.
        """

        logger.info("Processing transcribe request")
        result = self.pipe(audio, generate_kwargs={"language": "english"})

        return result[0]["text"] if isinstance(result, list) else result["text"]

    def __negotiate_device(self) -> str:
        """
        Negotiates the best device for the available backend.
        """

        if torch.cuda.is_available():
            return "cuda:0"
        elif torch.backends.mps.is_available():
            return "mps"
        else:
            return "cpu"

    def __negotiate_torch_dtype(self) -> torch.dtype:
        """
        Negotiates the best torch_dtype for the available backend.
        """

        return torch.float16 if torch.cuda.is_available() else torch.float32


class TranscriptionWorker(threading.Thread):
    is_running = True

    def __init__(self, transcription_service: TranscriptionService, parent_event_loop: asyncio.AbstractEventLoop):
        super().__init__()
        self.transcription_service = transcription_service
        self.parent_event_loop = parent_event_loop
        self.connections = {}

    def get_request_queue(self, connection_id: str) -> queue.Queue[TranscriptionRequest]:
        """
        Returns request queue for the given connection.
        """

        if connection_id in self.connections:
            return self.connections[connection_id]
        else:
            self.connections[connection_id] = queue.Queue()
            return self.connections[connection_id]

    def run(self) -> None:
        while self.is_running:
            # We need to copy connections dict, as it's size
            # can change during iteration.

            for id, request_queue in self.connections.copy().items():
                logger.debug(f"Processing queue {id}")

                try:
                    request = request_queue.get()
                except queue.Empty:
                    continue

                try:
                    transcription = self.transcription_service.transcribe(request.audio)
                    asyncio.run_coroutine_threadsafe(
                        request.result_queue.put(transcription),
                        self.parent_event_loop
                    )
                except Exception as e:
                    logger.error(f"Error processing transcription: {e}")

    def stop(self) -> None:
        self.is_running = False
