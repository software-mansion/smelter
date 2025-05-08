import { wasm } from './wasm';
import type * as Api from './api';

export type RendererOptions = {
  /**
   * A timeout that defines when the smelter should switch to fallback on the input stream that stopped sending frames.
   */
  streamFallbackTimeoutMs: number;

  loggerLevel?: 'error' | 'warn' | 'info' | 'debug' | 'trace';
};

export type InputFrameSet = {
  ptsMs: number;
  frames: Record<string, InputFrame>;
};

export type OutputFrameSet = {
  ptsMs: number;
  frames: Record<string, OutputFrame>;
};

export type InputFrame = {
  readonly frame: VideoFrame | HTMLVideoElement;
  readonly ptsMs: number;
};

export type OutputFrame = {
  resolution: Api.Resolution;
  data: Uint8ClampedArray;
};

export class Renderer {
  private renderer: wasm.SmelterRenderer;

  private constructor(renderer: wasm.SmelterRenderer) {
    this.renderer = renderer;
  }

  public static async create(options: RendererOptions): Promise<Renderer> {
    const renderer = await wasm.create_renderer({
      stream_fallback_timeout_ms: options.streamFallbackTimeoutMs,
      logger_level: options.loggerLevel ?? 'warn',
      upload_frames_with_copy_external: self.navigator.userAgent.includes('Macintosh'),
    });
    return new Renderer(renderer);
  }

  public async render(input: InputFrameSet): Promise<OutputFrameSet> {
    const frames = Object.entries(input.frames).map(([inputId, value]) => {
      return { inputId, frame: value.frame, ptsMs: value.ptsMs };
    });
    const output = await this.renderer.render({
      ptsMs: input.ptsMs,
      frames,
    });
    return {
      ptsMs: output.ptsMs,
      frames: Object.fromEntries(output.frames.map(({ outputId, ...value }) => [outputId, value])),
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
