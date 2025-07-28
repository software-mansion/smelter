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

type RoomIdParams = { Params: { roomId: string } };
type RoomAndInputIdParams = { Params: { roomId: string; inputId: string } };

export const app = Fastify({
  logger: true,
}).withTypeProvider<TypeBoxTypeProvider>();

app.put('/room', async (_req, res) => {
  const { roomId, room } = await state.createRoom();
  res.status(200).send({ roomId, whepUrl: room.getWhepUrl() });
});

app.get<RoomIdParams>('/room/:roomId', async (req, res) => {
  const { roomId } = req.params;
  const room = state.getRoom(roomId);
  res.status(200).send({ inputs: room.getState(), whepUrl: room.getWhepUrl() });
});

app.post<RoomAndInputIdParams>('/room/:roomId/input/:inputId/connect', async (req, res) => {
  const room = state.getRoom(req.params.roomId);
  room.connect();

  await connectStream(req.body.streamId);
});

app.post<{ Body: Static<typeof StreamAndRoomId> }>(
  '/add-stream',
  { schema: { body: StreamAndRoomId } },
  async (req, res) => {
    await connectStream(req.body.streamId);
    res.status(200).send({ status: 'ok' });
  }
);

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
