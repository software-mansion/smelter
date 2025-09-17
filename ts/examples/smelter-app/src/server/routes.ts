import Fastify from 'fastify';
import { Type } from '@sinclair/typebox';
import type { Static, TypeBoxTypeProvider } from '@fastify/type-provider-typebox';

import { state } from './serverState';
import { TwitchChannelSuggestions } from '../twitch/ChannelMonitor';
import type { RoomInputState } from './roomState';
import { config } from '../config';
import fs from 'fs';
import path from 'path';

type RoomIdParams = { Params: { roomId: string } };
type RoomAndInputIdParams = { Params: { roomId: string; inputId: string } };

type InputState = {
  inputId: string;
  title: string;
  description: string;
  sourceState: 'live' | 'offline' | 'unknown' | 'always-live';
  status: 'disconnected' | 'pending' | 'connected';
  volume: number;

  twitchChannelId?: string;
};

export const routes = Fastify({
  logger: config.logger,
}).withTypeProvider<TypeBoxTypeProvider>();

routes.get('/suggestions/mp4s', async (_req, res) => {
  const mp4sDir = path.resolve(process.cwd(), 'mp4s');
  let files: string[] = [];
  try {
    files = await fs.promises.readdir(mp4sDir);
  } catch {
    res.status(500).send({ error: 'Failed to read mp4s directory' });
    return;
  }
  const mp4Files = files.filter(f => f.toLowerCase().endsWith('.mp4'));
  res.status(200).send({ mp4s: mp4Files });
});

routes.get('/suggestions/twitch', async (_req, res) => {
  res.status(200).send({ twitch: TwitchChannelSuggestions.getTopStreams() });
});

routes.post('/room', async (_req, res) => {
  console.log('[request] Create new room');
  const { roomId, room } = await state.createRoom();
  res.status(200).send({ roomId, whepUrl: room.getWhepUrl() });
});

routes.get<RoomIdParams>('/room/:roomId', async (req, res) => {
  const { roomId } = req.params;
  const room = state.getRoom(roomId);
  const [inputs, layout] = room.getState();

  res.status(200).send({
    inputs: inputs.map(input => publicInputState(input)),
    layout,
    whepUrl: room.getWhepUrl(),
    pendingDelete: room.pendingDelete,
  });
});

const UpdateRoomSchema = Type.Object({
  inputOrder: Type.Optional(Type.Array(Type.String())),
  layout: Type.Optional(
    Type.Union([
      Type.Literal('grid'),
      Type.Literal('primary-on-left'),
      Type.Literal('primary-on-top'),
      Type.Literal('secondary-in-corner'),
    ])
  ),
});

routes.post<RoomIdParams & { Body: Static<typeof UpdateRoomSchema> }>(
  '/room/:roomId',
  { schema: { body: UpdateRoomSchema } },
  async (req, res) => {
    const { roomId } = req.params;
    console.log('[request] Update room', { body: req.body, roomId });
    const room = state.getRoom(roomId);
    if (req.body.inputOrder) {
      room.reorderInputs(req.body.inputOrder);
    }

    if (req.body.layout) {
      room.updateLayout(req.body.layout);
    }

    res.status(200).send({ status: 'ok' });
  }
);

const AddInputSchema = Type.Union([
  Type.Object({
    type: Type.Literal('twitch-channel'),
    twitchChannelId: Type.String(),
  }),
  Type.Object({
    type: Type.Literal('kick-channel'),
    kickChannelId: Type.String(),
  }),
  Type.Object({
    type: Type.Literal('local-mp4'),
    mp4Url: Type.String(),
  }),
]);

routes.post<RoomIdParams & { Body: Static<typeof AddInputSchema> }>(
  '/room/:roomId/input',
  { schema: { body: AddInputSchema } },
  async (req, res) => {
    const roomId = req.params.roomId;
    console.log('[request] Create input', { body: req.body, roomId });
    const room = state.getRoom(roomId);
    const inputId = await room.addNewInput(req.body);
    console.log('[info] Added input', { inputId });
    if (inputId) {
      await room.connectInput(inputId);
    }
    res.status(200).send({ inputId });
  }
);

routes.post<RoomAndInputIdParams>('/room/:roomId/input/:inputId/connect', async (req, res) => {
  const { roomId, inputId } = req.params;
  console.log('[request] Connect input', { roomId, inputId });
  const room = state.getRoom(roomId);
  await room.connectInput(inputId);

  res.status(200).send({ status: 'ok' });
});

routes.post<RoomAndInputIdParams>('/room/:roomId/input/:inputId/disconnect', async (req, res) => {
  const { roomId, inputId } = req.params;
  console.log('[request] Disconnect input', { roomId, inputId });
  const room = state.getRoom(roomId);
  await room.disconnectInput(inputId);

  res.status(200).send({ status: 'ok' });
});

const UpdateInputSchema = Type.Object({
  volume: Type.Number({ maximum: 1, minimum: 0 }),
});

routes.post<RoomAndInputIdParams & { Body: Static<typeof UpdateInputSchema> }>(
  '/room/:roomId/input/:inputId',
  { schema: { body: UpdateInputSchema } },
  async (req, res) => {
    const { roomId, inputId } = req.params;
    console.log('[request] Update input', { roomId, inputId, body: req.body });

    const room = state.getRoom(roomId);
    await room.updateInput(inputId, req.body);

    res.status(200).send({ status: 'ok' });
  }
);

routes.delete<RoomAndInputIdParams>('/room/:roomId/input/:inputId', async (req, res) => {
  const { roomId, inputId } = req.params;
  console.log('[request] Remove input', { roomId, inputId });
  const room = state.getRoom(roomId);
  await room.removeInput(inputId);

  res.status(200).send({ status: 'ok' });
});

function publicInputState(input: RoomInputState): InputState {
  if (input.type === 'local-mp4') {
    return {
      inputId: input.inputId,
      title: input.metadata.title,
      description: input.metadata.description,
      sourceState: 'always-live',
      status: input.status,
      volume: input.volume,
    };
  } else if (input.type === 'twitch-channel') {
    return {
      inputId: input.inputId,
      title: input.metadata.title,
      description: input.metadata.description,
      sourceState: input.monitor.isLive() ? 'live' : 'offline',
      status: input.status,
      volume: input.volume,
      twitchChannelId: input.channelId,
    };
  } else if (input.type === 'kick-channel') {
    return {
      inputId: input.inputId,
      title: input.metadata.title,
      description: input.metadata.description,
      sourceState: 'unknown',
      status: input.status,
      volume: input.volume,
    };
  }
  throw new Error('Unknown input state');
}
