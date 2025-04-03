import type { OutputFrame } from '@swmansion/smelter-browser-render';
import type { OutputSink } from './sink';
import { assert } from '../../utils';

export default class CanvasSink implements OutputSink {
  private ctx: OffscreenCanvasRenderingContext2D;

  public constructor(canvas: OffscreenCanvas) {
    const ctx = canvas.getContext('2d', { desynchronized: false });
    assert(ctx, 'Failed to instantiate a context.');
    this.ctx = ctx;
  }

  public async send(frame: OutputFrame): Promise<void> {
    const resolution = frame.resolution;
    this.ctx.putImageData(new ImageData(frame.data, resolution.width, resolution.height), 0, 0);
  }
}
