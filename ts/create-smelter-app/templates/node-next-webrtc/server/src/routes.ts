import Fastify from 'fastify';
import cors from '@fastify/cors';
import { Type } from '@sinclair/typebox';
import type { Static, TypeBoxTypeProvider } from '@fastify/type-provider-typebox';

import { store } from './app/store';
import { SmelterInstance } from './smelter';

export const routes = Fastify().withTypeProvider<TypeBoxTypeProvider>();

routes.register(cors);

export const UpdateLayoutSchema = Type.Object({
  showInstructions: Type.Boolean(),
});

export type UpdateLayoutType = Static<typeof UpdateLayoutSchema>;

routes.post<{ Body: UpdateLayoutType }>(
  '/layout-update',
  { schema: { body: UpdateLayoutSchema } },
  async (req, res) => {
    store.getState().updateLayout(req.body);
    res.status(200).send({ status: 'ok' });
  }
);

export const StartRtmpStreamSchema = Type.Object({
  url: Type.String(),
});
export type StartRtmpStreamType = Static<typeof StartRtmpStreamSchema>;

routes.post<{ Body: StartRtmpStreamType }>(
  '/start-rtmp-stream',
  { schema: { body: StartRtmpStreamSchema } },
  async (req, res) => {
    await SmelterInstance.registerRtmpOutput(req.body.url);
    res.status(200).send({ status: 'ok' });
  }
);
