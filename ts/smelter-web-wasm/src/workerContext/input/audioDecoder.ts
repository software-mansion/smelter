import { Queue } from '@datastructures-js/queue';
import type { InputAudioData } from './frame';
import type {
  EncodedSource,
  InputStartResult,
  AudioDataPayload,
  InputAudioSamplesSource,
} from './input';
import type { Logger } from 'pino';
import { assert, sleep } from '../../utils';

const MAX_DECODED_CHUNKS = 10;

export class InputAudioDecoder implements InputAudioSamplesSource {
  private source: EncodedSource;
  private decoder: VideoDecoder;
  private offsetMs?: number;
  private samples: Queue<InputAudioData>;
  private receivedEos: boolean = false;
  private firstChunkPromise: Promise<void>;

  public constructor(source: EncodedSource, logger: Logger) {
    this.source = source;
    this.samples = new Queue();

    let onFirstDecodedBatch: (() => void) | undefined;
    let onDecoderError: ((err: Error) => void) | undefined;
    this.firstChunkPromise = new Promise<void>((res, rej) => {
      onFirstDecodedBatch = res;
      onDecoderError = rej;
    });

    this.decoder = new AudioDecoder({
      output: sampleBatch => {
        onFirstDecodedBatch?.();
        this.onBatchDecoded(sampleBatch);
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
    await this.firstChunkPromise;
  }

  public nextBatch(): AudioDataPayload | undefined {
    const sampleBatch = this.samples.pop();
    this.trySchedulingDecoding();
    if (sampleBatch) {
      return { type: 'sampleBatch', sampleBatch };
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
