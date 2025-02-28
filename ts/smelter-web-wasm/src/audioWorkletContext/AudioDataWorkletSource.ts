import { assert } from '../utils';
import { listenForMessages } from './bridge';
import type { AudioWorkletMessage } from './workletApi';

type Output = Float32Array[];
type Input = Float32Array[];

export class AudioDataWorkletSource extends AudioWorkletProcessor {
  private messageBuffer: MessageBuffer = new MessageBuffer();

  constructor(...args: any[]) {
    // @ts-ignore
    super(...args);

    console.log({ args, test: 11 });
    listenForMessages<AudioWorkletMessage, boolean>(this.port, async msg => {
      await this.messageBuffer.onMessage(msg);
      return true;
    });
  }

  process(_inputs: Input[], outputs: Output[], _parameters: unknown) {
    const outputChannels = outputs[0];
    const audioData = this.messageBuffer.next();
    if (audioData) {
      outputChannels.forEach((buffer: Float32Array, index) => {
        outputChannels[index] = audioData[index].subarray(0, 128);
      });
    }
    return true;
  }
}

class MessageBuffer {
  private messageBuffer: AudioWorkletMessage[] = [];
  private onBufferConsumed?: () => void;

  public async onMessage(message: AudioWorkletMessage): Promise<void> {
    this.messageBuffer.push(message);
    console.log('message');
    while (this.messageBuffer.length > 10) {
      await new Promise<void>(res => {
        this.onBufferConsumed = res;
      });
    }
  }

  public next(): Float32Array[] | undefined {
    const next = this.messageBuffer.shift()?.data;
    this.onBufferConsumed?.();
    return next;
  }
}
