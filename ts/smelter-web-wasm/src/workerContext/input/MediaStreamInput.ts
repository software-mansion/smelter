import type { InputId } from '@swmansion/smelter-browser-render';
import type { Input, InputStartResult } from './input';
import { InputVideoFrame, InputVideoFrameRef } from './frame';
import type { Interval } from '../../utils';
import { SmelterEventType } from '../../eventSender';
import { workerPostEvent } from '../bridge';

export type InputState = 'started' | 'playing' | 'finished';

export class MediaStreamInput implements Input {
  private inputId: InputId;

  private frame?: InputVideoFrameRef;
  private reader: ReadableStreamDefaultReader<VideoFrame>;
  private readInterval?: Interval;

  private receivedEos: boolean = false;
  private sentEos: boolean = false;
  private sentFirstFrame: boolean = false;

  public constructor(inputId: InputId, source: ReadableStream) {
    this.reader = source.getReader();
    this.inputId = inputId;
  }

  public start(): InputStartResult {
    let readPromise: Promise<ReadableStreamReadResult<VideoFrame>> | undefined;
    this.readInterval = setInterval(async () => {
      if (readPromise) {
        return;
      }
      readPromise = this.reader.read();
      const readResult = await readPromise;
      if (readResult.value) {
        if (this.frame) {
          this.frame.decrementRefCount();
        }
        this.frame = new InputVideoFrameRef(readResult.value);
      }

      if (readResult.done) {
        this.close();
        this.receivedEos = true;
      }
      readPromise = undefined;
    }, 30);
    workerPostEvent({
      type: SmelterEventType.VIDEO_INPUT_DELIVERED,
      inputId: this.inputId,
    });
    return {};
  }

  public close() {
    if (this.readInterval) {
      clearInterval(this.readInterval);
    }
    if (this.frame) {
      this.frame.decrementRefCount();
      this.frame = undefined;
    }
  }

  public updateQueueStartTime(_queueStartTimeMs: number) {}

  public async getFrame(currentQueuePts: number): Promise<InputVideoFrame | undefined> {
    if (this.receivedEos) {
      if (!this.sentEos) {
        this.sentEos = true;
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_EOS,
          inputId: this.inputId,
        });
      }
      return;
    }
    const frame = this.frame;
    frame?.incrementRefCount();
    if (frame) {
      if (!this.sentFirstFrame) {
        this.sentFirstFrame = true;
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_PLAYING,
          inputId: this.inputId,
        });
      }
      // using Ref just to cache downloading frames if the same frame is used more than once
      return new InputVideoFrame(frame, currentQueuePts);
    }

    return;
  }
}
