import type { Output as CoreOutput } from '@swmansion/smelter-core';
import type { WorkerMessage } from '../workerApi';
import { handleRegisterCanvasOutput } from './output/canvas';
import { handleRegisterWhipOutput } from './output/whip';
import type { Api } from '@swmansion/smelter';
import { handleRegisterStreamOutput } from './output/stream';
import type { Logger } from 'pino';
import type { Framerate } from '../compositor/compositor';
import type { AudioMixer } from './AudioMixer';

export interface Output {
  get audioMixer(): AudioMixer | undefined;
  terminate(): Promise<void>;
}

type InitialScene = {
  initial: { video?: Api.Video; audio?: Api.Audio };
};

export type RegisterOutputResponse =
  | {
      type: 'web-wasm-stream';
      stream: MediaStream;
    }
  | {
      type: 'web-wasm-whip';
      stream: MediaStream;
    };

export type RegisterOutputResult = {
  output: Output;
  result?: RegisterOutputResponse;
  workerMessage: [WorkerMessage, Transferable[]];
};

export type RegisterWasmWhipOutput = CoreOutput.RegisterWasmWhipOutput & InitialScene;
export type RegisterWasmStreamOutput = CoreOutput.RegisterWasmStreamOutput & InitialScene;
export type RegisterWasmCanvasOutput = CoreOutput.RegisterWasmCanvasOutput & InitialScene;

export async function handleRegisterOutputRequest(
  outputId: string,
  body: CoreOutput.RegisterOutputRequest,
  logger: Logger,
  framerate: Framerate
): Promise<RegisterOutputResult> {
  if (body.type === 'web-wasm-whip') {
    return await handleRegisterWhipOutput(outputId, body, logger, framerate);
  } else if (body.type === 'web-wasm-stream') {
    return await handleRegisterStreamOutput(outputId, body);
  } else if (body.type === 'web-wasm-canvas') {
    return await handleRegisterCanvasOutput(outputId, body);
  } else {
    throw new Error(`Unknown output type ${body.type}`);
  }
}
