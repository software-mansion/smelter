import fetch from 'node-fetch';
import FormData from 'form-data';
import type { ReactElement } from 'react';
import type { SmelterManager } from '@swmansion/smelter-core';
import { OfflineSmelter as CoreSmelter, StateGuard } from '@swmansion/smelter-core';
import type { Renderers } from '@swmansion/smelter';

import type {
  RegisterInput,
  RegisterMp4InputResponse,
  RegisterOutput,
  RegisterWhipServerInputResponse,
} from '../api';
import { createLogger } from '../logger';
import LocallySpawnedInstanceManager from '../manager/locallySpawnedInstance';

export default class OfflineSmelter {
  private coreSmelter: CoreSmelter;
  private scheduler: StateGuard;

  public constructor(manager?: SmelterManager) {
    this.coreSmelter = new CoreSmelter(
      manager ?? LocallySpawnedInstanceManager.defaultManager(),
      createLogger()
    );
    this.scheduler = new StateGuard();
  }

  public async init(): Promise<void> {
    await this.scheduler.runBlocking(async () => {
      await this.coreSmelter.init();
    });
  }

  public async render(root: ReactElement, request: RegisterOutput, durationMs?: number) {
    await this.scheduler.runBlocking(async () => {
      await this.coreSmelter.render(root, request, durationMs);
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

  public async registerImage(imageId: string, request: Renderers.RegisterImage): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerImage(imageId, request);
    });
  }

  public async registerShader(shaderId: string, request: Renderers.RegisterShader): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerShader(shaderId, request);
    });
  }

  public async registerFont(fontSource: string | ArrayBuffer): Promise<object> {
    let fontBuffer: Buffer;

    if (fontSource instanceof ArrayBuffer) {
      fontBuffer = Buffer.from(fontSource);
    } else {
      const response = await fetch(fontSource);
      if (!response.ok) {
        throw new Error(`Failed to fetch the font file from ${fontSource}`);
      }
      fontBuffer = await response.buffer();
    }

    const formData = new FormData();
    formData.append('fontFile', fontBuffer);

    return await this.scheduler.run(async () => {
      return this.coreSmelter.manager.sendMultipartRequest({
        method: 'POST',
        route: `/api/font/register`,
        body: formData,
      });
    });
  }
}
