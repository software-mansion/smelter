import type { RegisterOutput } from '../../workerApi';
import type { InstanceContext } from '../instance';
import type { Output, RegisterOutputResult, RegisterWasmStreamOutput } from '../output';

type StreamOptions = {
  ctx: InstanceContext;
  canvasStream?: MediaStream;
  outputStream: MediaStream;
};

type VideoTrackResult = {
  workerMessage: RegisterOutput['video'];
  canvasStream: MediaStream;
  track: MediaStreamTrack;
  transferable: Transferable[];
};

type AudioTrackResult = {
  track: MediaStreamTrack;
  transferable: Transferable[];
};

export class StreamOutput implements Output {
  private outputId: string;
  private ctx: InstanceContext;
  private outputStream: MediaStream;
  private canvasStream?: MediaStream;

  constructor(outputId: string, options: StreamOptions) {
    this.outputId = outputId;
    this.canvasStream = options.canvasStream;
    this.outputStream = options.outputStream;
    this.ctx = options.ctx;
  }

  public async terminate(): Promise<void> {
    this.outputStream.getTracks().forEach(track => track.stop());
    this.canvasStream?.getTracks().forEach(track => track.stop());
    this.ctx.audioMixer.removeOutput(this.outputId);
  }
}

export async function handleRegisterStreamOutput(
  ctx: InstanceContext,
  outputId: string,
  request: RegisterWasmStreamOutput
): Promise<RegisterOutputResult> {
  let outputStream = new MediaStream();

  const videoResult = await handleVideo(ctx, outputId, request);
  const audioResult = await handleAudio(ctx, outputId, request);

  const output = new StreamOutput(outputId, {
    canvasStream: videoResult?.canvasStream,
    outputStream,
    ctx,
  });

  if (videoResult) {
    outputStream.addTrack(videoResult.track);
  }
  if (audioResult) {
    outputStream.addTrack(audioResult.track);
  }

  return {
    output,
    result: {
      type: 'web-wasm-stream',
      stream: outputStream,
    },
    workerMessage: [
      {
        type: 'registerOutput',
        outputId,
        output: {
          type: 'stream',
          video: videoResult?.workerMessage,
        },
      },
      [...(videoResult?.transferable ?? []), ...(audioResult?.transferable ?? [])],
    ],
  };
}

async function handleVideo(
  ctx: InstanceContext,
  _outputId: string,
  request: RegisterWasmStreamOutput
): Promise<VideoTrackResult | undefined> {
  if (!request.video || !request.initial.video) {
    return undefined;
  }
  const canvas = document.createElement('canvas');
  canvas.width = request.video.resolution.width;
  canvas.height = request.video.resolution.height;
  const canvasStream = canvas.captureStream(ctx.framerate.num / ctx.framerate.den);
  const track = canvasStream.getVideoTracks()[0];
  const offscreen = canvas.transferControlToOffscreen();

  return {
    workerMessage: {
      resolution: request.video.resolution,
      initial: request.initial.video,
      canvas: offscreen,
    },
    canvasStream,
    track,
    transferable: [offscreen],
  };
}

async function handleAudio(
  ctx: InstanceContext,
  outputId: string,
  request: RegisterWasmStreamOutput
): Promise<AudioTrackResult | undefined> {
  if (!request.audio || !request.initial.audio) {
    return undefined;
  }
  const track = ctx.audioMixer.addMediaStreamOutput(outputId);
  return {
    track,
    transferable: [],
  };
}
