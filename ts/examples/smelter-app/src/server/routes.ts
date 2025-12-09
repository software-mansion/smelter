import Fastify from 'fastify';
import { Type } from '@sinclair/typebox';
import type { Static, TypeBoxTypeProvider } from '@fastify/type-provider-typebox';
import { state } from './serverState';
import { TwitchChannelSuggestions } from '../twitch/TwitchChannelMonitor';
import type { RoomInputState, RegisterInputOptions } from './roomState';
import { config } from '../config';
import mp4SuggestionsMonitor from '../mp4/mp4SuggestionMonitor';
import { KickChannelSuggestions } from '../kick/KickChannelMonitor';
import type { ShaderConfig } from '../shaders/shaders';
import shadersController from '../shaders/shaders';

type RoomIdParams = { Params: { roomId: string } };
type RoomAndInputIdParams = { Params: { roomId: string; inputId: string } };

type InputState = {
  inputId: string;
  title: string;
  description: string;
  showTitle: boolean;
  sourceState: 'live' | 'offline' | 'unknown' | 'always-live';
  status: 'disconnected' | 'pending' | 'connected';
  volume: number;
  shaders: ShaderConfig[];
  channelId?: string;
};

export const routes = Fastify({
  logger: config.logger,
}).withTypeProvider<TypeBoxTypeProvider>();

routes.get('/suggestions/mp4s', async (_req, res) => {
  res.status(200).send({ mp4s: mp4SuggestionsMonitor.mp4Files });
});

routes.get('/suggestions/twitch', async (_req, res) => {
  res.status(200).send({ twitch: TwitchChannelSuggestions.getTopStreams() });
});

routes.get('/suggestions/kick', async (_req, res) => {
  console.log('[request] Get kick suggestions');
  res.status(200).send({ kick: KickChannelSuggestions.getTopStreams() });
});

routes.get('/suggestions', async (_req, res) => {
  res.status(200).send({ twitch: TwitchChannelSuggestions.getTopStreams() });
});

routes.post('/room', async (_req, res) => {
  console.log('[request] Create new room');
  const body = _req.body as { initInputs?: unknown } | undefined;

  if (body?.initInputs !== undefined && !Array.isArray(body.initInputs)) {
    return res.status(400).send({ error: 'initInputs must be an array' });
  }

  const initInputs = (body?.initInputs as RegisterInputOptions[]) || [];

  const { roomId, room } = await state.createRoom(initInputs);
  res.status(200).send({ roomId, whepUrl: room.getWhepUrl() });
});

routes.get('/shaders', async (_req, res) => {
  res.status(200).send({ shaders: shadersController.shaders });
});

routes.get<RoomIdParams>('/room/:roomId', async (req, res) => {
  const { roomId } = req.params;
  const room = state.getRoom(roomId);
  const [inputs, layout] = room.getState();

  res.status(200).send({
    inputs: inputs.map(publicInputState),
    layout,
    whepUrl: room.getWhepUrl(),
    pendingDelete: room.pendingDelete,
  });
});

routes.get('/rooms', async (_req, res) => {
  const adminKey = _req.headers['x-admin-key'];
  if (!adminKey || adminKey !== 'super-secret-hardcode-admin-key') {
    return res.status(401).send({ error: 'Unauthorized' });
  }

  res.header('Refresh', '2');

  const allRooms = state.getRooms();

  const roomsInfo = allRooms
    .map(room => {
      if (!room) {
        return undefined;
      }
      const [inputs, layout] = room.getState();
      return {
        roomId: room.idPrefix,
        inputs: inputs.map(publicInputState),
        layout,
        whepUrl: room.getWhepUrl(),
        pendingDelete: room.pendingDelete,
      };
    })
    .filter(Boolean);

  res
    .status(200)
    .header('Content-Type', 'application/json')
    .send(JSON.stringify({ rooms: roomsInfo }, null, 2));
});

