import type { Output as CoreOutput } from '@swmansion/smelter-core';
import type { WorkerMessage } from '../workerApi';
import { handleRegisterCanvasOutput } from './output/canvas';
import { handleRegisterWhipClientOutput } from './output/whip';
import type { Api } from '@swmansion/smelter';
import { handleRegisterStreamOutput } from './output/stream';
import type { InstanceContext } from './instance';

export interface Output {
  terminate(): Promise<void>;
}

type InitialScene = {
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene };
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
  ctx: InstanceContext,
  outputId: string,
  body: CoreOutput.RegisterOutputRequest
): Promise<RegisterOutputResult> {
  if (body.type === 'web-wasm-whip') {
    return await handleRegisterWhipClientOutput(ctx, outputId, body);
  } else if (body.type === 'web-wasm-stream') {
    return await handleRegisterStreamOutput(ctx, outputId, body);
  } else if (body.type === 'web-wasm-canvas') {
    return await handleRegisterCanvasOutput(ctx, outputId, body);
  } else {
    throw new Error(`Unknown output type ${body.type}`);
  }
}
