import { listenForMessages } from './bridge';
import type { AudioWorkletMessage } from './workletApi';

type Output = Float32Array[];
type Input = Float32Array[];

/**
 * This class will send data received via this.port to all outputs
 *
 * This implementation assumes that
 * - output will always be list of Float32Array for each channel
 * - each Float32Array (for each channel and output) will be the same length
 *   - it is expected to be 128 samples but implementation does not assume that
 * - messages have the same sample rate as AudioContext
 */
// @ts-ignore
export class AudioDataWorkletSource extends AudioWorkletProcessor {
  private messageBuffer: MessageBuffer = new MessageBuffer();
  private currentChunk: undefined | Float32Array[];
  private chunkOffset: number = 0;

  constructor(...args: any[]) {
    // @ts-ignore
    super(...args);

    // @ts-ignore
    listenForMessages<AudioWorkletMessage, boolean>(this.port, async msg => {
      await this.messageBuffer.onMessage(msg);
      return true;
    });
  }

  process(_inputs: Input[], outputs: Output[], _parameters: unknown) {
    const expectedSampleCount = outputs[0]?.[0]?.length; // Should be always 128
    if (!expectedSampleCount) {
      return true;
    }

    let sampleCount = 0;
    const chunks: Float32Array[][] = [];
    while (sampleCount < expectedSampleCount) {
      const samplesLeft = expectedSampleCount - sampleCount;

      let currentChunkLength = this.currentChunk?.[0]?.length ?? 0;
      if (this.chunkOffset >= currentChunkLength) {
        this.currentChunk = this.messageBuffer.next();
        this.chunkOffset = 0;
        currentChunkLength = this.currentChunk?.[0]?.length ?? 0;
      }
      if (!this.currentChunk) {
        break;
      }
      const samplesToTake = Math.min(this.currentChunk[0].length - this.chunkOffset, samplesLeft);
      chunks.push(
        this.currentChunk.map(channelSamples =>
          channelSamples.subarray(this.chunkOffset, this.chunkOffset + samplesToTake)
        )
      );
      this.chunkOffset += samplesToTake;
      sampleCount += samplesToTake;
    }
    for (const output of outputs) {
      output.forEach((buffer: Float32Array, channelIndex) => {
        let offset = 0;
        for (const chunk of chunks) {
          buffer.set(chunk[channelIndex], offset);
          offset += chunk.length;
        }
      });
    }
    if (!this.messageBuffer.isRunning()) {
      // @ts-ignore
      this.port.close();
      return false;
    }
    return true;
  }
}

class MessageBuffer {
  private messageBuffer: Float32Array[][] = [];
  private onBufferConsumed?: () => void;
  private started: boolean = false;
  private receivedEos: boolean = false;

  public async onMessage(message: AudioWorkletMessage): Promise<void> {
    if (message.type === 'eos') {
      this.receivedEos = true;
    } else if (message.type === 'chunk') {
      this.messageBuffer.push(...message.chunks);
      while (this.messageBuffer.length > 100) {
        await new Promise<void>(res => {
          this.onBufferConsumed = res;
        });
      }
    }
  }

  public next(): Float32Array[] | undefined {
    if (!this.started) {
      // wait for small buffer before starting processing
      if (this.messageBuffer.length >= 10) {
        this.started = true;
      }
      return;
    }
    const next = this.messageBuffer.shift();
    if (!next) {
      throttledLogger('audioWorklet sample drop');
    }

    this.onBufferConsumed?.();
    return next;
  }

  public isRunning(): boolean {
    return this.messageBuffer.length > 0 || !this.receivedEos;
  }
}

/**
 *  Performance sensitive, throttle logs
 */
const throttledLogger = ((minDurationMs: number) => {
  let last = Date.now();
  let counter = 0;
  return (msg: string) => {
    const now = Date.now();
    if (now - last < minDurationMs) {
      counter += 1;
    } else {
      if (counter > 1) {
        console.warn(msg, `Skipped ${counter} messages`);
      } else {
        console.warn(msg);
      }
      counter = 0;
      last = now;
    }
  };
})(1000);
