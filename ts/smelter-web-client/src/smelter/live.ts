import type { ReactElement } from 'react';
import { pino } from 'pino';
import type { Renderers } from '@swmansion/smelter';
import { Smelter as CoreSmelter, StateGuard } from '@swmansion/smelter-core';
import type {
  RegisterInput,
  RegisterMp4InputResponse,
  RegisterOutput,
  RegisterWhepServerOutputResponse,
  RegisterWhipServerInputResponse,
} from '../api';
import type { InstanceOptions } from '../manager';
import RemoteInstanceManager from '../manager';

export default class Smelter {
  private coreSmelter: CoreSmelter;
  private scheduler: StateGuard;

  public constructor(opts: InstanceOptions) {
    const logger = pino({
      level: 'warn',
      browser: {
        asObject: true,
        write: {
          debug: console.log,
          trace: console.log,
        },
      },
    });
    this.coreSmelter = new CoreSmelter(new RemoteInstanceManager(opts), logger);
    this.scheduler = new StateGuard();
  }

  public async init(): Promise<void> {
    await this.scheduler.runBlocking(async () => {
      await this.coreSmelter.init();
    });
  }

  public async registerOutput(
    outputId: string,
    root: ReactElement,
    request: Extract<RegisterOutput, { type: 'whep_server' }>
  ): Promise<RegisterWhepOutputResponse>;

  public async registerOutput(
    outputId: string,
    root: ReactElement,
    request: RegisterOutput
  ): Promise<object>;

  public async registerOutput(
    outputId: string,
    root: ReactElement,
    request: RegisterOutput
  ): Promise<object> {
    return await this.scheduler.run(async () => {
      let result = await this.coreSmelter.registerOutput(outputId, root, request);
      if (request.type === 'whep_server') {
        return {
          endpointRoute: result.endpoint_route,
        };
      }
      return result;
    });
  }

  public async unregisterOutput(outputId: string): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.unregisterOutput(outputId);
    });
  }

  public async registerInput(
    inputId: string,
    request: Extract<RegisterInput, { type: 'whip_server' }>
  ): Promise<RegisterWhipInputResponse>;

  public async registerInput(
    inputId: string,
    request: Extract<RegisterInput, { type: 'mp4' }>
  ): Promise<RegisterMp4InputResponse>;

  public async registerInput(inputId: string, request: RegisterInput): Promise<object>;

  public async registerInput(inputId: string, request: RegisterInput): Promise<object> {
    return await this.scheduler.run(async () => {
      let result = await this.coreSmelter.registerInput(inputId, request);
      if (request.type === 'mp4') {
        return {
          videoDurationMs: result.video_duration_ms,
          audioDurationMs: result.audio_duration_ms,
        };
      } else if (request.type === 'whip_server') {
        return {
          bearerToken: result.bearer_token,
          endpointRoute: result.endpoint_route,
        };
      } else {
        return result;
      }
    });
  }

  public async unregisterInput(inputId: string): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.unregisterInput(inputId);
    });
  }

  public async registerImage(imageId: string, request: Renderers.RegisterImage): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerImage(imageId, request);
    });
  }

  public async unregisterImage(imageId: string): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.unregisterImage(imageId);
    });
  }

  public async registerShader(shaderId: string, request: Renderers.RegisterShader): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerShader(shaderId, request);
    });
  }

  public async unregisterShader(shaderId: string): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.unregisterShader(shaderId);
    });
  }

  public async registerWebRenderer(
    instanceId: string,
    request: Renderers.RegisterWebRenderer
  ): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerWebRenderer(instanceId, request);
    });
  }

  public async unregisterWebRenderer(instanceId: string): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.unregisterWebRenderer(instanceId);
    });
  }

  public async registerFont(fontSource: string | ArrayBuffer): Promise<object> {
    let fontBlob: Blob;

    if (fontSource instanceof ArrayBuffer) {
      fontBlob = new Blob([fontSource]);
    } else {
      const response = await fetch(fontSource);
      if (!response.ok) {
        throw new Error(`Failed to fetch the font file from ${fontSource}`);
      }
      fontBlob = await response.blob();
    }

    const formData = new FormData();
    formData.append('fontFile', fontBlob);

    return await this.scheduler.run(async () => {
      return this.coreSmelter.manager.sendMultipartRequest({
        method: 'POST',
        route: `/api/font/register`,
        body: formData,
      });
    });
  }

  public async start(): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.start();
    });
  }

  public async terminate(): Promise<void> {
    await this.scheduler.runBlocking(async () => {
      await this.coreSmelter.terminate();
    });
  }
}
