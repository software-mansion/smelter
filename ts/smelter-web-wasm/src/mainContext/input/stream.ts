import type { Input, RegisterInputResult } from '../input';
import { IS_FIREFOX, type InstanceContext } from '../instance';

export class StreamInput implements Input {
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

export async function handleRegisterStreamInput(
  ctx: InstanceContext,
  inputId: string,
  stream: MediaStream
): Promise<RegisterInputResult> {
  const videoTrack = stream.getVideoTracks()[0];
  const audioTrack = stream.getAudioTracks()[0];

  if (!ctx.enableWebWorker) {
    const videoElement = document.createElement('video');
    videoElement.srcObject = stream;
    await videoElement.play();
    if (audioTrack) {
      ctx.audioMixer.addMediaStreamInput(inputId, audioTrack);
    }
    return {
      input: new StreamInput(inputId, ctx),
      workerMessage: [
        {
          type: 'registerInput',
          inputId,
          input: {
            type: 'domVideoElement',
            videoElement,
          },
        },
        [],
      ],
    };
  } else {
    const transferable = [];
    // @ts-ignore
    let videoTrackProcessor: MediaStreamTrackProcessor | undefined;
    if (videoTrack) {
      // @ts-ignore
      videoTrackProcessor = new MediaStreamTrackProcessor({ track: videoTrack });
      transferable.push(videoTrackProcessor.readable);
    }

    if (audioTrack) {
      ctx.audioMixer.addMediaStreamInput(inputId, audioTrack);
    }

    return {
      input: new StreamInput(inputId, ctx),
      workerMessage: [
        {
          type: 'registerInput',
          inputId,
          input: {
            type: 'stream',
            videoStream: videoTrackProcessor.readable,
          },
        },
        transferable,
      ],
    };
  }
}
