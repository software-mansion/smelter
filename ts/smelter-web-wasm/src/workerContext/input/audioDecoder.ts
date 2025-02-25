import { Queue } from '@datastructures-js/queue';
import type { InputVideoFrame } from './frame';
import type {
  InputVideoFrameSource,
  EncodedSource,
  VideoFramePayload,
  InputStartResult,
} from './input';
import type { Logger } from 'pino';
import { sleep } from '../../utils';

const MAX_DECODED_FRAMES = 10;

export class Decoder implements InputVideoFrameSource {
  private source: EncodedSource;
  private decoder: VideoDecoder;
  private offsetMs?: number;
  private frames: Queue<InputVideoFrame>;
  private receivedEos: boolean = false;
  private firstFramePromise: Promise<void>;

  public constructor(source: EncodedSource, logger: Logger) {
    this.source = source;
    this.frames = new Queue();

    let onFirstFrame: (() => void) | undefined;
    let onDecoderError: ((err: Error) => void) | undefined;
    this.firstFramePromise = new Promise<void>((res, rej) => {
      onFirstFrame = res;
      onDecoderError = rej;
    });

    this.decoder = new VideoDecoder({
      output: videoFrame => {
        onFirstFrame?.();
        this.onFrameDecoded(videoFrame);
      },
      error: error => {
        onDecoderError?.(error);
        logger.error(`H264Decoder error: ${error}`);
      },
    });
  }

  public async init(): Promise<void> {
    const metadata = this.source.getMetadata();
    this.decoder.configure(metadata.video.decoderConfig);
    while (!this.trySchedulingDecoding()) {
      await sleep(100);
    }
    await this.firstFramePromise;
  }

  public nextFrame(): VideoFramePayload | undefined {
    const frame = this.frames.pop();
    this.trySchedulingDecoding();
    if (frame) {
      return { type: 'frame', frame: frame };
    } else if (this.receivedEos && this.decoder.decodeQueueSize === 0) {
      return { type: 'eos' };
    }
    return;
  }

  public getMetadata(): InputStartResult {
    throw new Error('Decoder does not provide metadata');
  }

  public close() {
    this.decoder.close();
    this.source.close();
  }

  private onFrameDecoded(videoFrame: VideoFrame) {
    const frameTimeMs = videoFrame.timestamp / 1000;
    if (this.offsetMs === undefined) {
      this.offsetMs = -frameTimeMs;
    }

    this.frames.push({
      frame: videoFrame,
      ptsMs: this.offsetMs + frameTimeMs,
    });
  }

  private trySchedulingDecoding(): boolean {
    if (this.receivedEos) {
      return true;
    }
    while (this.frames.size() + this.decoder.decodeQueueSize < MAX_DECODED_FRAMES) {
      const payload = this.source.nextChunk();
      if (!payload) {
        return false;
      } else if (payload.type === 'eos') {
        this.receivedEos = true;
        return true;
      } else if (payload.type === 'chunk') {
        this.decoder.decode(payload.chunk);
      }
    }
    return true;
  }
}
