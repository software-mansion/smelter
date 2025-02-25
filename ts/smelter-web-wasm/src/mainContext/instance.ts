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

  public async sendRequest(request: ApiRequest): Promise<object> {
    return await this.handleRequest(request);
  }

  sendMultipartRequest(_request: MultipartRequest): Promise<object> {
    throw new Error('Method sendMultipartRequest not implemented for web-wasm.');
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

    // WasmInstance might be kept in the memory by some React state
    (this.audioMixer as any) = undefined;
    this.inputs = {};
    this.outputs = {};
  }

  private async handleRequest(request: ApiRequest): Promise<any> {
    const route = apiPath.test(request.route);
    if (!route) {
      if (apiStartPath.test(request.route)) {
        await this.worker.postMessage({ type: 'start' });
        return;
      }
      throw new Error('Unknown route');
    }

    if (route.type == 'input') {
      if (route.operation === 'register') {
        assert(request.body);
        return await this.handleRegisterInput(
          route.id,
          request.body as CoreInput.RegisterInputRequest
        );
      } else if (route.operation === 'unregister') {
        const input = this.inputs[route.id];
        if (input) {
          delete this.inputs[route.id];
          await input.terminate();
        }
        return await this.worker.postMessage({
          type: 'unregisterInput',
          inputId: route.id,
        });
      }
    } else if (route.type === 'output') {
      if (route.operation === 'register') {
        assert(request.body);
        return await this.handleRegisterOutput(
          route.id,
          request.body as CoreOutput.RegisterOutputRequest
        );
      } else if (route.operation === 'unregister') {
        const output = this.outputs[route.id];
        if (output) {
          delete this.outputs[route.id];
          await output.terminate();
        }
        return await this.worker.postMessage({
          type: 'unregisterOutput',
          outputId: route.id,
        });
      } else if (route.operation === 'update') {
        const body = request.body as Api.UpdateOutputRequest;
        if (body.audio) {
          this.audioMixer.update(route.id, body.audio.inputs);
        }
        return await this.worker.postMessage({
          type: 'updateScene',
          outputId: route.id,
          output: body,
        });
      }
    } else if (route.type === 'image') {
      if (route.operation === 'register') {
        assert(request.body);
        return await this.worker.postMessage({
          type: 'registerImage',
          imageId: route.id,
          image: request.body as ImageSpec,
        });
      } else if (route.operation === 'unregister') {
        return await this.worker.postMessage({
          type: 'unregisterImage',
          imageId: route.id,
        });
      }
    } else if (route.type === 'shader') {
      throw new Error('Shaders are not supported');
    } else if (route.type === 'web-renderer') {
      throw new Error('Web renderers are not supported');
    }

    throw new Error('Unknown request');
  }

  private async handleRegisterOutput(
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

  private async handleRegisterInput(
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
}

export default WasmInstance;
