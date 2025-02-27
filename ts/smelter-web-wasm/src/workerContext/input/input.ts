import Mp4Source from './source/Mp4Source';
import { QueuedInput } from './QueuedInput';
import type { InputVideoFrame } from './frame';
import type { Frame, FrameFormat } from '@swmansion/smelter-browser-render';
import { MediaStreamInput } from './MediaStreamInput';
import type { RegisterInput } from '../../workerApi';
import type { Logger } from 'pino';
import { assert } from '../../utils';

export type InputStartResult = {
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export type ContainerInfo = {
  video: {
    durationMs?: number;
    decoderConfig: VideoDecoderConfig;
  };
};

export interface Input {
  start(): InputStartResult;
  updateQueueStartTime(queueStartTimeMs: number): void;
  getFrame(currentQueuePts: number, frameFormat: FrameFormat): Promise<Frame | undefined>;
  close(): void;
}

export type VideoFramePayload = { type: 'frame'; frame: InputVideoFrame } | { type: 'eos' };

export interface InputVideoFrameSource {
  init(): Promise<void>;
  getMetadata(): InputStartResult;
  nextFrame(): VideoFramePayload | undefined;
  close(): void;
}

export type EncodedVideoPayload = { type: 'chunk'; chunk: EncodedVideoChunk } | { type: 'eos' };

/**
 * `EncodedVideoSource` produces encoded video chunks required for decoding.
 */
export interface EncodedVideoSource {
  init(): Promise<void>;
  getMetadata(): ContainerInfo;
  nextChunk(): EncodedVideoPayload | undefined;
  close(): void;
}

export async function createInput(
  inputId: string,
  request: RegisterInput,
  logger: Logger
): Promise<Input> {
  const inputLogger = logger.child({ inputId });
  if (request.type === 'mp4') {
    if (!request.url) {
      throw new Error('Mp4 url is required');
    }
    const source = new Mp4Source(request.url, inputLogger);
    await source.init();
    return new QueuedInput(inputId, source, inputLogger);
  } else if (request.type === 'stream') {
    assert(request.videoStream);
    return new MediaStreamInput(inputId, request.videoStream, inputLogger);
  } else if (request.type === 'track') {
    assert(request.videoTrack);
    // @ts-ignore
    const videoTrackProcessor = new MediaStreamTrackProcessor({ track: request.videoTrack });
    return new MediaStreamInput(inputId, videoTrackProcessor.readable, inputLogger);
  }
  throw new Error(`Unknown input type ${(request as any).type}`);
}
