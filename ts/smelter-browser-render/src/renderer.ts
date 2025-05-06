import { wasm } from './wasm';
import type * as Api from './api';

export type RendererOptions = {
  /**
   * A timeout that defines when the smelter should switch to fallback on the input stream that stopped sending frames.
   */
  streamFallbackTimeoutMs: number;

  loggerLevel?: 'error' | 'warn' | 'info' | 'debug' | 'trace';

  /**
   * Input frame upload strategy.
   * On most platforms it's more performant to copy input VideoFrame data to CPU and then upload it to texture.
   * But on macOS using dedicated wgpu copy_external_image_to_texture function results in better performance.
   */
  uploadFramesWithCopyExternal?: boolean;
};

export type FrameSet<T> = {
  ptsMs: number;
  frames: { [id: string]: T };
};

export type InputFrame = {
  readonly frame: VideoFrame;
  readonly ptsMs: number;
};

export type OutputFrame = {
  resolution: Api.Resolution;
  format: FrameFormat;
  data: Uint8ClampedArray;
};

export enum FrameFormat {
  RGBA_BYTES = 'RGBA_BYTES',
  YUV_BYTES = 'YUV_BYTES',
}

export class Renderer {
  private renderer: wasm.SmelterRenderer;

  private constructor(renderer: wasm.SmelterRenderer) {
    this.renderer = renderer;
  }

  public static async create(options: RendererOptions): Promise<Renderer> {
    const renderer = await wasm.create_renderer({
      stream_fallback_timeout_ms: options.streamFallbackTimeoutMs,
      logger_level: options.loggerLevel ?? 'warn',
      upload_frames_with_copy_external: options.uploadFramesWithCopyExternal ?? false,
    });
    return new Renderer(renderer);
  }

  public async render(input: FrameSet<InputFrame>): Promise<FrameSet<OutputFrame>> {
    const frames = new Map(Object.entries(input.frames));
    const inputFrameSet = new wasm.FrameSet(input.ptsMs, frames);
    const output = await this.renderer.render(inputFrameSet);
    return {
      ptsMs: output.pts_ms,
      frames: Object.fromEntries(output.frames),
    };
  }

  public async updateScene(
    outputId: Api.OutputId,
    resolution: Api.Resolution,
    scene: Api.Component
  ) {
    await this.renderer.update_scene(outputId, resolution, scene);
  }

  public async registerInput(inputId: Api.InputId) {
    await this.renderer.register_input(inputId);
  }

  public async registerImage(rendererId: Api.RendererId, imageSpec: Api.ImageSpec) {
    await this.renderer.register_image(rendererId, imageSpec);
  }

  public async registerShader(rendererId: Api.RendererId, shaderSpec: Api.ShaderSpec) {
    await this.renderer.register_shader(rendererId, shaderSpec);
  }

  public async registerFont(fontUrl: string) {
    await this.renderer.register_font(fontUrl);
  }

  public async unregisterInput(inputId: Api.InputId) {
    await this.renderer.unregister_input(inputId);
  }

  public async unregisterImage(rendererId: Api.RendererId) {
    await this.renderer.unregister_image(rendererId);
  }

  public async unregisterShader(rendererId: Api.RendererId) {
    await this.renderer.unregister_shader(rendererId);
  }

  public async unregisterOutput(outputId: Api.OutputId) {
    await this.renderer.unregister_output(outputId);
  }
}
