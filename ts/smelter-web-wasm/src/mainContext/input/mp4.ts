import type { Input, RegisterInputResult } from '../input';
import type { InstanceContext } from '../instance';

export class Mp4Input implements Input {
  private inputId: string;
  private ctx: InstanceContext;

  constructor(inputId: string, ctx: InstanceContext) {
    this.inputId = inputId;
    this.ctx = ctx;
  }

  public async terminate(): Promise<void> {
    this.ctx.audioMixer.removeInput(this.inputId);
  }
}

export async function handleRegisterMp4Input(
  ctx: InstanceContext,
  inputId: string,
  url: string
): Promise<RegisterInputResult> {
  const messagePort = ctx.audioMixer.addWorkletInput(inputId);
  return {
    input: new Mp4Input(inputId, ctx),
    workerMessage: [
      {
        type: 'registerInput',
        inputId,
        input: {
          type: 'mp4',
          url,
          audioWorkletMessagePort: messagePort,
        },
      },
      [messagePort],
    ],
  };
}
