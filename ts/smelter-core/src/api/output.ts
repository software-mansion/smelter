import type {
  Api,
  RegisterRtpOutput,
  RegisterMp4Output,
  RegisterHlsOutput,
  RegisterWhipClientOutput,
  RegisterWhepServerOutput,
  RegisterRtmpClientOutput,
  _smelterInternals,
} from '@swmansion/smelter';
import { inputRefIntoRawId } from './input';
import { intoRegisterWhipClientOutput } from './output/whip';
import { intoRegisterWhepServerOutput } from './output/whep';
import { intoRegisterRtpOutput } from './output/rtp';
import { intoRegisterMp4Output } from './output/mp4';
import { intoRegisterHlsOutput } from './output/hls';
import { intoRegisterRtmpClientOutput } from './output/rtmp';

/**
 * It represents HTTP request that can be sent to
 * to compositor, but also additional variants that are specific to WASM like canvas
 */
export type RegisterOutputRequest = Api.RegisterOutput | RegisterWasmSpecificOutputRequest;

export type RegisterWasmSpecificOutputRequest = RegisterWasmSpecificOutput & {
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene };
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
  | ({ type: 'hls' } & RegisterHlsOutput)
  | ({ type: 'whip_client' } & RegisterWhipClientOutput)
  | ({ type: 'whep_server' } & RegisterWhepServerOutput)
  | ({ type: 'rtmp_client' } & RegisterRtmpClientOutput)
  | RegisterWasmSpecificOutput;

export function intoRegisterOutput(
  output: RegisterOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  if (!output['video'] && !(output as any)['audio']) {
    throw new Error('Either audio or video field needs to be specified.');
  }
  if (output.type === 'rtp_stream') {
    return intoRegisterRtpOutput(output, initial);
  } else if (output.type === 'mp4') {
    return intoRegisterMp4Output(output, initial);
  } else if (output.type === 'hls') {
    return intoRegisterHlsOutput(output, initial);
  } else if (output.type === 'whip_client') {
    return intoRegisterWhipClientOutput(output, initial);
  } else if (output.type === 'whep_server') {
    return intoRegisterWhepServerOutput(output, initial);
  } else if (output.type === 'rtmp_client') {
    return intoRegisterRtmpClientOutput(output, initial);
  } else if (['web-wasm-canvas', 'web-wasm-whip', 'web-wasm-stream'].includes(output.type)) {
    // just pass wasm types as they are, they will not be serialized
    return { ...output, initial };
  } else {
    throw new Error(`Unknown output type ${(output as any).type}`);
  }
}

export function intoAudioInputsConfiguration(
  inputs: _smelterInternals.AudioConfig
): Api.AudioScene {
  return {
    inputs: inputs.map(input => ({
      input_id: inputRefIntoRawId(input.inputRef),
      volume: input.volume,
    })),
  };
}
