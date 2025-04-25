import type { InputId } from '@swmansion/smelter-browser-render';
import type { Logger } from 'pino';
import { Queue } from '@datastructures-js/queue';
import { workerPostEvent } from '../pipeline';
import { SmelterEventType } from '../../eventSender';
import { assert, sleep } from '../../utils';
import type { Input, InputStartResult, QueuedInputSource } from './input';
import type { InputAudioData, InternalVideoFrame } from './frame';
import { InputVideoFrame, InputVideoFrameRef } from './frame';
import type { AudioWorkletMessage } from '../../audioWorkletContext/workletApi';

export type InputState = 'started' | 'playing' | 'finished';

const MAX_BUFFER_FRAME_COUNT = 10;
const ENQUEUE_INTERVAL_MS = 50;

type QueueFrame = {
  ref: InputVideoFrameRef;
  ptsMs: number;
};

export class QueuedInput implements Input {
  private inputId: InputId;
  private source: QueuedInputSource;
  private logger: Logger;
  /**
   * frames PTS start from 0, where 0 represents first frame
   */
  private frames: Queue<QueueFrame>;

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
    if (!port) {
      return;
    }
    const buffer = new AudioBatchBuffer();
    while (!this.shouldClose) {
      const payload = this.source.nextBatch();
      if (!payload) {
        const workletMessage = buffer.flush();
        if (workletMessage) {
          await port.postMessage(workletMessage);
        }
        await sleep(50);
        continue;
      }
      if (payload.type === 'eos') {
        const workletMessage = buffer.flush();
        if (workletMessage) {
          await port.postMessage(workletMessage);
        }
        await port.postMessage({ type: 'eos' });
        port.terminate();
        return;
      } else if (payload.type === 'sampleBatch') {
        const workletMessage = buffer.next(payload.sampleBatch);
        if (workletMessage) {
          await port.postMessage(workletMessage);
        }
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
  public async getFrame(currentQueuePts: number): Promise<InputVideoFrame | undefined> {
    this.dropOldFrames(currentQueuePts);
    const frame = this.frames.front();
    if (frame) {
      frame.ref.incrementRefCount();
      if (!this.sentFirstFrame) {
        this.sentFirstFrame = true;
        this.logger.debug('Input started');
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_PLAYING,
          inputId: this.inputId,
        });
      }

      if (this.frames.size() === 1 && this.receivedEos) {
        this.frames.pop().ref.decrementRefCount();
        this.logger.debug('Input finished');
        workerPostEvent({
          type: SmelterEventType.VIDEO_INPUT_EOS,
          inputId: this.inputId,
        });
      }

      return new InputVideoFrame(frame.ref, frame.ptsMs);
    }
    return;
  }

  private newFrameRef(frame: InternalVideoFrame): QueueFrame {
    if (!this.firstFrameTimeMs) {
      this.firstFrameTimeMs = Date.now();
    }
    if (!this.firstFramePtsMs) {
      this.firstFramePtsMs = frame.ptsMs;
    }
    frame.ptsMs = frame.ptsMs - this.firstFramePtsMs;
    return {
      ref: new InputVideoFrameRef(frame.frame),
      ptsMs: frame.ptsMs,
    };
  }

  /**
   * Finds frame with PTS closest to `queuePts` and removes frames older than it
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

    this.frames = Queue.fromArray(
      frames.filter(frame => {
        if (frame.ptsMs < targetFrame.ptsMs) {
          frame.ref.decrementRefCount();
          return false;
        }
        return true;
      })
    );
  }

  private queuePtsToInputPts(queuePts: number): number {
    assert(this.firstFrameTimeMs);
    // TODO: handle before start
    const offsetMs = this.firstFrameTimeMs - this.queueStartTimeMs;
    return queuePts - offsetMs;
  }
}

const BATCH_SIZE = 10;

class AudioBatchBuffer {
  private buffer: Float32Array[][] = [];

  public next(audioData: InputAudioData): AudioWorkletMessage | undefined {
    const channelCount = audioData.data.numberOfChannels;
    const channels = [...Array(channelCount)].map((_, index) => {
      const size = audioData.data.allocationSize({
        format: 'f32-planar',
        planeIndex: index,
      });
      const buffer = new Float32Array(size / Float32Array.BYTES_PER_ELEMENT);
      audioData.data.copyTo(buffer, {
        format: 'f32-planar',
        planeIndex: index,
      });
      return buffer;
    });
    audioData.data.close();
    this.buffer.push(channels);
    if (this.buffer.length > BATCH_SIZE) {
      return this.flush();
    }
    return;
  }

  public flush(): AudioWorkletMessage | undefined {
    if (this.buffer.length === 0) {
      return;
    }
    const chunks = this.buffer;
    this.buffer = [];
    return {
      type: 'chunk',
      chunks,
    };
  }
}
