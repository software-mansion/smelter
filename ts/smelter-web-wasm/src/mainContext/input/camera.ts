import type { Input, RegisterInputResult } from '../input';

export class CameraInput implements Input {
  private mediaStream: MediaStream;

  constructor(mediaStream: MediaStream) {
    this.mediaStream = mediaStream;
  }

  public get audioTrack(): MediaStreamTrack | undefined {
    return this.mediaStream.getAudioTracks()[0];
  }

  public async terminate(): Promise<void> {
    this.mediaStream.getTracks().forEach(track => track.stop());
  }
}

export async function handleRegisterCameraInput(inputId: string): Promise<RegisterInputResult> {
  const mediaStream = await navigator.mediaDevices.getUserMedia({
    audio: {
      noiseSuppression: { ideal: true },
    },
    video: true,
  });

  const isSafari = !!(window as any).safari;
  if (isSafari) {
    // On Safari, MediaStreamTrackProcessor can be only created on web worker
    return await registerOnSafari(inputId, mediaStream);
  } else {
    return await registerOnChrome(inputId, mediaStream);
  }
}

async function registerOnChrome(
  inputId: string,
  mediaStream: MediaStream
): Promise<RegisterInputResult> {
  const videoTrack = mediaStream.getVideoTracks()[0];
  const transferable = [];

  // @ts-ignore
  let videoTrackProcessor: MediaStreamTrackProcessor | undefined;
  if (videoTrack) {
    // @ts-ignore
    videoTrackProcessor = new MediaStreamTrackProcessor({ track: videoTrack });
    transferable.push(videoTrackProcessor.readable);
  }

  return {
    input: new CameraInput(mediaStream),
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

async function registerOnSafari(
  inputId: string,
  mediaStream: MediaStream
): Promise<RegisterInputResult> {
  const videoTrack = mediaStream.getVideoTracks()[0];
  return {
    input: new CameraInput(mediaStream),
    workerMessage: [
      {
        type: 'registerInput',
        inputId,
        input: {
          type: 'track',
          videoTrack: videoTrack,
        },
      },
      [videoTrack],
    ],
  };
}
