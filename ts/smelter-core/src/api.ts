import { Api } from '@swmansion/smelter';
import type { SmelterManager } from './smelterManager';
import type { RegisterOutputRequest } from './api/output';
import { inputRefIntoRawId, type InputRef, type RegisterInputRequest } from './api/input';
import { imageRefIntoRawId, type ImageRef } from './api/image';

export { Api };

export type ApiRequest = {
  method: 'GET' | 'POST';
  route: string;
  body?: object;
};

export type MultipartRequest = {
  method: 'POST';
  route: string;
  body: any; //FormData
};

export type RegisterInputResponse = {
  endpoint_route?: string;
  bearer_token?: string;
  video_duration_ms?: number;
  audio_duration_ms?: number;
};

export class ApiClient {
  private serverManager: SmelterManager;

  constructor(serverManager: SmelterManager) {
    this.serverManager = serverManager;
  }

  public async updateScene(outputId: string, request: Api.UpdateOutputRequest): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/output/${encodeURIComponent(outputId)}/update`,
      body: request,
    });
  }

  public async registerOutput(outputId: string, request: RegisterOutputRequest): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/output/${encodeURIComponent(outputId)}/register`,
      body: request,
    });
  }

  public async unregisterOutput(
    outputId: string,
    body: { schedule_time_ms?: number }
  ): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/output/${encodeURIComponent(outputId)}/unregister`,
      body,
    });
  }

  public async registerInput(
    inputId: InputRef,
    request: RegisterInputRequest
  ): Promise<RegisterInputResponse> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/input/${encodeURIComponent(inputRefIntoRawId(inputId))}/register`,
      body: request,
    });
  }

  public async unregisterInput(
    inputId: InputRef,
    body: { schedule_time_ms?: number }
  ): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/input/${encodeURIComponent(inputRefIntoRawId(inputId))}/unregister`,
      body,
    });
  }

  public async registerShader(shaderId: string, request: Api.ShaderSpec): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/shader/${encodeURIComponent(shaderId)}/register`,
      body: request,
    });
  }

  public async unregisterShader(shaderId: string): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/shader/${encodeURIComponent(shaderId)}/unregister`,
      body: {},
    });
  }

  public async registerImage(imageRef: ImageRef, request: Api.ImageSpec): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/image/${encodeURIComponent(imageRefIntoRawId(imageRef))}/register`,
      body: request,
    });
  }

  public async unregisterImage(
    imageRef: ImageRef,
    body: { schedule_time_ms?: number }
  ): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/image/${encodeURIComponent(imageRefIntoRawId(imageRef))}/unregister`,
      body,
    });
  }

  public async registerWebRenderer(
    instanceId: string,
    request: Api.WebRendererSpec
  ): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/web-renderer/${encodeURIComponent(instanceId)}/register`,
      body: request,
    });
  }

  public async unregisterWebRenderer(instanceId: string): Promise<object> {
    return this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/web-renderer/${encodeURIComponent(instanceId)}/unregister`,
      body: {},
    });
  }

  public async start(): Promise<void> {
    await this.serverManager.sendRequest({
      method: 'POST',
      route: `/api/start`,
      body: {},
    });
  }
}
