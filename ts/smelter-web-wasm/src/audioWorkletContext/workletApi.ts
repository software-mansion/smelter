import type { AsyncMessagePort } from './bridge';

export type AudioWorkletMessage = {
  type: 'chunk';
  data: AudioData;
};

export type AudioWorkletMessagePort = AsyncMessagePort<AudioWorkletMessage, boolean>;
