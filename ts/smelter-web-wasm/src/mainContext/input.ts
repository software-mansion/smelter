import type { Input as CoreInput } from '@swmansion/smelter-core';
import type { WorkerMessage } from '../workerApi';
import { assert } from '../utils';
import { handleRegisterCameraInput } from './input/camera';
import { handleRegisterScreenCaptureInput } from './input/screenCapture';
import { handleRegisterStreamInput } from './input/stream';
import { handleRegisterMp4Input } from './input/mp4';

export interface Input {
  get audioTrack(): MediaStreamTrack | undefined;
  terminate(): Promise<void>;
}

/**
 * Can be used if entire code for the input runs in worker.
 */
class NoopInput implements Input {
  public async terminate(): Promise<void> {}

  public get audioTrack(): MediaStreamTrack | undefined {
    return undefined;
  }
}

export type RegisterInputResult = {
  input: Input;
  workerMessage: [WorkerMessage, Transferable[]];
};

export async function handleRegisterInputRequest(
  inputId: string,
  body: CoreInput.RegisterInputRequest
): Promise<RegisterInputResult> {
  if (body.type === 'mp4') {
    assert(body.url, 'mp4 URL is required');
    return handleRegisterMp4Input(inputId, body.url);
  } else if (body.type === 'camera') {
    return await handleRegisterCameraInput(inputId);
  } else if (body.type === 'screen_capture') {
    return await handleRegisterScreenCaptureInput(inputId);
  } else if (body.type === 'stream') {
    return await handleRegisterStreamInput(inputId, body.stream);
  } else {
    throw new Error(`Unknown input type ${body.type}`);
  }
}
