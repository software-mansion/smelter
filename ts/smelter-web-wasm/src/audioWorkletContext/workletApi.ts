import type { AsyncMessagePort } from './bridge';

export type AudioWorkletMessage =
  | {
      type: 'chunk';
      chunks: Float32Array[][];
    }
  | { type: 'eos' };

export type AudioWorkletMessagePort = AsyncMessagePort<AudioWorkletMessage, boolean>;
