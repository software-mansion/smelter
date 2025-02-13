import type { Frame } from '@swmansion/smelter-browser-render';
import { FrameFormat } from '@swmansion/smelter-browser-render';
import { assert } from '../../utils';
import type { Logger } from 'pino';

export type InputVideoFrame = {
  frame: Omit<VideoFrame, 'timestamp'>;
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
  private downloadedFrame?: Frame;
  private logger: Logger;

  public constructor(frame: InputVideoFrame, logger: Logger) {
    this.frame = frame;
    this.logger = logger;
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
  public async getFrame(): Promise<Frame> {
    assert(this.refCount > 0);

    if (!this.downloadedFrame) {
      this.downloadedFrame = await this.downloadFrame(this.frame);
    }
    return this.downloadedFrame;
  }

  private async downloadFrame(inputFrame: InputVideoFrame): Promise<Frame> {
    // TODO: Add support back from safari
    // Safari does not support conversion to RGBA
    // Chrome does not support conversion to YUV

    const frame = inputFrame.frame;

    // visibleRect is undefined when inputFrame is detached
    assert(frame.visibleRect);

    const options = {
      format: 'RGBA',
      layout: [
        {
          offset: 0,
          stride: frame.visibleRect.width * 4,
        },
      ],
    };

    const buffer = new Uint8ClampedArray(frame.allocationSize(options as VideoFrameCopyToOptions));
    const planeLayouts = await frame.copyTo(buffer, options as VideoFrameCopyToOptions);

    if (!checkPlaneLayouts(options.layout, planeLayouts)) {
      const frameInfo = {
        displayWidth: frame.displayWidth,
        displayHeight: frame.displayHeight,
        codedWidth: frame.codedWidth,
        codedHeight: frame.codedHeight,
        visibleRect: frame.visibleRect,
        codedRect: frame.codedRect,
        format: frame.format,
        colorSpace: frame.colorSpace,
        duration: frame.duration,
      };

      this.logger.error(
        { planeLayouts, frameInfo },
        "Copied frame's plane layouts do not match expected layouts"
      );
    }

    return {
      resolution: {
        width: frame.visibleRect.width,
        height: frame.visibleRect.height,
      },
      format: FrameFormat.RGBA_BYTES,
      data: buffer,
    };
  }
}

/**
 * Returns `true` if plane layouts are valid
 */
function checkPlaneLayouts(expected: PlaneLayout[], received: PlaneLayout[]): boolean {
  if (expected.length !== received.length) {
    return false;
  }
  for (let i = 0; i < expected.length; i++) {
    if (expected[i].offset !== received[i].offset) {
      return false;
    }
    if (expected[i].stride !== received[i].stride) {
      return false;
    }
  }

  return true;
}
