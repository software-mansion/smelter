import type { Frame, InputId } from '@swmansion/smelter-browser-render';
import type { Input, InputStartResult } from './input';
import { InputVideoFrameRef } from './frame';
import type { Interval } from '../../utils';
import { SmelterEventType } from '../../eventSender';
import { workerPostEvent } from '../bridge';
import type { Logger } from 'pino';

export type InputState = 'started' | 'playing' | 'finished';

export class MediaStreamInput implements Input {
  private inputId: InputId;

  private frameRef?: InputVideoFrameRef;
  private reader: ReadableStreamDefaultReader<VideoFrame>;
  private readInterval?: Interval;

  private receivedEos: boolean = false;
  private sentEos: boolean = false;
  private sentFirstFrame: boolean = false;

  private logger: Logger;

  public constructor(inputId: InputId, source: ReadableStream, logger: Logger) {
    this.reader = source.getReader();
    this.inputId = inputId;
    this.logger = logger;
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
        if (this.frameRef) {
          this.frameRef.decrementRefCount();
        }
        this.frameRef = new InputVideoFrameRef(
          {
            frame: readResult.value,
            ptsMs: 0, // pts does not matter here
          },
          this.logger
        );
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
  }

  public updateQueueStartTime(_queueStartTimeMs: number) {}

  public async getFrame(_currentQueuePts: number): Promise<Frame | undefined> {
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
    const frameRef = this.frameRef;
    if (frameRef) {
      if (!this.sentFirstFrame) {
        this.sentFirstFrame = true;
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_PLAYING,
          inputId: this.inputId,
        });
      }
      // using Ref just to cache downloading frames if the same frame is used more than once
      frameRef.incrementRefCount();
      const frame = await frameRef.getFrame();
      frameRef.decrementRefCount();

      return frame;
    }
    return frameRef;
  }
}
