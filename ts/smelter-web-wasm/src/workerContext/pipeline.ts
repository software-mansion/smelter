import type { Component, ShaderSpec, ImageSpec, Renderer } from '@swmansion/smelter-browser-render';
import type { Framerate } from '../compositor/compositor';
import type { Logger } from 'pino';
import type { Api } from '@swmansion/smelter';
import { createInput } from './input/input';
import { Output } from './output/output';
import { Queue, WorkloadBalancer } from './queue';
import type {
  MainThreadHandle,
  RegisterInput,
  RegisterOutput,
  WorkerMessage,
  WorkerResponse,
} from '../workerApi';
import { SmelterEventType } from '../eventSender';

export class Pipeline {
  private workloadBalancer: WorkloadBalancer = new WorkloadBalancer();
  private handle: MainThreadHandle;
  private renderer: Renderer;
  private queue: Queue;
  private logger: Logger;
  private started = false;

  public constructor(options: {
    renderer: Renderer;
    framerate: Framerate;
    logger: Logger;
    handle: MainThreadHandle;
  }) {
    this.renderer = options.renderer;
    this.logger = options.logger.child({ element: 'pipeline' });
    this.handle = options.handle;
    this.queue = new Queue(
      options.framerate,
      options.renderer,
      options.logger,
      this.workloadBalancer
    );
  }

  public async handleRequest(request: WorkerMessage): Promise<WorkerResponse> {
    if (request.type === 'registerInput') {
      return await this.registerInput(request.inputId, request.input);
    } else if (request.type === 'registerOutput') {
      return this.registerOutput(request.outputId, request.output);
    } else if (request.type === 'registerImage') {
      return await this.registerImage(request.imageId, request.image);
    } else if (request.type === 'registerShader') {
      return await this.registerShader(request.shaderId, request.shader);
    } else if (request.type === 'unregisterInput') {
      return await this.unregisterInput(request.inputId);
    } else if (request.type === 'unregisterOutput') {
      return await this.unregisterOutput(request.outputId);
    } else if (request.type === 'unregisterImage') {
      return this.unregisterImage(request.imageId);
    } else if (request.type === 'unregisterShader') {
      return this.unregisterShader(request.shaderId);
    } else if (request.type === 'updateScene') {
      return this.updateScene(request.outputId, request.output);
    } else if (request.type === 'registerFont') {
      return this.registerFont(request.url);
    } else if (request.type === 'start') {
      return this.start();
    } else if (request.type === 'terminate') {
      return await this.terminate();
    } else {
      this.logger.warn(request, 'Web worker received unknown message.');
    }
  }

  public start() {
    if (this.started) {
      throw new Error('Smelter was already started');
    }
    this.started = true;
    this.queue.start();
  }

  public async terminate(): Promise<void> {
    this.queue.stop();
  }

  public async registerInput(inputId: string, request: RegisterInput): Promise<WorkerResponse> {
    const input = await createInput(
      inputId,
      request,
      this.logger,
      this.workloadBalancer,
      this.handle
    );
    // `addInput` will throw an exception if input already exists
    this.queue.addInput(inputId, input);
    await this.renderer.registerInput(inputId);
    const result = input.start();
    return {
      type: 'registerInput',
      body: {
        video_duration_ms: result.videoDurationMs,
        audio_duration_ms: result.audioDurationMs,
      },
    };
  }

  public async unregisterInput(inputId: string): Promise<void> {
    this.queue.removeInput(inputId);
    await this.renderer.unregisterInput(inputId);
  }

  public async registerOutput(outputId: string, request: RegisterOutput) {
    if (request.video) {
      const output = new Output(request);
      this.queue.addOutput(outputId, output);
      try {
        // `updateScene` implicitly registers the output.
        // In case of an error, the output has to be manually cleaned up from the renderer.
        await this.renderer.updateScene(
          outputId,
          request.video.resolution,
          request.video.initial.root as Component
        );
      } catch (e) {
        this.queue.removeOutput(outputId);
        await this.renderer.unregisterOutput(outputId);
        throw e;
      }
    }
  }

  public async unregisterOutput(outputId: string): Promise<void> {
    this.queue.removeOutput(outputId);
    await this.renderer.unregisterOutput(outputId);
    // If we add outputs that can end early or require flushing
    // then this needs to be change
    this.handle.postEvent({
      type: SmelterEventType.OUTPUT_DONE,
      outputId,
    });
  }

  public async updateScene(outputId: string, request: Api.UpdateOutputRequest) {
    if (!request.video) {
      return;
    }
    const output = this.queue.getOutput(outputId);
    if (!output) {
      throw new Error(`Unknown output "${outputId}"`);
    }
    await this.renderer.updateScene(outputId, output.resolution, request.video.root as Component);
  }

  public async registerImage(imageId: string, request: ImageSpec) {
    await this.renderer.registerImage(imageId, request);
  }

  public async unregisterImage(imageId: string) {
    await this.renderer.unregisterImage(imageId);
  }

  public async registerShader(shaderId: string, request: ShaderSpec) {
    await this.renderer.registerShader(shaderId, request);
  }

  public async unregisterShader(shaderId: string) {
    await this.renderer.unregisterShader(shaderId);
  }

  public async registerFont(url: string): Promise<void> {
    await this.renderer.registerFont(url);
  }
}
