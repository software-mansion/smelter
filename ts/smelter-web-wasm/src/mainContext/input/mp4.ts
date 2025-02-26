import type { Input, RegisterInputResult } from '../input';

export class Mp4Input implements Input {
  private mediaStream: MediaStream;

  constructor(mediaStream: MediaStream) {
    // @ts-ignore
    this.mediaStream = new MediaStreamTrackGenerator({ kind: 'audio' });
  }

  public get audioTrack(): MediaStreamTrack | undefined {
    return this.mediaStream.getAudioTracks()[0];
  }

  public async terminate(): Promise<void> {
    this.mediaStream.getTracks().forEach(track => track.stop());
  }
}

export async function handleRegisterMp4Input(
  inputId: string,
  url: string
): Promise<RegisterInputResult> {
  return {
    input: new Mp4Input(stream),
    workerMessage: [
      {
        type: 'registerInput',
        inputId,
        input: {
          type: 'mp4',
          videoStream: videoTrackProcessor.readable,
        },
      },
      transferable,
    ],
  };
}
