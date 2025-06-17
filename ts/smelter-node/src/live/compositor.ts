import type { ReactElement } from 'react';
import FormData from 'form-data';
import fetch from 'node-fetch';
import type { Renderers } from '@swmansion/smelter';
import type { SmelterManager } from '@swmansion/smelter-core';
import { Smelter as CoreSmelter } from '@swmansion/smelter-core';

import LocallySpawnedInstance from '../manager/locallySpawnedInstance';
import { createLogger } from '../logger';
import type { RegisterInput, RegisterOutput } from '../api';

export default class Smelter {
  private coreSmelter: CoreSmelter;

  public constructor(manager?: SmelterManager) {
    this.coreSmelter = new CoreSmelter(
      manager ?? LocallySpawnedInstance.defaultManager(),
      createLogger()
    );
  }

  public async init(): Promise<void> {
    await this.coreSmelter.init();
  }

  public async registerOutput(
    outputId: string,
    root: ReactElement,
    request: RegisterOutput
  ): Promise<void> {
    await this.coreSmelter.registerOutput(outputId, root, request);
  }

  public async unregisterOutput(outputId: string): Promise<void> {
    await this.coreSmelter.unregisterOutput(outputId);
  }

  public async registerInput(inputId: string, request: RegisterInput): Promise<any> {
    let result = await this.coreSmelter.registerInput(inputId, request);
    const mappedResult: any = {};

    if ('bearer_token' in result) {
      mappedResult.bearerToken = result['bearer_token'];
      mappedResult.endpointRoute = `/whip/${encodeURIComponent(`global:${inputId}`)}`;
    }
    if ('video_duration_ms' in result) {
      mappedResult.videoDurationMs = result['video_duration_ms'];
    }
    if ('audio_duration_ms' in result) {
      mappedResult.audioDurationMs = result['audio_duration_ms'];
    }
    return mappedResult;
  }

  public async unregisterInput(inputId: string): Promise<void> {
    await this.coreSmelter.unregisterInput(inputId);
  }

  public async registerImage(imageId: string, request: Renderers.RegisterImage): Promise<void> {
    await this.coreSmelter.registerImage(imageId, request);
  }

  public async unregisterImage(imageId: string): Promise<void> {
    await this.coreSmelter.unregisterImage(imageId);
  }

  public async registerShader(shaderId: string, request: Renderers.RegisterShader): Promise<void> {
    await this.coreSmelter.registerShader(shaderId, request);
  }

  public async unregisterShader(shaderId: string): Promise<void> {
    await this.coreSmelter.unregisterShader(shaderId);
  }

  public async registerWebRenderer(
    instanceId: string,
    request: Renderers.RegisterWebRenderer
  ): Promise<void> {
    await this.coreSmelter.registerWebRenderer(instanceId, request);
  }

  public async unregisterWebRenderer(instanceId: string): Promise<void> {
    await this.coreSmelter.unregisterWebRenderer(instanceId);
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

  public async start(): Promise<void> {
    await this.coreSmelter.start();
  }

  public async terminate(): Promise<void> {
    await this.coreSmelter.terminate();
  }
}
