import type { Renderers } from '@swmansion/smelter';
import { _smelterInternals } from '@swmansion/smelter';
import type { RegisterInputResponse, RegisterOutputResponse } from '../api';
import { ApiClient } from '../api';
import Output from './output';
import type { SmelterManager } from '../smelterManager';
import type { RegisterOutput } from '../api/output';
import { intoRegisterOutput } from '../api/output';
import type { RegisterInput } from '../api/input';
import { intoRegisterInput } from '../api/input';
import { parseEvent } from '../event';
import { intoRegisterImage, intoRegisterWebRenderer } from '../api/renderer';
import { handleEvent } from './event';
import type { ReactElement } from 'react';
import type { Logger } from 'pino';
import type { ImageRef } from '../api/image';

export class Smelter {
  public readonly manager: SmelterManager;
  private api: ApiClient;
  private store: _smelterInternals.LiveInputStreamStore<string>;
  private outputs: Record<string, Output> = {};
  private startTime?: number;
  private logger: Logger;

  public constructor(manager: SmelterManager, logger: Logger) {
    this.manager = manager;
    this.api = new ApiClient(this.manager);
    this.store = new _smelterInternals.LiveInputStreamStore(logger);
    this.logger = logger;
  }

  public async init(): Promise<void> {
    this.manager.registerEventListener((event: unknown) => this.handleEvent(event));
    await this.manager.setupInstance({
      aheadOfTimeProcessing: false,
      logger: this.logger.child({ element: 'connection-manager' }),
    });
  }

  public async registerOutput(
    outputId: string,
    root: ReactElement,
    request: RegisterOutput
  ): Promise<RegisterOutputResponse> {
    this.logger.info({ outputId, type: request.type }, 'Register new output');
    const output = new Output(
      outputId,
      root,
      request,
      this.api,
      this.store,
      this.startTime,
      this.logger
    );

    const apiRequest = intoRegisterOutput(request, output.scene());
    const result = await this.api.registerOutput(outputId, apiRequest);
    if (request.type === 'whep') {
      result.endpoint_route = `/whep/${encodeURIComponent(outputId)}`;
    }
    this.outputs[outputId] = output;
    await output.ready();
    return result;
  }

  public async unregisterOutput(outputId: string): Promise<object> {
    this.logger.info({ outputId }, 'Unregister output');
    await this.outputs[outputId].close();
    delete this.outputs[outputId];
    // TODO: wait for event
    return this.api.unregisterOutput(outputId, {});
  }

  public async registerInput(
    inputId: string,
    request: RegisterInput
  ): Promise<RegisterInputResponse> {
    this.logger.info({ inputId, type: request.type }, 'Register new input');
    return this.store.runBlocking(async updateStore => {
      const inputRef = { type: 'global', id: inputId } as const;
      const result = await this.api.registerInput(inputRef, intoRegisterInput(inputId, request));
      if (request.type === 'whip') {
        result.endpoint_route = `/whip/${encodeURIComponent(inputId)}`;
      }

      updateStore({
        type: 'add_input',
        input: {
          inputId,
          videoDurationMs: result.video_duration_ms,
          audioDurationMs: result.audio_duration_ms,
        },
      });

      return result;
    });
  }

  public async unregisterInput(inputId: string): Promise<object> {
    this.logger.info({ inputId }, 'Unregister input');
    return this.store.runBlocking(async updateStore => {
      const inputRef = { type: 'global', id: inputId } as const;
      const result = this.api.unregisterInput(inputRef, {});
      updateStore({ type: 'remove_input', inputId });
      return result;
    });
  }

  public async registerShader(
    shaderId: string,
    request: Renderers.RegisterShader
  ): Promise<object> {
    this.logger.info({ shaderId }, 'Register shader');
    return this.api.registerShader(shaderId, request);
  }

  public async unregisterShader(shaderId: string): Promise<object> {
    this.logger.info({ shaderId }, 'Unregister shader');
    return this.api.unregisterShader(shaderId);
  }

  public async registerImage(imageId: string, request: Renderers.RegisterImage): Promise<object> {
    this.logger.info({ imageId }, 'Register image');
    const imageRef = { type: 'global', id: imageId } as const satisfies ImageRef;

    return this.api.registerImage(imageRef, intoRegisterImage(request));
  }

  public async unregisterImage(imageId: string): Promise<object> {
    this.logger.info({ imageId }, 'Unregister image');
    const imageRef = { type: 'global', id: imageId } as const satisfies ImageRef;

    return this.api.unregisterImage(imageRef, {});
  }

  public async registerWebRenderer(
    instanceId: string,
    request: Renderers.RegisterWebRenderer
  ): Promise<object> {
    this.logger.info({ instanceId }, 'Register web renderer');
    return this.api.registerWebRenderer(instanceId, intoRegisterWebRenderer(request));
  }

  public async unregisterWebRenderer(instanceId: string): Promise<object> {
    this.logger.info({ instanceId }, 'Unregister web renderer');
    return this.api.unregisterWebRenderer(instanceId);
  }

  public async start(): Promise<void> {
    this.logger.info('Start smelter instance.');
    const startTime = Date.now();
    await this.api.start();
    Object.values(this.outputs).forEach(output => {
      output.initClock(startTime);
    });
    this.startTime = startTime;
  }

  public async terminate(): Promise<void> {
    for (const output of Object.values(this.outputs)) {
      await output.close();
    }
    await this.manager.terminate();
  }

  private handleEvent(rawEvent: unknown) {
    const event = parseEvent(rawEvent, this.logger);
    if (!event) {
      return;
    }
    this.logger.debug({ event }, 'New event received');
    handleEvent(this.store, this.outputs, event);
  }
}
