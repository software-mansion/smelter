import { Smelter as CoreSmelter, StateGuard } from '@swmansion/smelter-core';
import type { ReactElement } from 'react';
import type { Logger } from 'pino';
import { pino } from 'pino';
import { assert } from '../utils';
import {
  type RegisterOutput,
  type RegisterInput,
  type RegisterImage,
  type RegisterShader,
  intoRegisterOutputRequest,
} from './api';
import WasmInstance from '../mainContext/instance';
import type { RegisterOutputResponse } from '../mainContext/output';

export type SmelterOptions = {
  framerate?: Framerate | number;
};

export type Framerate = {
  num: number;
  den: number;
};

let wasmBundleUrl: string | undefined;

/*
 * Defines url where WASM bundle is hosted. This method needs to be called before
 * first Smelter instance is initiated.
 */
export function setWasmBundleUrl(url: string) {
  wasmBundleUrl = url;
}

export default class Smelter {
  private coreSmelter?: CoreSmelter;
  private scheduler: StateGuard;
  private instance?: WasmInstance;
  private options: SmelterOptions;
  private logger: Logger = pino({
    level: 'warn',
    browser: {
      asObject: true,
      write: {
        debug: console.log,
        trace: console.log,
      },
    },
  });

  public constructor(options?: SmelterOptions) {
    this.options = options ?? {};
    this.scheduler = new StateGuard();
  }

  /*
   * Initializes Smelter instance. It needs to be called before any resource is registered.
   * Outputs won't produce any results until `start()` is called.
   */
  public async init(): Promise<void> {
    await this.scheduler.runBlocking(async () => {
      assert(
        wasmBundleUrl,
        'Location of WASM bundle is not defined, call setWasmBundleUrl() first.'
      );
      this.instance = new WasmInstance({
        framerate: resolveFramerate(this.options.framerate),
        wasmBundleUrl,
        logger: this.logger.child({ element: 'wasmInstance' }),
      });
      this.coreSmelter = new CoreSmelter(this.instance, this.logger);

      await this.coreSmelter!.init();
    });
  }

  public async registerOutput(
    outputId: string,
    root: ReactElement,
    request: RegisterOutput
  ): Promise<{ stream?: MediaStream }> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      const response = (await this.coreSmelter.registerOutput(
        outputId,
        root,
        intoRegisterOutputRequest(request)
      )) as RegisterOutputResponse | undefined;
      if (response?.type === 'web-wasm-stream' || response?.type === 'web-wasm-whip') {
        return { stream: response.stream };
      } else {
        return {};
      }
    });
  }

  public async unregisterOutput(outputId: string): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.unregisterOutput(outputId);
    });
  }

  public async registerInput(inputId: string, request: RegisterInput): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.registerInput(inputId, request);
    });
  }

  public async unregisterInput(inputId: string): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.unregisterInput(inputId);
    });
  }

  public async registerImage(imageId: string, request: RegisterImage): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.registerImage(imageId, request);
    });
  }

  public async unregisterImage(imageId: string): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.unregisterImage(imageId);
    });
  }

  public async registerShader(shaderId: string, shaderSpec: RegisterShader): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.registerShader(shaderId, shaderSpec);
    });
  }

  public async unregisterShader(shaderId: string): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.coreSmelter);
      await this.coreSmelter.unregisterShader(shaderId);
    });
  }

  public async registerFont(fontUrl: string): Promise<void> {
    return await this.scheduler.run(async () => {
      assert(this.instance);
      await this.instance.registerFont(new URL(fontUrl, import.meta.url).toString());
    });
  }

  /**
   * Starts processing pipeline. Any previously registered output will start producing video data.
   */
  public async start(): Promise<void> {
    return await this.scheduler.run(async () => {
      await this.coreSmelter!.start();
    });
  }

  /**
   * Stops processing pipeline.
   */
  public async terminate(): Promise<void> {
    return await this.scheduler.runBlocking(async () => {
      await this.coreSmelter?.terminate();
    });
  }
}

function resolveFramerate(framerate?: number | Framerate): Framerate {
  if (!framerate) {
    return { num: 30, den: 1 };
  } else if (typeof framerate === 'number') {
    return { num: framerate, den: 1 };
  } else {
    return framerate;
  }
}
