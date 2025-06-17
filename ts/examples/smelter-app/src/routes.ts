import type { Express } from 'express';
import express, { json } from 'express';
import { LayoutValues, store } from './store';
import { addTwitchStream } from './addTwitchStream';
import { SmelterInstance } from './smelter';

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
    const streamId = req.body.streamId;
    store.getState().removeStream(streamId);
    await SmelterInstance.unregisterInput(streamId);
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
      availableStreams: state.availableStreams,
      connectedStreamIds: state.connectedStreamIds,
      audioStreamId: state.audioStreamId,
      layout: state.layout,
    };
  })()
    .then(result => res.send(result))
    .catch(err => next(err));
});
