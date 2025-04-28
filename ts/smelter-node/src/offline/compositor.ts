import type {
  Input as CoreInput,
  Output as CoreOutput,
  SmelterManager,
} from '@swmansion/smelter-core';
import { OfflineSmelter as CoreSmelter } from '@swmansion/smelter-core';
import { createLogger } from '../logger';
import LocallySpawnedInstanceManager from '../manager/locallySpawnedInstance';
import type { ReactElement } from 'react';
import type { Renderers } from '@swmansion/smelter';
import fetch from 'node-fetch';
import FormData from 'form-data';

export default class OfflineSmelter {
  private coreSmelter: CoreSmelter;

  public constructor(manager?: SmelterManager) {
    this.coreSmelter = new CoreSmelter(
      manager ?? LocallySpawnedInstanceManager.defaultManager(),
      createLogger()
    );
  }

  public async init(): Promise<void> {
    await this.coreSmelter.init();
  }

  public async render(root: ReactElement, request: CoreOutput.RegisterOutput, durationMs?: number) {
    await this.coreSmelter.render(root, request, durationMs);
  }

  public async registerInput(inputId: string, request: CoreInput.RegisterInput): Promise<object> {
    let result = await this.coreSmelter.registerInput(inputId, request);
    const mappedResult: any = {};

    if ('bearer_token' in result) {
      mappedResult.bearerToken = result['bearer_token'];
    }
    if ('video_duration_ms' in result) {
      mappedResult.videoDurationMs = result['video_duration_ms'];
    }
    if ('audio_duration_ms' in result) {
      mappedResult.audioDurationMs = result['audio_duration_ms'];
    }
    return mappedResult;
  }

  public async registerImage(imageId: string, request: Renderers.RegisterImage): Promise<void> {
    await this.coreSmelter.registerImage(imageId, request);
  }

  public async registerShader(shaderId: string, request: Renderers.RegisterShader): Promise<void> {
    await this.coreSmelter.registerShader(shaderId, request);
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

    return this.coreSmelter.manager.sendMultipartRequest({
      method: 'POST',
      route: `/api/font/register`,
      body: formData,
    });
  }
}
