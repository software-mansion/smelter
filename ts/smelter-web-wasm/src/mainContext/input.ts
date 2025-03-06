import type { Input as CoreInput } from '@swmansion/smelter-core';
import type { WorkerMessage } from '../workerApi';
import { assert, downloadToArrayBuffer } from '../utils';
import { handleRegisterCameraInput } from './input/camera';
import { handleRegisterScreenCaptureInput } from './input/screenCapture';
import { handleRegisterStreamInput } from './input/stream';
import { handleRegisterMp4Input } from './input/mp4';
import type { InstanceContext } from './instance';

export interface Input {
  terminate(): Promise<void>;
}

export type RegisterInputResult = {
  input: Input;
  workerMessage: [WorkerMessage, Transferable[]];
};

export async function handleRegisterInputRequest(
  ctx: InstanceContext,
  inputId: string,
  body: CoreInput.RegisterInputRequest
): Promise<RegisterInputResult> {
  if (body.type === 'mp4') {
    assert(body.url, 'mp4 URL is required');
    const arrayBuffer = await downloadToArrayBuffer(body.url);
    return await handleRegisterMp4Input(ctx, inputId, arrayBuffer);
  } else if (body.type === 'mp4_blob') {
    const arrayBuffer = await (body.blob as Blob).arrayBuffer();
    return await handleRegisterMp4Input(ctx, inputId, arrayBuffer);
  } else if (body.type === 'camera') {
    return await handleRegisterCameraInput(ctx, inputId);
  } else if (body.type === 'screen_capture') {
    return await handleRegisterScreenCaptureInput(ctx, inputId);
  } else if (body.type === 'stream') {
    return await handleRegisterStreamInput(ctx, inputId, body.stream);
  } else {
    throw new Error(`Unknown input type ${body.type}`);
  }
}
