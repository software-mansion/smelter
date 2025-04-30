import type { Logger } from 'pino';
import { Pipeline } from '../pipeline';
import type {
  InitOptions,
  MainThreadHandle,
  WorkerEvent,
  WorkerHandle,
  WorkerMessage,
  WorkerResponse,
} from '../../workerApi';
import { loadWasmModule, Renderer } from '@swmansion/smelter-browser-render';
import { LoggerLevel } from '@swmansion/smelter-core';
import { assert } from '../../utils';

export class PassThroughWorker implements WorkerHandle {
  private logger: Logger;
  private instance: Pipeline | undefined;
  private handle: MainThreadHandle;

  constructor(onEvent: (event: WorkerEvent) => void, logger: Logger) {
    this.logger = logger;
    this.handle = new Handle(onEvent);
  }

  private async initInstance(options: InitOptions): Promise<void> {
    await loadWasmModule(options.wasmBundleUrl);
    const loggerLevel = (
      Object.values(LoggerLevel).includes(options.loggerLevel as any) ? options.loggerLevel : 'warn'
    ) as LoggerLevel;
    const renderer = await Renderer.create({
      streamFallbackTimeoutMs: 500,
      loggerLevel,
      uploadFramesWithCopyExternal: self.navigator.userAgent.includes('Macintosh'),
    });
    this.instance = new Pipeline({
      renderer,
      framerate: options.framerate,
      logger: this.logger,
      handle: this.handle,
    });
  }

  public async postMessage(
    request: WorkerMessage,
    _transferable?: Transferable[]
  ): Promise<WorkerResponse> {
    if (request.type === 'init') {
      return await this.initInstance(request);
    }
    assert(this.instance);
    return await this.instance.handleRequest(request);
  }

  public terminate() {}
}

class Handle implements MainThreadHandle {
  private onEvent: (event: WorkerEvent) => void;
  constructor(onEvent: (event: WorkerEvent) => void) {
    this.onEvent = onEvent;
  }

  public async postEvent(event: WorkerEvent) {
    this.onEvent(event);
  }
}
