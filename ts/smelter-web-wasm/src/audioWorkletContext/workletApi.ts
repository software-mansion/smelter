import type { AsyncMessagePort } from './bridge';

export type AudioWorkletMessage = {
  type: 'chunk';
  data: Float32Array[];
};

export type AudioWorkletMessagePort = AsyncMessagePort<AudioWorkletMessage, boolean>;
