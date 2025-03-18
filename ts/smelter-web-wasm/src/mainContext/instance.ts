import type {
  ApiRequest,
  SmelterManager,
  Input as CoreInput,
  Output as CoreOutput,
  MultipartRequest,
} from '@swmansion/smelter-core';
import type { Framerate } from '../compositor/compositor';
import type { WorkerEvent, WorkerMessage, WorkerResponse } from '../workerApi';
import { EventSender } from '../eventSender';
import { Path } from 'path-parser';
import { assert } from '../utils';
import type { ImageSpec } from '@swmansion/smelter-browser-render';
import type { Api } from '@swmansion/smelter';
import type { Logger } from 'pino';
import { AsyncWorker } from '../workerContext/bridge';
import type { RegisterOutputResponse, Output } from './output';
import { handleRegisterOutputRequest } from './output';
import type { Input } from './input';
import { handleRegisterInputRequest } from './input';
import { AudioMixer } from './AudioMixer';

const apiPath = new Path('/api/:type/:id/:operation');
const apiStartPath = new Path('/api/start');

export type InstanceContext = {
  logger: Logger;
  audioMixer: AudioMixer;
  framerate: Framerate;
};

class WasmInstance implements SmelterManager {
  private instance?: InnerInstance;

  public constructor(options: { framerate: Framerate; wasmBundleUrl: string; logger: Logger }) {
    this.instance = new InnerInstance(options);
  }

  public async setupInstance(): Promise<void> {
    return this.instance?.setupInstance();
  }

  public async sendRequest(request: ApiRequest): Promise<object> {
    return await this.handleRequest(request);
  }

  sendMultipartRequest(_request: MultipartRequest): Promise<object> {
    throw new Error('Method sendMultipartRequest not implemented for web-wasm.');
  }

  public async registerFont(fontUrl: string): Promise<void> {
    await this.instance?.registerFont(fontUrl);
  }

  public registerEventListener(cb: (event: unknown) => void): void {
    this.instance?.registerEventListener(cb);
  }

  public async terminate(): Promise<void> {
    await this.instance?.terminate();
    this.instance = undefined;
  }

  private async handleRequest(request: ApiRequest): Promise<any> {
    assert(this.instance, 'Failed to handle the request: instance terminated.');
    const route = apiPath.test(request.route);
    if (!route) {
      if (apiStartPath.test(request.route)) {
        await this.instance.handleStart();
        return;
      }
      throw new Error('Unknown route');
    }

    if (route.type == 'input') {
      if (route.operation === 'register') {
        return await this.instance.handleRegisterInput(
          route.id,
          request.body as CoreInput.RegisterInputRequest
        );
      } else if (route.operation === 'unregister') {
        return await this.instance.handleUnregisterInput(route.id);
      }
    } else if (route.type === 'output') {
      if (route.operation === 'register') {
        return await this.instance.handleRegisterOutput(
          route.id,
          request.body as CoreOutput.RegisterOutputRequest
        );
      } else if (route.operation === 'unregister') {
        return await this.instance.handleUnregisterOutput(route.id);
      } else if (route.operation === 'update') {
        return await this.instance.handleUpdateOutput(
          route.id,
          request.body as Api.UpdateOutputRequest
        );
      }
    } else if (route.type === 'image') {
      if (route.operation === 'register') {
        return await this.instance.handleRegisterImage(route.id, request.body as ImageSpec);
      } else if (route.operation === 'unregister') {
        return await this.instance.handleUnregisterImage(route.id);
      }
    } else if (route.type === 'shader') {
      throw new Error('Shaders are not supported');
    } else if (route.type === 'web-renderer') {
      throw new Error('Web renderers are not supported');
    }

    throw new Error('Unknown request');
  }
}

class InnerInstance {
  private eventSender: EventSender = new EventSender();
  private worker: AsyncWorker<WorkerMessage, WorkerResponse, WorkerEvent>;
  private logger: Logger;
  private framerate: Framerate;
  private wasmBundleUrl: string;
  private outputs: Record<string, Output> = {};
  private inputs: Record<string, Input> = {};
  private audioMixer: AudioMixer;

