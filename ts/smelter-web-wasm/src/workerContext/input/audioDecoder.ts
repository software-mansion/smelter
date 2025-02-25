import { Queue } from '@datastructures-js/queue';
import type { InputAudioData } from './frame';
import type { EncodedSource, AudioDataPayload, InputAudioSamplesSource } from './input';
import type { Logger } from 'pino';
import { assert, sleep } from '../../utils';
import type { WorkloadBalancer, WorkloadBalancerNode } from '../queue';

const MAX_DECODED_CHUNKS = 40;

export class InputAudioDecoder implements InputAudioSamplesSource {
  private source: EncodedSource;
  private decoder: AudioDecoder;
  private offsetMs?: number;
  private samples: Queue<InputAudioData>;
  private receivedEos: boolean = false;
  private initialBufferReadyPromise: Promise<void>;
  private workloadBalancerNode: WorkloadBalancerNode;

  public constructor(source: EncodedSource, logger: Logger, workloadBalancer: WorkloadBalancer) {
    this.source = source;
    this.samples = new Queue();
    this.workloadBalancerNode = workloadBalancer.highPriorityNode();

    let onInitialBufferReady: (() => void) | undefined;
    let onDecoderError: ((err: Error) => void) | undefined;
    this.initialBufferReadyPromise = new Promise<void>((res, rej) => {
      onInitialBufferReady = res;
      onDecoderError = rej;
    });

    this.decoder = new AudioDecoder({
      output: sampleBatch => {
        this.onBatchDecoded(sampleBatch);
        if (this.samples.size() > MAX_DECODED_CHUNKS / 2) {
          onInitialBufferReady?.();
        }
      },
      error: error => {
        onDecoderError?.(error);
        logger.error(`AudioDecoder error: ${error}`);
      },
    });
  }

  public async init(): Promise<void> {
    const metadata = this.source.getMetadata();
    assert(metadata.audio);
    this.decoder.configure(metadata.audio.decoderConfig);
    while (!this.trySchedulingDecoding()) {
      await sleep(100);
    }
    await this.initialBufferReadyPromise;
  }

  public nextBatch(): AudioDataPayload | undefined {
    this.workloadBalancerNode.setState(
      this.receivedEos ? 1 : this.samples.size() / MAX_DECODED_CHUNKS
    );
    const sampleBatch = this.samples.pop();
    this.trySchedulingDecoding();
    if (sampleBatch) {
      return { type: 'sampleBatch', sampleBatch };
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

  private onBatchDecoded(data: AudioData) {
    const frameTimeMs = data.timestamp / 1000;
    if (this.offsetMs === undefined) {
      this.offsetMs = -frameTimeMs;
    }

    this.samples.push({
      data,
      ptsMs: this.offsetMs + frameTimeMs,
    });
  }

  private trySchedulingDecoding(): boolean {
    if (this.receivedEos) {
      return true;
    }
    while (this.samples.size() + this.decoder.decodeQueueSize < MAX_DECODED_CHUNKS) {
      const payload = this.source.nextAudioChunk();
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
