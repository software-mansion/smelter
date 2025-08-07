import logging
from fastapi import FastAPI, WebSocket
from .worker import TranscriptionHandler
import numpy as np


logger = logging.getLogger(__name__)
logging.basicConfig(level=logging.DEBUG)


app = FastAPI()
transcription_handler = TranscriptionHandler()


@app.websocket("/ws")
async def websocket_endpoint(websocket: WebSocket):
    print("connection")

    await websocket.accept()

    while True:
        b = await websocket.receive_bytes()
        pcm = np.frombuffer(b, dtype=np.int16).astype(np.float32, order='C') / 32768.0
        print(pcm)


    # logger.info("Received websocket connection")
    # await transcription_handler.accept(websocket)

    # # TODO: This future will never finish.
    # #       It's a temporary hack, do something with this later
    # await asyncio.Future()
