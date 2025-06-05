import type { InputId } from '@swmansion/smelter-browser-render';
import type { Input, InputStartResult } from './input';
import type { InputVideoFrame } from './frame';
import { InputFrameFromVideoElement } from './frame';
import type { MainThreadHandle } from '../../workerApi';

export type InputState = 'started' | 'playing' | 'finished';

export class HTMLVideoElementInput implements Input {
  private inputId: InputId;
  private mainThreadHandle: MainThreadHandle;
  private videoElement: HTMLVideoElement;

  public constructor(
    inputId: InputId,
    videoElement: HTMLVideoElement,
    mainThreadHandle: MainThreadHandle
  ) {
    this.videoElement = videoElement;
    this.inputId = inputId;
    this.mainThreadHandle = mainThreadHandle;
  }

  public start(): InputStartResult {
    return {};
  }

  public close() {}

  public updateQueueStartTime(_queueStartTimeMs: number) {}

  public async getFrame(currentQueuePts: number): Promise<InputVideoFrame | undefined> {
    return new InputFrameFromVideoElement(this.videoElement, currentQueuePts);
  }
}
