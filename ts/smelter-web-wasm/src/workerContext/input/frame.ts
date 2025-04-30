import { assert } from '../../utils';

/**
 * Frame used internally (between decoder and input)
 */
export type InternalVideoFrame = {
  frame: VideoFrame;
  ptsMs: number;
};

export type InputAudioData = {
  data: Omit<AudioData, 'timestamp'>;
  ptsMs: number;
};

export class InputFrameFromVideoFrame {
  private readonly ref: InputVideoFrameRef;
  public readonly ptsMs: number;

  constructor(ref: InputVideoFrameRef, ptsMs: number) {
    this.ref = ref;
    this.ptsMs = ptsMs;
  }

  get frame(): VideoFrame | HTMLVideoElement {
    return this.ref.getFrame();
  }

  close(): void {
    this.ref.decrementRefCount();
  }
}

export class InputFrameFromVideoElement {
  private element: HTMLVideoElement;
  public readonly ptsMs: number;

  constructor(element: HTMLVideoElement, ptsMs: number) {
    this.element = element;
    this.ptsMs = ptsMs;
  }

  get frame(): VideoFrame | HTMLVideoElement {
    return this.element;
  }

  close(): void {}
}

export interface InputVideoFrame {
  readonly ptsMs: number;
  readonly frame: VideoFrame | HTMLVideoElement;
  close(): void;
}

/**
 * Represents ref counted frame.
 */
export class InputVideoFrameRef {
  public readonly frame: VideoFrame;
  private refCount: number;

  public constructor(frame: VideoFrame) {
    this.frame = frame;
    this.refCount = 1;
  }

  /**
   *  Increments reference count. Should be called every time the reference is copied.
   */
  public incrementRefCount(): void {
    assert(this.refCount > 0);
    this.refCount++;
  }

  /**
   * Decrements reference count. If reference count reaches 0, `FrameWithPts` is freed from the memory.
   * It's unsafe to use the returned frame after `decrementRefCount()` call.
   * Should be used after we're sure we no longer need the frame.
   */
  public decrementRefCount(): void {
    assert(this.refCount > 0);

    this.refCount--;
    if (this.refCount === 0) {
      this.frame.close();
    }
  }

  /**
   * Returns underlying frame. Fails if frame was freed from memory.
   */
  public getFrame(): VideoFrame {
    assert(this.refCount > 0);
    return this.frame;
  }
}
