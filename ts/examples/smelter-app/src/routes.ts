import type { Express } from 'express';
import express, { json } from 'express';
import { LayoutValues, store } from './store';
import { SmelterInstance } from './smelter';
import { SMELTER_WORKDIR } from './manageHlsToHlsStreams';
import path from 'path';

export const app: Express = express();

app.use(json());
app.use((err: Error, _req: any, res: any, _next: any) => {
  console.error(err.stack);
  if ((err as any).body) {
    console.log((err as any).body);
  }
  res.status(500).send({ msg: err.message, stack: err.stack });
});

app.post('/add-stream', (req, res, next) => {
  connectStream(req.body.streamId)
    .then(() => res.send({}))
    .catch(err => next(err));
});

app.post('/remove-stream', (req, res, next) => {
  (async () => {
    const streamId: string = req.body.streamId;
    store.getState().removeStream(streamId);
    try {
      await SmelterInstance.unregisterInput(streamId);
    } catch (err: any) {
      if (err.body?.error_code !== 'INPUT_STREAM_NOT_FOUND') {
        throw err;
      }
    }
  })()
    .then(() => res.send({}))
    .catch(err => next(err));
});

app.post('/select-audio', (req, res, next) => {
  (async () => {
    const streamId = req.body.streamId;
    store.getState().selectAudioStream(streamId);
  })()
    .then(() => res.send({}))
    .catch(err => next(err));
});

app.post('/update-layout', (req, res, next) => {
  (async () => {
    const layout = req.body.layout;
    if (!LayoutValues.includes(layout)) {
      throw new Error(`Unknown layout ${layout}`);
    }
    store.getState().setLayout(layout);
  })()
    .then(() => res.send({}))
    .catch(err => next(err));
});

app.get('/state', async (_req, res, next) => {
  (async () => {
    const state = store.getState();
    return {
      availableStreams: state.availableStreams.filter(
        stream =>
          stream.localHlsReady ||
          state.connectedStreamIds.includes(stream.id) ||
          stream.type === 'static'
      ),
      connectedStreamIds: state.connectedStreamIds,
      audioStreamId: state.audioStreamId,
      layout: state.layout,
    };
  })()
    .then(result => res.send(result))
    .catch(err => next(err));
});

async function connectStream(streamId: string): Promise<void> {
  let state = store.getState();
  let stream = state.availableStreams.find(stream => stream.id === streamId);
  if (!stream) {
    throw new Error(`Unknown streamId: ${streamId}`);
  }

  if (stream.type === 'static') {
    try {
      await SmelterInstance.registerInput(streamId, {
        type: 'mp4',
        serverPath: path.join(process.cwd(), `${streamId}.mp4`),
        loop: true,
        videoDecoder: 'vulkan_h264'
      });
      state.addStream(streamId);
    } catch (err: any) {
      if (err.body?.error_code === 'INPUT_STREAM_ALREADY_REGISTERED') {
        state.addStream(streamId);
      }
      console.log(err.body, err);
      throw err;
    }
  } else {
    try {
      await SmelterInstance.registerInput(streamId, {
        type: 'hls',
        url: path.join(SMELTER_WORKDIR, streamId, 'index.m3u8'),
      });
      state.addStream(streamId);
    } catch (err: any) {
      if (err.body?.error_code === 'INPUT_STREAM_ALREADY_REGISTERED') {
        state.addStream(streamId);
      }
      console.log(err.body, err);
      throw err;
    }
  }
}
