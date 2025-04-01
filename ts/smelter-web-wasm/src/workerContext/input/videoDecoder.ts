import { Queue } from '@datastructures-js/queue';
import type { InternalVideoFrame } from './frame';
import type { InputVideoFrameSource, EncodedSource, VideoFramePayload } from './input';
import type { Logger } from 'pino';
import { assert, sleep } from '../../utils';
import type { WorkloadBalancer, WorkloadBalancerNode } from '../queue';

const MAX_DECODED_FRAMES = 20;

export class InputVideoDecoder implements InputVideoFrameSource {
  private source: EncodedSource;
  private decoder: VideoDecoder;
  private offsetMs?: number;
  private frames: Queue<InternalVideoFrame>;
  private receivedEos: boolean = false;
  private firstFramePromise: Promise<void>;
  private workloadBalancerNode: WorkloadBalancerNode;

  public constructor(source: EncodedSource, logger: Logger, workloadBalancer: WorkloadBalancer) {
    this.source = source;
    this.frames = new Queue();
    this.workloadBalancerNode = workloadBalancer.highPriorityNode();

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
    assert(metadata.video);
    this.decoder.configure(metadata.video.decoderConfig);
    while (!this.trySchedulingDecoding()) {
      await sleep(100);
    }
    await this.firstFramePromise;
  }

  public nextFrame(): VideoFramePayload | undefined {
    this.workloadBalancerNode.setState(
      this.receivedEos ? 1 : this.frames.size() / MAX_DECODED_FRAMES
    );
    const frame = this.frames.pop();
    this.trySchedulingDecoding();
    if (frame) {
      return { type: 'frame', frame: frame };
    } else if (this.receivedEos && this.decoder.decodeQueueSize === 0) {
      this.workloadBalancerNode.close();
      return { type: 'eos' };
    }
    return;
  }

  public close() {
    this.decoder.close();
    this.source.close();
    this.workloadBalancerNode.close();
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
      const payload = this.source.nextVideoChunk();
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
