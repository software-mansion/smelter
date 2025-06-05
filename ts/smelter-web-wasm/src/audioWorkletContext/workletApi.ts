export type AudioWorkletMessage =
  | {
      type: 'chunk';
      chunks: Float32Array[][];
    }
  | { type: 'eos' };
