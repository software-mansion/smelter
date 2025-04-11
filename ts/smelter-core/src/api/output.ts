import type {
  Api,
  RegisterRtpOutput,
  RegisterMp4Output,
  RegisterWhipOutput,
  _smelterInternals,
  RegisterRtmpClientOutput,
} from '@swmansion/smelter';
import { inputRefIntoRawId } from './input';
import { intoRegisterWhipOutput } from './output/whip';
import { intoRegisterRtpOutput } from './output/rtp';
import { intoRegisterMp4Output } from './output/mp4';
import { intoRegisterRtmpClientOutput } from './output/rtmp';

/**
 * It represents HTTP request that can be sent to
 * to compositor, but also additional variants that are specific to WASM like canvas
 */
export type RegisterOutputRequest = Api.RegisterOutput | RegisterWasmSpecificOutputRequest;

export type RegisterWasmSpecificOutputRequest = RegisterWasmSpecificOutput & {
  initial: { video?: Api.Video; audio?: Api.Audio };
};

export type RegisterWasmWhipOutput = {
  type: 'web-wasm-whip';
  iceServers?: Array<{
    credential?: string;
    urls: string | string[];
    username?: string;
  }>;
  endpointUrl: string;
  bearerToken?: string;
  video?: {
    resolution: Api.Resolution;
    maxBitrate?: number;
  };
  audio?: boolean;
};

export type RegisterWasmCanvasOutput = {
  type: 'web-wasm-canvas';
  video?: {
    resolution: Api.Resolution;
    canvas: any; // HTMLCanvasElement
  };
  audio?: boolean;
};

export type RegisterWasmStreamOutput = {
  type: 'web-wasm-stream';
  video?: {
    resolution: Api.Resolution;
  };
  audio?: boolean;
};

export type RegisterWasmSpecificOutput =
  | RegisterWasmWhipOutput
  | RegisterWasmStreamOutput
  | RegisterWasmCanvasOutput;

export type RegisterOutput =
  | ({ type: 'rtp_stream' } & RegisterRtpOutput)
  | ({ type: 'mp4' } & RegisterMp4Output)
  | ({ type: 'whip' } & RegisterWhipOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput)
  | RegisterWasmSpecificOutput;

export function intoRegisterOutput(
  output: RegisterOutput,
  initial: { video?: Api.Video; audio?: Api.Audio }
): RegisterOutputRequest {
  if (!output['video'] && !(output as any)['audio']) {
    throw new Error('Either audio or video field needs to be specified.');
  }
  if (output.type === 'rtp_stream') {
    return intoRegisterRtpOutput(output, initial);
  } else if (output.type === 'mp4') {
    return intoRegisterMp4Output(output, initial);
  } else if (output.type === 'whip') {
    return intoRegisterWhipOutput(output, initial);
  } else if (output.type === 'rtmp_client') {
    return intoRegisterRtmpClientOutput(output, initial);
  } else if (['web-wasm-canvas', 'web-wasm-whip', 'web-wasm-stream'].includes(output.type)) {
    // just pass wasm types as they are, they will not be serialized
    return { ...output, initial };
  } else {
    throw new Error(`Unknown output type ${(output as any).type}`);
  }
}

export function intoAudioInputsConfiguration(inputs: _smelterInternals.AudioConfig): Api.Audio {
  return {
    inputs: inputs.map(input => ({
      input_id: inputRefIntoRawId(input.inputRef),
      volume: input.volume,
    })),
  };
}