  public constructor(options: { framerate: Framerate; wasmBundleUrl: string; logger: Logger }) {
    this.logger = options.logger;
    this.framerate = options.framerate;
    this.wasmBundleUrl = options.wasmBundleUrl;
    this.audioMixer = new AudioMixer(this.logger);

    const worker = new Worker(new URL('../esm/runWorker.mjs', import.meta.url), {
      type: 'module',
    });
    const onEvent = (event: WorkerEvent) => {
      if (EventSender.isExternalEvent(event)) {
        this.eventSender.sendEvent(event);
        return;
      }
      throw new Error(`Unknown event received. ${JSON.stringify(event)}`);
    };
    this.worker = new AsyncWorker(worker, onEvent, this.logger);
  }

  private get ctx(): InstanceContext {
    return {
      logger: this.logger,
      framerate: this.framerate,
      audioMixer: this.audioMixer,
    };
  }

  public async setupInstance(): Promise<void> {
    await this.worker.postMessage({
      type: 'init',
      framerate: this.framerate,
      wasmBundleUrl: this.wasmBundleUrl,
      loggerLevel: this.logger.level,
    });
    await this.audioMixer.init();
    this.logger.debug('WASM instance initialized');
  }

  public async registerFont(fontUrl: string): Promise<void> {
    await this.worker.postMessage({ type: 'registerFont', url: fontUrl });
  }

  public registerEventListener(cb: (event: unknown) => void): void {
    this.eventSender.registerEventCallback(cb);
  }

  public async terminate(): Promise<void> {
    this.logger.debug('Terminate WASM instance.');
    await Promise.all(Object.values(this.outputs).map(output => output.terminate()));
    await Promise.all(Object.values(this.inputs).map(input => input.terminate()));
    await this.worker.postMessage({ type: 'terminate' });
    await this.audioMixer.close();
    this.worker.terminate();
  }

  public async handleStart() {
    await this.worker.postMessage({ type: 'start' });
  }

  public async handleRegisterOutput(
    outputId: string,
    request: CoreOutput.RegisterOutputRequest
  ): Promise<RegisterOutputResponse | undefined> {
    const { output, result, workerMessage } = await handleRegisterOutputRequest(
      this.ctx,
      outputId,
      request
    );
    try {
      await this.worker.postMessage(workerMessage[0], workerMessage[1]);
    } catch (err: any) {
      output.terminate().catch(err => {
        this.logger.warn({ err, outputId }, 'Failed to terminate output');
      });
      throw err;
    }
    if ('initial' in request && request.initial && request.initial.audio?.inputs) {
      this.audioMixer.update(outputId, request.initial.audio.inputs);
    }
    this.outputs[outputId] = output;
    return result;
  }

  public async handleRegisterInput(
    inputId: string,
    request: CoreInput.RegisterInputRequest
  ): Promise<{ video_duration_ms?: number; audio_duration_ms?: number }> {
    const { input, workerMessage } = await handleRegisterInputRequest(this.ctx, inputId, request);
    let result;
    try {
      result = await this.worker.postMessage(workerMessage[0], workerMessage[1]);
    } catch (err: any) {
      input.terminate().catch(err => {
        this.logger.warn({ err, inputId }, 'Failed to terminate input');
      });
      throw err;
    }
    this.inputs[inputId] = input;
    assert(result?.type === 'registerInput');
    return result.body;
  }

  public async handleUnregisterOutput(outputId: string) {
    const output = this.outputs[outputId];
    if (output) {
      delete this.outputs[outputId];
      await output.terminate();
    }
    return await this.worker.postMessage({
      type: 'unregisterOutput',
      outputId,
    });
  }

  public async handleUnregisterInput(inputId: string) {
    const input = this.inputs[inputId];
    if (input) {
      delete this.inputs[inputId];
      await input.terminate();
    }
    return await this.worker.postMessage({
      type: 'unregisterInput',
      inputId: inputId,
    });
  }

  public async handleUpdateOutput(outputId: string, update: Api.UpdateOutputRequest) {
    if (update.audio) {
      this.audioMixer.update(outputId, update.audio.inputs);
    }
    return await this.worker.postMessage({
      type: 'updateScene',
      outputId,
      output: update,
    });
  }

  public async handleRegisterImage(imageId: string, spec: ImageSpec) {
    return await this.worker.postMessage({
      type: 'registerImage',
      imageId,
      image: spec,
    });
  }

  public async handleUnregisterImage(imageId: string) {
    return await this.worker.postMessage({
      type: 'unregisterImage',
      imageId,
    });
  }
}

export default WasmInstance;
