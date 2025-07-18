import type { _smelterInternals, Api } from '@swmansion/smelter';
import type { ImageSpec, Resolution, ShaderSpec } from '@swmansion/smelter-browser-render';
import type { Framerate } from './compositor/compositor';

export type RegisterInput =
  | {
      type: 'mp4';
      arrayBuffer: ArrayBuffer;
      audioWorkletMessagePort?: MessagePort;
    }
  | {
      type: 'stream';
      videoStream?: ReadableStream;
    }
  | {
      type: 'whep';
      endpointUrl: string;
      bearerToken: string;
    };

export type RegisterOutput = {
  type: 'stream';
  video?: {
    canvas: OffscreenCanvas;
    resolution: Resolution;
    initial: Api.VideoScene;
  };
};

export type InitOptions = {
  framerate: Framerate;
  wasmBundleUrl: string;
  loggerLevel: string;
};

export type WorkerMessage =
  | ({ type: 'init' } & InitOptions)
  | {
      type: 'start';
    }
  | {
      type: 'registerInput';
      inputId: string;
      input: RegisterInput;
    }
  | {
      type: 'unregisterInput';
      inputId: string;
    }
  | {
      type: 'registerOutput';
      outputId: string;
      output: RegisterOutput;
    }
  | {
      type: 'updateScene';
      outputId: string;
      output: Api.UpdateOutputRequest;
    }
  | {
      type: 'unregisterOutput';
      outputId: string;
    }
  | {
      type: 'registerImage';
      imageId: string;
      image: ImageSpec;
    }
  | {
      type: 'unregisterImage';
      imageId: string;
    }
  | {
      type: 'registerShader';
      shaderId: string;
      shader: ShaderSpec;
    }
  | {
      type: 'unregisterShader';
      shaderId: string;
    }
  | {
      type: 'registerFont';
      url: string;
    }
  | {
      type: 'terminate';
    };

export type WorkerResponse = void | {
  type: 'registerInput';
  body: { video_duration_ms?: number; audio_duration_ms?: number };
};

export type WorkerEvent =
  | {
      type:
        | _smelterInternals.SmelterEventType.AUDIO_INPUT_DELIVERED
        | _smelterInternals.SmelterEventType.VIDEO_INPUT_DELIVERED
        | _smelterInternals.SmelterEventType.AUDIO_INPUT_PLAYING
        | _smelterInternals.SmelterEventType.VIDEO_INPUT_PLAYING
        | _smelterInternals.SmelterEventType.AUDIO_INPUT_EOS
        | _smelterInternals.SmelterEventType.VIDEO_INPUT_EOS;
      inputId: string;
    }
  | {
      type: _smelterInternals.SmelterEventType.OUTPUT_DONE;
      outputId: string;
    };
