import type { Express } from 'express';
import express, { json } from 'express';
import { LayoutValues, store } from './store';
import { SmelterInstance } from './smelter';
import { SMELTER_WORKDIR, waitForStream } from './manageHlsToHlsStreams';
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
  addTwitchStream(req.body.streamId)
    .then(() => res.send({}))
    .catch(err => next(err));
});

app.post('/remove-stream', (req, res, next) => {
  (async () => {
    const streamId: string = req.body.streamId;
    store.getState().removeStream(streamId);
    try {
      await SmelterInstance.unregisterInput(streamId);
    } catch (err) {
      console.log('Unregister err', err, (err as any)?.body);
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
        stream => stream.available || state.connectedStreamIds.includes(stream.id)
      ),
      connectedStreamIds: state.connectedStreamIds,
      audioStreamId: state.audioStreamId,
      layout: state.layout,
    };
  })()
    .then(result => res.send(result))
    .catch(err => next(err));
});

async function addTwitchStream(streamId: string): Promise<void> {
  let state = store.getState();
  if (state.availableStreams.filter(stream => stream.id == streamId).length === 0) {
    throw new Error(`Unknown streamId: ${streamId}`);
  }

  if (state.connectedStreamIds.filter(id => id === streamId).length > 0) {
    throw new Error('Already connected stream.');
  }

  try {
    await waitForStream(streamId);

    await SmelterInstance.registerInput(streamId, {
      type: 'hls',
      url: path.join(SMELTER_WORKDIR, streamId, 'index.m3u8'),
    });
    state.addStream(streamId);
  } catch (err: any) {
    console.log(err.body, err);
    throw err;
  }
}
