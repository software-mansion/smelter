import { loadWasmModule, Renderer } from '@swmansion/smelter-browser-render';
import { Pipeline } from './pipeline';
import { pino } from 'pino';
import type { InitOptions, MainThreadHandle, WorkerMessage, WorkerResponse } from '../workerApi';
import { assert } from '../utils';
import { LoggerLevel } from '@swmansion/smelter-core';
import { registerWorkerEntrypoint } from './bridge/dedicatedWorker';

let instance: Pipeline | undefined;

async function initInstance(handle: MainThreadHandle, options: InitOptions) {
  assert(
    options.wasmBundleUrl,
    'Location of WASM bundle is not defined, call setWasmBundleUrl() first.'
  );
  await loadWasmModule(options.wasmBundleUrl);
  const loggerLevel = (
    Object.values(LoggerLevel).includes(options.loggerLevel as any) ? options.loggerLevel : 'warn'
  ) as LoggerLevel;
  const renderer = await Renderer.create({
    streamFallbackTimeoutMs: 500,
    loggerLevel,
  });
  const logger = pino({ level: options.loggerLevel }).child({ runtime: 'worker' });
  instance = new Pipeline({ renderer, framerate: options.framerate, logger, handle });
}

registerWorkerEntrypoint(
  async (handle: MainThreadHandle, request: WorkerMessage): Promise<WorkerResponse> => {
    if (request.type === 'init') {
      return await initInstance(handle, request);
    }
    assert(instance);
    return await instance.handleRequest(request);
  }
);
