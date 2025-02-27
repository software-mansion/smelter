import type { Output, RegisterOutputResult, RegisterWasmCanvasOutput } from '../output';
import type { RegisterOutput } from '../../workerApi';
import type { InstanceContext } from '../instance';

type VideoTrackResult = {
  workerMessage: RegisterOutput['video'];
  transferable: Transferable[];
};

type AudioTrackResult = {
  transferable: Transferable[];
};

export class CanvasOutput implements Output {
  private outputId: string;
  private ctx: InstanceContext;

  constructor(outputId: string, ctx: InstanceContext) {
    this.outputId = outputId;
    this.ctx = ctx;
  }

  public async terminate(): Promise<void> {
    await this.ctx.audioMixer.removeOutput(this.outputId);
  }
}

export async function handleRegisterCanvasOutput(
  ctx: InstanceContext,
  outputId: string,
  request: RegisterWasmCanvasOutput
): Promise<RegisterOutputResult> {
  const videoResult = await handleVideo(ctx, outputId, request);
  const audioResult = await handleAudio(ctx, outputId, request);

  const output = new CanvasOutput(outputId, ctx);
  return {
    output,
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
  _ctx: InstanceContext,
  _outputId: string,
  request: RegisterWasmCanvasOutput
): Promise<VideoTrackResult | undefined> {
  if (!request.video || !request.initial.video) {
    return undefined;
  }
  const canvas = request.video.canvas;
  canvas.width = request.video.resolution.width;
  canvas.height = request.video.resolution.height;
  const offscreen = canvas.transferControlToOffscreen();

  return {
    workerMessage: {
      resolution: request.video.resolution,
      initial: request.initial.video,
      canvas: offscreen,
    },
    transferable: [offscreen],
  };
}

async function handleAudio(
  ctx: InstanceContext,
  outputId: string,
  request: RegisterWasmCanvasOutput
): Promise<AudioTrackResult | undefined> {
  if (!request.audio || !request.initial.audio) {
    return undefined;
  }

  ctx.audioMixer.addPlaybackOutput(outputId);

  return {
    transferable: [],
  };
}
