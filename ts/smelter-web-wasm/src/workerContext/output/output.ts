import type { OutputFrame, Resolution } from '@swmansion/smelter-browser-render';
import type { OutputSink } from './sink';
import CanvasSink from './canvas';
import type { RegisterOutput } from '../../workerApi';

export class Output {
  private sink: OutputSink;
  public readonly resolution: Resolution;

  public constructor(request: RegisterOutput) {
    if (request.type === 'stream' && request.video) {
      this.sink = new CanvasSink(request.video.canvas);
    } else {
      throw new Error(`Unknown output type ${(request as any).type}`);
    }
    this.resolution = request.video.resolution;
  }

  public async send(frame: OutputFrame): Promise<void> {
    await this.sink.send(frame);
  }
}
