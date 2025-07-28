import { LayoutValues, store } from './store';
import { SmelterInstance } from './smelter';
import path from 'path';
import Fastify from 'fastify';
import type { Static } from '@sinclair/typebox';
import { Type } from '@sinclair/typebox';
import { spawn } from './utils';

export const RequestWithStreamId = Type.Object({
  streamId: Type.String(),
});

export const app = Fastify({
  logger: true,
});

app.post<{ Body: Static<typeof RequestWithStreamId> }>(
  '/add-stream',
  { schema: { body: RequestWithStreamId } },
  async (req, res) => {
    await connectStream(req.body.streamId);
    res.status(200).send({ status: 'ok' });
  }
);

app.post<{ Body: Static<typeof RequestWithStreamId> }>('/remove-stream', async (req, res) => {
  const streamId: string = req.body.streamId;
  store.getState().removeStream(streamId);
  try {
    await SmelterInstance.unregisterInput(streamId);
  } catch (err: any) {
    if (err.body?.error_code !== 'INPUT_STREAM_NOT_FOUND') {
      throw err;
    }
  }
  res.status(200).send({ status: 'ok' });
});

app.post<{ Body: Static<typeof RequestWithStreamId> }>('/select-audio', async (req, res) => {
  const streamId = req.body.streamId;
  store.getState().selectAudioStream(streamId);
  res.status(200).send({ status: 'ok' });
});

export const UpdateLayout = Type.Object({
  layout: Type.Union([Type.Literal('grid'), Type.Literal('primary-on-left')]),
});

app.post<{ Body: Static<typeof UpdateLayout> }>('/update-layout', async (req, res) => {
  const layout = req.body.layout;
  if (!LayoutValues.includes(layout)) {
    throw new Error(`Unknown layout ${layout}`);
  }
  store.getState().setLayout(layout);
  res.status(200).send({ status: 'ok' });
});

app.get('/state', async (_req, res) => {
  const state = store.getState();
  res.status(200).send({
    availableStreams: state.availableStreams.filter(
      stream =>
        stream.live || state.connectedStreamIds.includes(stream.id) || stream.type === 'static'
    ),
    connectedStreamIds: state.connectedStreamIds,
    audioStreamId: state.audioStreamId,
    layout: state.layout,
  });
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
        videoDecoder: 'vulkan_h264',
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
    const streamlinkOutput = await spawn(
      'streamlink',
      ['--stream-url', `https://www.twitch.tv/${streamId}`, '720p,720p60,best'],
      {
        stdio: 'pipe',
      }
    );
    const hlsPlaylistUrl = streamlinkOutput.stdout.trim();
    console.log({ hlsPlaylistUrl });
    await SmelterInstance.registerInput(streamId, {
      type: 'hls',
      url: hlsPlaylistUrl,
    });
    state.addStream(streamId);
  }
}
