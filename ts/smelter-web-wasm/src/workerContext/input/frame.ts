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

export class InputVideoFrame {
  private readonly ref: InputVideoFrameRef;
  public readonly ptsMs: number;

  constructor(ref: InputVideoFrameRef, ptsMs: number) {
    this.ref = ref;
    this.ptsMs = ptsMs;
  }

  get frame(): ImageBitmap {
    return this.ref.getFrame();
  }

  close(): void {
    this.ref.decrementRefCount();
  }
}

/**
 * Represents ref counted frame.
 */
export class InputVideoFrameRef {
  // Frame is only needed for cleanup
  public readonly frame: VideoFrame;
  public readonly frameData: ImageBitmap;
  private refCount: number;

  private constructor(frameData: ImageBitmap, frame: VideoFrame) {
    this.frame = frame;
    this.frameData = frameData;
    this.refCount = 1;
  }

  public static async fromVideoFrame(frame: VideoFrame): Promise<InputVideoFrameRef> {
    const frameData = await createImageBitmap(frame);
    return new InputVideoFrameRef(frameData, frame);
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
      this.frameData.close();
      this.frame.close();
    }
  }

  /**
   * Returns underlying frame. Fails if frame was freed from memory.
   */
  public getFrame(): ImageBitmap {
    assert(this.refCount > 0);
    return this.frameData;
  }
}
