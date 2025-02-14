import type {
  Frame,
  FrameSet,
  InputId,
  OutputId,
  Renderer,
} from '@swmansion/smelter-browser-render';
import { FrameFormat } from '@swmansion/smelter-browser-render';
import type { Framerate } from '../compositor/compositor';
import type { Input } from './input/input';
import type { Output } from './output/output';
import type { Timeout } from '../utils';
import type { Logger } from 'pino';

export type StopQueueFn = () => void;

export class Queue {
  private inputs: Record<InputId, Input> = {};
  private outputs: Record<OutputId, Output> = {};
  private renderer: Renderer;
  private logger: Logger;
  private frameTicker: FrameTicker;
  private startTimeMs?: number;
  private inputFrameFormat: FrameFormat;

  public constructor(framerate: Framerate, renderer: Renderer, logger: Logger, isSafari: boolean) {
    this.renderer = renderer;
    this.logger = logger;
    this.frameTicker = new FrameTicker(framerate, logger);
    this.inputFrameFormat = isSafari ? FrameFormat.YUV_BYTES : FrameFormat.RGBA_BYTES;
  }

  public start() {
    this.logger.debug('Start queue');
    this.startTimeMs = Date.now();
    this.frameTicker.start(this.startTimeMs, async (pts: number) => {
      await this.onTick(pts);
    });
    for (const input of Object.values(this.inputs)) {
      input.updateQueueStartTime(this.startTimeMs);
    }
  }

  public stop() {
    this.frameTicker.stop();
    for (const input of Object.values(this.inputs)) {
      input.close();
    }
  }

  public addInput(inputId: InputId, input: Input) {
    if (this.inputs[inputId]) {
      throw new Error(`Input "${inputId}" already exists`);
    }
    if (this.startTimeMs) {
      input.updateQueueStartTime(this.startTimeMs);
    }
    this.inputs[inputId] = input;
  }

  public removeInput(inputId: InputId) {
    delete this.inputs[inputId];
  }

  public getInput(inputId: InputId): Input | undefined {
    return this.inputs[inputId];
  }

  public addOutput(outputId: OutputId, output: Output) {
    if (this.outputs[outputId]) {
      throw new Error(`Output "${outputId}" already exists`);
    }
    this.outputs[outputId] = output;
  }

  public removeOutput(outputId: OutputId) {
    delete this.outputs[outputId];
  }

  public getOutput(outputId: OutputId): Output | undefined {
    return this.outputs[outputId];
  }

  private async onTick(currentPtsMs: number): Promise<void> {
    const frames = await this.getInputFrames(currentPtsMs);
    this.logger.trace({ frames }, 'onQueueTick');

    const outputs = this.renderer.render({
      ptsMs: currentPtsMs,
      frames,
    });
    this.sendOutputs(outputs);
  }

  private async getInputFrames(currentPtsMs: number): Promise<Record<InputId, Frame>> {
    const frames: Array<[InputId, Frame | undefined]> = await Promise.all(
      Object.entries(this.inputs).map(async ([inputId, input]) => [
        inputId,
        await input.getFrame(currentPtsMs, this.inputFrameFormat),
      ])
    );
    const validFrames = frames.filter((entry): entry is [string, Frame] => !!entry[1]);

    return Object.fromEntries(validFrames);
  }

  private sendOutputs(outputs: FrameSet) {
    for (const [outputId, frame] of Object.entries(outputs.frames)) {
      const output = this.outputs[outputId];
      if (!output) {
        this.logger.warn(`Output "${outputId}" not found`);
        continue;
      }
      void output.send(frame);
    }
  }
}

class FrameTicker {
  private framerate: Framerate;
  private onTick?: (pts: number) => Promise<void>;
  private logger: Logger;

  private timeout?: Timeout;
  private pendingTick?: Promise<void>;

  private startTimeMs: number = 0; // init on start
  private frameCounter: number = 0;

  constructor(framerate: Framerate, logger: Logger) {
    this.framerate = framerate;
    this.logger = logger;
  }

  public start(startTimeMs: number, onTick: (pts: number) => Promise<void>) {
    this.onTick = onTick;
    this.startTimeMs = startTimeMs;
    this.scheduleNext();
  }

  public stop() {
    if (this.timeout) {
      clearTimeout(this.timeout);
      this.timeout = undefined;
    }
  }

  private scheduleNext() {
    const timeoutDuration = this.startTimeMs + this.currentPtsMs() - Date.now();
    this.timeout = setTimeout(
      () => {
        void this.doTick();
        this.scheduleNext();
      },
      Math.max(timeoutDuration, 0)
    );
  }

  private async doTick(): Promise<void> {
    if (this.pendingTick) {
      return;
    }
    this.maybeSkipFrames();
    try {
      this.pendingTick = this.onTick?.(this.currentPtsMs());
      await this.pendingTick;
    } catch (err: any) {
      this.logger.warn(err, 'Queue tick failed.');
    }
    this.pendingTick = undefined;
    this.frameCounter += 1;
  }

  private currentPtsMs(): number {
    return this.frameCounter * 1000 * (this.framerate.den / this.framerate.num);
  }

  private maybeSkipFrames() {
    const frameDurationMs = 1000 * (this.framerate.den / this.framerate.num);
    while (Date.now() - this.startTimeMs > this.currentPtsMs() + frameDurationMs * 2) {
      this.logger.info(`Processing to slow, dropping frame (frameCounter: ${this.frameCounter})`);
      this.frameCounter += 1;
    }
  }
}