const UpdateRoomSchema = Type.Object({
  inputOrder: Type.Optional(Type.Array(Type.String())),
  layout: Type.Optional(
    Type.Union([
      Type.Literal('grid'),
      Type.Literal('primary-on-left'),
      Type.Literal('primary-on-top'),
      Type.Literal('picture-in-picture'),
      Type.Literal('transition'),
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
    channelId: Type.String(),
  }),
  Type.Object({
    type: Type.Literal('kick-channel'),
    channelId: Type.String(),
  }),
  Type.Object({
    type: Type.Literal('whip'),
    username: Type.String(),
  }),
  Type.Object({
    type: Type.Literal('local-mp4'),
    source: Type.Union([
      Type.Object({ fileName: Type.String() }),
      Type.Object({ url: Type.String() }),
    ]),
  }),
]);

routes.post<RoomIdParams & { Body: Static<typeof AddInputSchema> }>(
  '/room/:roomId/input',
  { schema: { body: AddInputSchema } },
  async (req, res) => {
    const { roomId } = req.params;
    console.log('[request] Create input', { body: req.body, roomId });
    const room = state.getRoom(roomId);
    const inputId = await room.addNewInput(req.body);
    console.log('[info] Added input', { inputId });
    let bearerToken = '';
    if (inputId) {
      bearerToken = await room.connectInput(inputId);
    }
    let whipUrl = `${config.whipBaseUrl}/${inputId}`;
    res.status(200).send({ inputId, bearerToken, whipUrl });
  }
);

routes.post<RoomAndInputIdParams>('/room/:roomId/input/:inputId/whip/ack', async (req, res) => {
  const { roomId, inputId } = req.params;
  console.log('[request] WHIP ack', { roomId, inputId });
  try {
    const input = state
      .getRoom(roomId)
      .getInputs()
      .find(i => i.inputId === inputId);
    if (!input || input.type !== 'whip') {
      return res.status(400).send({ error: 'Not a WHIP input' });
    }
    await state.getRoom(roomId).ackWhipInput(inputId);
    res.status(200).send({ status: 'ok' });
  } catch (err: any) {
    res.status(400).send({ status: 'error', message: err?.message ?? 'Invalid input' });
  }
});

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
  showTitle: Type.Optional(Type.Boolean()),
  shaders: Type.Optional(
    Type.Array(
      Type.Object({
        shaderName: Type.String(),
        shaderId: Type.String(),
        enabled: Type.Boolean(),
        params: Type.Array(
          Type.Object({
            paramName: Type.String(),
            paramValue: Type.Number(),
          })
        ),
      })
    )
  ),
});

routes.post<RoomAndInputIdParams & { Body: Static<typeof UpdateInputSchema> }>(
  '/room/:roomId/input/:inputId',
  { schema: { body: UpdateInputSchema } },
  async (req, res) => {
    const { roomId, inputId } = req.params;
    console.log('[request] Update input', { roomId, inputId, body: JSON.stringify(req.body) });
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
  switch (input.type) {
    case 'local-mp4':
      return {
        inputId: input.inputId,
        title: input.metadata.title,
        description: input.metadata.description,
        showTitle: input.showTitle,
        sourceState: 'always-live',
        status: input.status,
        volume: input.volume,
        shaders: input.shaders,
      };
    case 'twitch-channel':
      return {
        inputId: input.inputId,
        title: input.metadata.title,
        description: input.metadata.description,
        showTitle: input.showTitle,
        sourceState: input.monitor.isLive() ? 'live' : 'offline',
        status: input.status,
        volume: input.volume,
        shaders: input.shaders,
        channelId: input.channelId,
      };
    case 'kick-channel':
      return {
        inputId: input.inputId,
        title: input.metadata.title,
        description: input.metadata.description,
        showTitle: input.showTitle,
        sourceState: input.monitor.isLive() ? 'live' : 'offline',
        status: input.status,
        volume: input.volume,
        shaders: input.shaders,
        channelId: input.channelId,
      };
    case 'whip':
      return {
        inputId: input.inputId,
        title: input.metadata.title,
        description: input.metadata.description,
        showTitle: input.showTitle,
        sourceState: input.monitor.isLive() ? 'live' : 'offline',
        status: input.status,
        volume: input.volume,
        shaders: input.shaders,
      };
    default:
      throw new Error('Unknown input state');
  }
}
