import { createContext } from 'react';
import { AudioContext } from './audioOutputContext.js';
import type { TimeContext } from './timeContext.js';
import { LiveTimeContext } from './timeContext.js';
import { LiveInputStreamStore, type InputStreamStore } from './inputStreamStore.js';
import type { RegisterMp4Input } from '../types/input.js';
import type { RegisterImage } from '../types/resource.js';
import type { Logger } from '../types/logger.js';

export type SmelterOutputContext = {
  // global store for input stream state
  globalInputStreamStore: InputStreamStore<string>;
  // internal input streams store
  internalInputStreamStore: InputStreamStore<number>;
  // Audio mixer configuration
  audioContext: AudioContext;
  // Time tracking and handling for blocking tasks
  timeContext: TimeContext;

  outputId: string;

  logger: Logger;

  // TODO: aggregate that into some context object when we add more methods like this.
  registerMp4Input: (
    inputId: number,
    registerRequest: RegisterMp4Input
  ) => Promise<{ videoDurationMs?: number; audioDurationMs?: number }>;

  unregisterMp4Input: (inputId: number) => Promise<void>;

  registerImage: (imageId: number, registerRequest: RegisterImage) => Promise<void>;

  unregisterImage: (imageId: number) => Promise<void>;
};

const noopLogger = {
  error: () => null,
  warn: () => null,
  info: () => null,
  debug: () => null,
  trace: () => null,
} as const;

export const SmelterContext = createContext<SmelterOutputContext>({
  globalInputStreamStore: new LiveInputStreamStore(noopLogger),
  internalInputStreamStore: new LiveInputStreamStore(noopLogger),
  audioContext: new AudioContext(() => {}),
  timeContext: new LiveTimeContext(),
  outputId: '',
  registerMp4Input: async () => ({}),
  unregisterMp4Input: async () => {},
  registerImage: async () => {},
  unregisterImage: async () => {},
  logger: noopLogger,
});
