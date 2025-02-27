import type { Frame, InputId } from '@swmansion/smelter-browser-render';
import type { Logger } from 'pino';
import { Queue } from '@datastructures-js/queue';
import { workerPostEvent } from '../pipeline';
import { SmelterEventType } from '../../eventSender';
import type { Interval } from '../../utils';
import { assert, sleep } from '../../utils';
import type { Input, InputStartResult, InputVideoFrameSource, QueuedInputSource } from './input';
import type { InputVideoFrame } from './frame';
import { InputVideoFrameRef } from './frame';

export type InputState = 'started' | 'playing' | 'finished';

const MAX_BUFFER_FRAME_COUNT = 10;
const ENQUEUE_INTERVAL_MS = 50;

export class QueuedInput implements Input {
  private inputId: InputId;
  private source: QueuedInputSource;
  private logger: Logger;
  /**
   * frames PTS start from 0, where 0 represents first frame
   */
  private frames: Queue<InputVideoFrameRef>;

  private shouldClose: boolean = false;

  /**
   * Queue PTS of the first frame
   */
  private firstFrameTimeMs?: number;
  /**
   * Timestamp from first frame;
   * TODO: maybe consider always zeroing them earlier
   */
  private firstFramePtsMs?: number;
  /**
   * Start time of the queue
   */
  private queueStartTimeMs: number = 0;

  private receivedEos: boolean = false;
  private sentFirstFrame: boolean = false;

  public constructor(inputId: InputId, source: QueuedInputSource, logger: Logger) {
    this.inputId = inputId;
    this.source = source;
    this.logger = logger;
    this.frames = new Queue();
  }

  public start(): InputStartResult {
    void this.startAudioProcessor();
    void this.startVideoProcessor();

    workerPostEvent({
      type: SmelterEventType.VIDEO_INPUT_DELIVERED,
      inputId: this.inputId,
    });
    return this.source.getMetadata();
  }

  public close() {
    this.shouldClose = true;
  }

  public updateQueueStartTime(queueStartTimeMs: number) {
    this.queueStartTimeMs = queueStartTimeMs;
  }

  private async startAudioProcessor() {
    const port = this.source.audioWorkletMessagePort();
    while (!this.shouldClose) {
      const payload = this.source.nextBatch();
      if (!payload) {
        await sleep(50);
        continue;
      }
      if (payload.type === 'eos') {
        // TODO: maybe send EOS to worklet
        return;
      } else if (payload.type === 'sampleBatch') {
        await port.postMessage({ type: 'chunk', data: payload.sampleBatch.data as any }, [
          payload.sampleBatch.data,
        ]);
      }
    }
  }

  private async startVideoProcessor() {
    while (!this.shouldClose) {
      if (this.frames.size() >= MAX_BUFFER_FRAME_COUNT) {
        await sleep(ENQUEUE_INTERVAL_MS);
        continue;
      }
      const payload = this.source.nextFrame();
      if (!payload) {
        await sleep(ENQUEUE_INTERVAL_MS);
        continue;
      }
      if (payload?.type === 'eos') {
        this.receivedEos = true;
        return;
      } else if (payload.type === 'frame') {
        this.frames.push(this.newFrameRef(payload.frame));
      }
    }
  }

  /**
   * Retrieves reference of a frame closest to the provided `currentQueuePts`.
   */
  public async getFrame(currentQueuePts: number): Promise<Frame | undefined> {
    this.dropOldFrames(currentQueuePts);
    const frameRef = this.frames.front();
    if (frameRef) {
      frameRef.incrementRefCount();
      const frame = await frameRef.getFrame();
      frameRef.decrementRefCount();

      if (!this.sentFirstFrame) {
        this.sentFirstFrame = true;
        this.logger.debug('Input started');
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_PLAYING,
          inputId: this.inputId,
        });
      }

      if (this.frames.size() === 1 && this.receivedEos) {
        this.frames.pop().decrementRefCount();
        this.logger.debug('Input finished');
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_EOS,
          inputId: this.inputId,
        });
      }

      return frame;
    }
    return;
  }

  private newFrameRef(frame: InputVideoFrame): InputVideoFrameRef {
    if (!this.firstFrameTimeMs) {
      this.firstFrameTimeMs = Date.now();
    }
    if (!this.firstFramePtsMs) {
      this.firstFramePtsMs = frame.ptsMs;
    }
    frame.ptsMs = frame.ptsMs - this.firstFramePtsMs;
    return new InputVideoFrameRef(frame, this.logger);
  }

  /**
   * Finds frame with PTS closest to `framePts` and removes frames older than it
   */
  private dropOldFrames(queuePts: number): void {
    if (this.frames.isEmpty()) {
      return;
    }
    const inputPts = this.queuePtsToInputPts(queuePts);

    const frames = this.frames.toArray();
    const targetFrame = frames.reduce((prevFrame, frame) => {
      const prevPtsDiff = Math.abs(prevFrame.ptsMs - inputPts);
      const currPtsDiff = Math.abs(frame.ptsMs - inputPts);
      return prevPtsDiff < currPtsDiff ? prevFrame : frame;
    });

    for (const frame of frames) {
      if (frame.ptsMs < targetFrame.ptsMs) {
        frame.decrementRefCount();
        this.frames.pop();
      }
    }
  }

  private queuePtsToInputPts(queuePts: number): number {
    assert(this.firstFrameTimeMs);
    // TODO: handle before start
    const offsetMs = this.firstFrameTimeMs - this.queueStartTimeMs;
    return queuePts - offsetMs;
  }
}
