import type {
  FrameSet,
  InputId,
  OutputFrame,
  OutputId,
  Renderer,
} from '@swmansion/smelter-browser-render';
import type { Framerate } from '../compositor/compositor';
import type { Input } from './input/input';
import type { Output } from './output/output';
import { sleep } from '../utils';
import type { Logger } from 'pino';
import type { InputVideoFrame } from './input/frame';

export type StopQueueFn = () => void;

export class Queue {
  private inputs: Record<InputId, Input> = {};
  private outputs: Record<OutputId, Output> = {};
  private renderer: Renderer;
  private logger: Logger;
  private frameTicker: FrameTicker;
  private startTimeMs?: number;

  public constructor(
    framerate: Framerate,
    renderer: Renderer,
    logger: Logger,
    workloadBalancer: WorkloadBalancer
  ) {
    this.renderer = renderer;
    this.logger = logger;
    this.frameTicker = new FrameTicker(framerate, logger, workloadBalancer);
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

    try {
      const outputs = this.renderer.render({
        ptsMs: currentPtsMs,
        frames,
      });
      this.sendOutputs(outputs);
    } finally {
      for (const frame of Object.values(frames)) {
        frame.close();
      }
    }
  }

  private async getInputFrames(currentPtsMs: number): Promise<Record<InputId, InputVideoFrame>> {
    const frames: Array<[InputId, InputVideoFrame | undefined]> = await Promise.all(
      Object.entries(this.inputs).map(async ([inputId, input]) => [
        inputId,
        await input.getFrame(currentPtsMs),
      ])
    );
    const validFrames = frames.filter((entry): entry is [string, InputVideoFrame] => !!entry[1]);
    return Object.fromEntries(validFrames);
  }

  private sendOutputs(outputs: FrameSet<OutputFrame>) {
    for (const [outputId, frame] of Object.entries(outputs.frames)) {
      const output = this.outputs[outputId];
      if (!output) {
        this.logger.info(`Output "${outputId}" not found`);
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

  private shouldClose: boolean = false;
  private pendingTick?: Promise<void>;

  private startTimeMs: number = 0; // init on start
  private frameCounter: number = 0;

  private workloadBalancerNode: WorkloadBalancerNode;

  constructor(framerate: Framerate, logger: Logger, workloadBalancer: WorkloadBalancer) {
    this.framerate = framerate;
    this.logger = logger;
    this.workloadBalancerNode = workloadBalancer.lowPriorityNode();
  }

  public start(startTimeMs: number, onTick: (pts: number) => Promise<void>) {
    this.onTick = onTick;
    this.startTimeMs = startTimeMs;
    void this.runSchedulingLoop();
  }

  public stop() {
    this.shouldClose = true;
    this.workloadBalancerNode.close();
  }

  private async runSchedulingLoop(): Promise<void> {
    while (!this.shouldClose) {
      try {
        this.pendingTick = this.doTick();
        await this.pendingTick;
      } catch (err) {
        this.logger.warn(err, 'Render error');
      }

      const timeoutDuration = this.startTimeMs + this.currentPtsMs() - Date.now();
      await sleep(Math.max(timeoutDuration, 2) * this.workloadBalancerNode.throttlingFactor);
    }
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
      this.logger.info(`Processing too slow, dropping frame (frameCounter: ${this.frameCounter})`);
      this.frameCounter += 1;
    }
  }
}

/**
 * `render` method from @swmansion/smelter-browser-render is synchronous and
 * takes long time. On devices that can't process everything on time it can
 * starve other work on this thread like audio decoder.
 *
 * WorkloadBalancer exposes api to create nodes. Each node adds an API
 * that allows:
 * - node to signal if processing happens on time
 * - balancer to signal to node if it should be throttled
 */
export class WorkloadBalancer {
  private highPriorityNodes: Set<WorkloadBalancerNode> = new Set();
  private lowPriorityNodes: Set<WorkloadBalancerNode> = new Set();

  constructor() {
    setInterval(() => {
      this.recalculateThrottling();
    }, 100);
  }

  public highPriorityNode(): WorkloadBalancerNode {
    const node = new WorkloadBalancerNode(this);
    this.highPriorityNodes.add(node);
    return node;
  }

  public lowPriorityNode(): WorkloadBalancerNode {
    const node = new WorkloadBalancerNode(this);
    this.lowPriorityNodes.add(node);
    return node;
  }

  private recalculateThrottling() {
    const now = Date.now();
    const minHighPriorityState = [...this.highPriorityNodes].reduce(
      (aac, value) => (now - value.stateUpdateTimestamp > 2000 ? aac : Math.min(value.state, aac)),
      1
    );
    for (const lowPriorityNode of this.lowPriorityNodes) {
      const factor = 1 - (minHighPriorityState - 0.5) * 0.4; // value between [0.8, 1.2]
      const newThrottlingFactor = lowPriorityNode.throttlingFactor * factor;
      lowPriorityNode.throttlingFactor = Math.min(10, Math.max(1, newThrottlingFactor));
    }
  }

  public remove(node: WorkloadBalancerNode) {
    this.highPriorityNodes.delete(node);
    this.lowPriorityNodes.delete(node);
  }
}

export class WorkloadBalancerNode {
  private balancer: WorkloadBalancer;
  /*
   * number between 0 and 1 representing state
   * 0 - failed
   * 0.5 - target
   * 1 - working on time
   */
  public state: number = 0.5;
  public stateUpdateTimestamp: number = Date.now();

  public throttlingFactor: number = 1;

  constructor(balancer: WorkloadBalancer) {
    this.balancer = balancer;
  }

  public setState(state: number) {
    this.state = state;
    this.stateUpdateTimestamp = Date.now();
  }

  public close() {
    this.balancer.remove(this);
  }
}
