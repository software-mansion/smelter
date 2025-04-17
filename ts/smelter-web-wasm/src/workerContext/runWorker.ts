import { loadWasmModule, Renderer } from '@swmansion/smelter-browser-render';
import { Pipeline } from './pipeline';
import { pino, type Logger } from 'pino';
import type { InitOptions, WorkerMessage, WorkerResponse } from '../workerApi';
import { registerWorkerEntrypoint } from './bridge';
import { assert } from '../utils';
import { LoggerLevel } from '@swmansion/smelter-core';

let instance: Pipeline | undefined;
let onMessageLogger: Logger = pino({ level: 'warn' });

async function initInstance(options: InitOptions) {
  await loadWasmModule(options.wasmBundleUrl);
  const loggerLevel = (
    Object.values(LoggerLevel).includes(options.loggerLevel as any) ? options.loggerLevel : 'warn'
  ) as LoggerLevel;
  const renderer = await Renderer.create({
    streamFallbackTimeoutMs: 500,
    loggerLevel,
    uploadFramesWithCopyExternal: self.navigator.userAgent.includes("Macintosh"),
  });
  const logger = pino({ level: options.loggerLevel }).child({ runtime: 'worker' });
  onMessageLogger = logger.child({ element: 'onMessage' });
  instance = new Pipeline({ renderer, framerate: options.framerate, logger });
}

registerWorkerEntrypoint<WorkerMessage, WorkerResponse>(
  async (request: WorkerMessage): Promise<WorkerResponse> => {
    if (request.type === 'init') {
      return await initInstance(request);
    }
    assert(instance);
    if (request.type === 'registerInput') {
      return await instance.registerInput(request.inputId, request.input);
    } else if (request.type === 'registerOutput') {
      return instance.registerOutput(request.outputId, request.output);
    } else if (request.type === 'registerImage') {
      return await instance.registerImage(request.imageId, request.image);
    } else if (request.type === 'registerShader') {
      return await instance.registerShader(request.shaderId, request.shader);
    } else if (request.type === 'unregisterInput') {
      return await instance.unregisterInput(request.inputId);
    } else if (request.type === 'unregisterOutput') {
      return await instance.unregisterOutput(request.outputId);
    } else if (request.type === 'unregisterImage') {
      return instance.unregisterImage(request.imageId);
    } else if (request.type === 'unregisterShader') {
      return instance.unregisterShader(request.shaderId);
    } else if (request.type === 'updateScene') {
      return instance.updateScene(request.outputId, request.output);
    } else if (request.type === 'registerFont') {
      return instance.registerFont(request.url);
    } else if (request.type === 'start') {
      return instance.start();
    } else if (request.type === 'terminate') {
      return await instance.terminate();
    } else {
      onMessageLogger.warn(request, 'Web worker received unknown message.');
    }
  }
);
