import { wasm } from './wasm';
import type * as Api from './api';

export type RendererOptions = {
  /**
   * A timeout that defines when the smelter should switch to fallback on the input stream that stopped sending frames.
   */
  streamFallbackTimeoutMs: number;

  loggerLevel?: 'error' | 'warn' | 'info' | 'debug' | 'trace';
};

export type FrameSet = {
  ptsMs: number;
  frames: { [id: string]: Frame };
};

export type Frame = {
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
    });
    return new Renderer(renderer);
  }

  public render(input: FrameSet): FrameSet {
    const frames = new Map(Object.entries(input.frames));
    const inputFrameSet = new wasm.FrameSet(input.ptsMs, frames);
    const output = this.renderer.render(inputFrameSet);
    return {
      ptsMs: output.pts_ms,
      frames: Object.fromEntries(output.frames),
    };
  }

  public updateScene(outputId: Api.OutputId, resolution: Api.Resolution, scene: Api.Component) {
    this.renderer.update_scene(outputId, resolution, scene);
  }

  public registerInput(inputId: Api.InputId) {
    this.renderer.register_input(inputId);
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

  public unregisterInput(inputId: Api.InputId) {
    this.renderer.unregister_input(inputId);
  }

  public unregisterImage(rendererId: Api.RendererId) {
    this.renderer.unregister_image(rendererId);
  }

  public unregisterShader(rendererId: Api.RendererId) {
    this.renderer.unregister_shader(rendererId);
  }

  public unregisterOutput(outputId: Api.OutputId) {
    this.renderer.unregister_output(outputId);
  }
}
