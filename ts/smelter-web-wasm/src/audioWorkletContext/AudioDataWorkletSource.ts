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

    listenForMessages<AudioWorkletMessage, boolean>(this.port, async msg => {
      await this.messageBuffer.onMessage(msg);
      return true;
    });
  }

  process(_inputs: Input[], outputs: Output[], _parameters: unknown) {
    const outputChannels = outputs[0];
    const audioData = this.messageBuffer.next();
    if (audioData) {
      assert(
        audioData.format === 'f32-planar',
        `Unsupported audio data format ${audioData.format}`
      );
      outputChannels.forEach((buffer: Float32Array, index) => {
        const destinationFormat: AudioDataCopyToOptions = {
          planeIndex: index,
          frameCount: 128,
        };
        // TODO: fix
        audioData.copyTo(buffer, destinationFormat);
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
    while (this.messageBuffer.length > 10) {
      await new Promise<void>(res => {
        this.onBufferConsumed = res;
      });
    }
  }

  public next(): AudioData | undefined {
    const next = this.messageBuffer.shift()?.data;
    this.onBufferConsumed?.();
    return next;
  }
}
