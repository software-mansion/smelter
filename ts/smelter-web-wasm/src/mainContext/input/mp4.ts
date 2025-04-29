import MP4Box from 'mp4box';

import type { Input, RegisterInputResult } from '../input';
import type { InstanceContext } from '../instance';
import type { Input as CoreInput } from '@swmansion/smelter-core';

export class Mp4Input implements Input {
  private inputId: string;
  private ctx: InstanceContext;

  constructor(inputId: string, ctx: InstanceContext) {
    this.inputId = inputId;
    this.ctx = ctx;
  }

  public async terminate(): Promise<void> {
    await this.ctx.audioMixer.removeInput(this.inputId);
  }
}

export async function handleRegisterMp4Input(
  ctx: InstanceContext,
  inputId: string,
  request: CoreInput.RegisterMp4InputRequest
): Promise<RegisterInputResult> {
  let arrayBuffer: ArrayBuffer;
  if (request.blob) {
    arrayBuffer = await request.blob.arrayBuffer();
  } else if (request.url) {
    const response = await fetch(request.url);
    arrayBuffer = await response.arrayBuffer();
  } else {
    throw new Error('mp4 URL or Blob is required');
  }

  const metadata = await parseMp4(arrayBuffer);

  let messagePort;
  if (metadata?.sampleRate === ctx.audioMixer.mainSampleRate) {
    messagePort = ctx.audioMixer.addWorkletInput(inputId);
  } else if (metadata?.sampleRate) {
    messagePort = await ctx.audioMixer.addWorkletInputWithResample(inputId, metadata.sampleRate);
  }

  return {
    input: new Mp4Input(inputId, ctx),
    workerMessage: [
      {
        type: 'registerInput',
        inputId,
        input: {
          type: 'mp4',
          arrayBuffer,
          audioWorkletMessagePort: messagePort,
        },
      },
      [...(messagePort ? [messagePort] : []), arrayBuffer],
    ],
  };
}

async function parseMp4(buffer: ArrayBuffer): Promise<{ sampleRate?: number }> {
  (buffer as any).fileStart = 0;

  const file = MP4Box.createFile();

  const result = new Promise<{ sampleRate?: number }>((res, rej) => {
    file.onReady = info => {
      try {
        res(parseMp4Info(info));
      } catch (err: any) {
        rej(err);
      }
    };
    file.onError = (error: string) => {
      rej(new Error(error));
    };
  });

  file.appendBuffer(buffer as any);
  return result;
}

function parseMp4Info(info: MP4Box.MP4Info): { sampleRate?: number } {
  const audioTrack = info.audioTracks[0];
  if (!audioTrack) {
    return {};
  }
  return { sampleRate: audioTrack.audio.sample_rate };
}
