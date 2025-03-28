import { assert } from '../../utils';

export type InputVideoFrame = {
  frame: VideoFrame;
  ptsMs: number;
};

export type InputAudioData = {
  data: Omit<AudioData, 'timestamp'>;
  ptsMs: number;
};

/**
 * Represents frame produced by decoder.
 * Memory has to be manually managed by incrementing reference count on `FrameRef` copy and decrementing it once it's no longer used
 * `Input` manages memory in `getFrameRef()`
 * `Queue` on tick pulls `FrameRef` for each input and once render finishes, decrements the ref count
 */
export class InputVideoFrameRef {
  private frame: InputVideoFrame;
  private refCount: number;

  public constructor(frame: InputVideoFrame) {
    this.frame = frame;
    this.refCount = 1;
  }

  public get ptsMs(): number {
    return this.frame.ptsMs;
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
      this.frame.frame.close();
    }
  }

  /**
   * Returns underlying frame. Fails if frame was freed from memory.
   */
  public getFrame(): VideoFrame {
    assert(this.refCount > 0);
    return this.frame.frame;
  }
}
