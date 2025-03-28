import Mp4Source from './source/Mp4Source';
import { QueuedInput } from './QueuedInput';
import type { InputAudioData, InputVideoFrame, InputVideoFrameRef } from './frame';
import { MediaStreamInput } from './MediaStreamInput';
import type { RegisterInput } from '../../workerApi';
import type { Logger } from 'pino';
import { assert } from '../../utils';
import type { AsyncMessagePort } from '../../audioWorkletContext/bridge';
import type { AudioWorkletMessage } from '../../audioWorkletContext/workletApi';
import type { WorkloadBalancer } from '../queue';

export type InputStartResult = {
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export type ContainerInfo = {
  video?: {
    durationMs?: number;
    decoderConfig: VideoDecoderConfig;
  };
  audio?: {
    durationMs?: number;
    decoderConfig: AudioDecoderConfig;
  };
};

export interface Input {
  start(): InputStartResult;
  updateQueueStartTime(queueStartTimeMs: number): void;
  getFrame(currentQueuePts: number): Promise<InputVideoFrameRef | undefined>;
  close(): void;
}

export type VideoFramePayload = { type: 'frame'; frame: InputVideoFrame } | { type: 'eos' };

export type AudioDataPayload =
  | { type: 'sampleBatch'; sampleBatch: InputAudioData }
  | { type: 'eos' };

export interface InputVideoFrameSource {
  init(): Promise<void>;
  nextFrame(): VideoFramePayload | undefined;
  close(): void;
}

export interface InputAudioSamplesSource {
  init(): Promise<void>;
  nextBatch(): AudioDataPayload | undefined;
  close(): void;
}

export interface QueuedInputSource extends InputVideoFrameSource, InputAudioSamplesSource {
  getMetadata(): InputStartResult;
  audioWorkletMessagePort(): AsyncMessagePort<AudioWorkletMessage, boolean> | undefined;
}

export type EncodedVideoPayload = { type: 'chunk'; chunk: EncodedVideoChunk } | { type: 'eos' };

export type EncodedAudioPayload = { type: 'chunk'; chunk: EncodedAudioChunk } | { type: 'eos' };

/**
 * `EncodedVideoSource` produces encoded video chunks required for decoding.
 */
export interface EncodedSource {
  init(): Promise<void>;
  getMetadata(): ContainerInfo;
  nextVideoChunk(): EncodedVideoPayload | undefined;
  nextAudioChunk(): EncodedAudioPayload | undefined;
  close(): void;
}

export async function createInput(
  inputId: string,
  request: RegisterInput,
  logger: Logger,
  workloadBalancer: WorkloadBalancer
): Promise<Input> {
  const inputLogger = logger.child({ inputId });
  if (request.type === 'mp4') {
    const source = new Mp4Source(
      request.arrayBuffer,
      inputLogger,
      workloadBalancer,
      request.audioWorkletMessagePort
    );
    await source.init();
    return new QueuedInput(inputId, source, inputLogger);
  } else if (request.type === 'stream') {
    assert(request.videoStream);
    return new MediaStreamInput(inputId, request.videoStream);
  }
  throw new Error(`Unknown input type ${(request as any).type}`);
}
