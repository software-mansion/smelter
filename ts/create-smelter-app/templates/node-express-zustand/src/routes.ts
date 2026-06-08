import type { Express } from 'express';
import express, { json } from 'express';
import { SmelterInstance } from './smelter';
import { store } from './store';

export const app: Express = express();

app.use(json());

// curl -XPOST -H "Content-type: application/json" -d '{ "inputId": "input_1", "mp4Url": "https://smelter.dev/videos/template-scene-race.mp4" }' 'http://localhost:3000/add-stream'
app.post('/add-stream', async (req, res) => {
  await SmelterInstance.registerInput(req.body.inputId, {
    type: 'mp4',
    url: req.body.mp4Url,
  });
  res.send({});
});

// curl -XPOST 'http://localhost:3000/toggle-instructions'
app.post('/toggle-instructions', async (_req, res) => {
  store.getState().toggleInstructions();
  res.send({});
});
