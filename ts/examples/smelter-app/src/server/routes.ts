import Fastify from 'fastify';
import { Type } from '@sinclair/typebox';
import type { Static, TypeBoxTypeProvider } from '@fastify/type-provider-typebox';

import { state } from './serverState';
import { TwitchChannelSuggestions } from '../twitch/ChannelMonitor';
import type { RoomInputState } from './roomState';
import { config } from '../config';

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

routes.post('/room', async (_req, res) => {
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
]);

routes.post<RoomIdParams & { Body: Static<typeof AddInputSchema> }>(
  '/room/:roomId/input',
  { schema: { body: AddInputSchema } },
  async (req, res) => {
    const room = state.getRoom(req.params.roomId);
    console.log(req.body);
    const inputId = await room.addNewInput(req.body);

    res.status(200).send({ inputId });
  }
);

routes.post<RoomAndInputIdParams>('/room/:roomId/input/:inputId/connect', async (req, res) => {
  const room = state.getRoom(req.params.roomId);
  await room.connectInput(req.params.inputId);

  res.status(200).send({ status: 'ok' });
});

routes.post<RoomAndInputIdParams>('/room/:roomId/input/:inputId/disconnect', async (req, res) => {
  const room = state.getRoom(req.params.roomId);
  await room.disconnectInput(req.params.inputId);

  res.status(200).send({ status: 'ok' });
});

const UpdateInputSchema = Type.Object({
  volume: Type.Number({ maximum: 1, minimum: 0 }),
});

routes.post<RoomAndInputIdParams & { Body: Static<typeof UpdateInputSchema> }>(
  '/room/:roomId/input/:inputId',
  { schema: { body: UpdateInputSchema } },
  async (req, res) => {
    const room = state.getRoom(req.params.roomId);
    await room.updateInput(req.params.inputId, req.body);

    res.status(200).send({ status: 'ok' });
  }
);

routes.delete<RoomAndInputIdParams>('/room/:roomId/input/:inputId', async (req, res) => {
  const room = state.getRoom(req.params.roomId);
  await room.removeInput(req.params.inputId);

  res.status(200).send({ status: 'ok' });
});

routes.get('/suggestions', async (_req, res) => {
  res.status(200).send({ twitch: TwitchChannelSuggestions.getTopStreams() });
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
