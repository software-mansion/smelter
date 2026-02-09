import Fastify from 'fastify';
import cors from '@fastify/cors';
import { Type } from '@sinclair/typebox';
import type { Static, TypeBoxTypeProvider } from '@fastify/type-provider-typebox';

import { store } from './app/store';

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
