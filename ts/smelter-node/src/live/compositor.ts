import type { ReactElement } from 'react';
import FormData from 'form-data';
import fetch from 'node-fetch';
import type { Renderers } from '@swmansion/smelter';
import type { SmelterManager } from '@swmansion/smelter-core';
import { StateGuard, Smelter as CoreSmelter } from '@swmansion/smelter-core';

import LocallySpawnedInstance from '../manager/locallySpawnedInstance';
import { createLogger } from '../logger';
import type { RegisterInput, RegisterOutput } from '../api';

export default class Smelter {
  private coreSmelter: CoreSmelter;
  private scheduler: StateGuard;

  public constructor(manager?: SmelterManager) {
    this.coreSmelter = new CoreSmelter(
      manager ?? LocallySpawnedInstance.defaultManager(),
      createLogger()
    );
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
    request: RegisterOutput
  ): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerOutput(outputId, root, request);
    });
  }

  public async unregisterOutput(outputId: string): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.unregisterOutput(outputId);
    });
  }

  public async registerInput(inputId: string, request: RegisterInput): Promise<void> {
    await this.scheduler.run(async () => {
      await this.coreSmelter.registerInput(inputId, request);
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
