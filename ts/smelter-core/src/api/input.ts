import type { Api } from '../api';
import type {
  RegisterMp4Input,
  RegisterRtpInput,
  Inputs,
  RegisterWhipInput,
} from '@swmansion/smelter';
import { _smelterInternals } from '@swmansion/smelter';

/**
 * It represents HTTP request that can be sent to
 * to compositor, but also additional variants that are specific to WASM like camera
 */
export type RegisterInputRequest =
  | RegisterRtpStreamInputRequest
  | RegisterMp4InputRequest
  | RegisterWhipInputRequest
  | RegisterDecklinkInputRequest
  | { type: 'camera' }
  | { type: 'screen_capture' }
  | { type: 'stream'; stream: any };

export type RegisterRtpStreamInputRequest = Extract<Api.RegisterInput, { type: 'rtp_stream' }>;
export type RegisterMp4InputRequest = { blob?: any } & Extract<Api.RegisterInput, { type: 'mp4' }>;
export type RegisterWhipInputRequest = Extract<Api.RegisterInput, { type: 'whip' }>;
export type RegisterDecklinkInputRequest = Extract<Api.RegisterInput, { type: 'decklink' }>;

export type InputRef = _smelterInternals.InputRef;
export const inputRefIntoRawId = _smelterInternals.inputRefIntoRawId;
export const parseInputRef = _smelterInternals.parseInputRef;

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'whip' } & RegisterWhipInput)
  | { type: 'camera' }
  | { type: 'screen_capture' }
  | { type: 'stream'; stream: any };

/**
 * Converts object passed by user (or modified by platform specific interface) into
 * HTTP request
 */
export function intoRegisterInput(input: RegisterInput): RegisterInputRequest {
  if (input.type === 'mp4') {
    return intoMp4RegisterInput(input);
  } else if (input.type === 'rtp_stream') {
    return intoRtpRegisterInput(input);
  } else if (input.type === 'whip') {
    return intoWhipRegisterInput(input);
  } else if (input.type === 'camera') {
    return { type: 'camera' };
  } else if (input.type === 'screen_capture') {
    return { type: 'screen_capture' };
  } else if (input.type === 'stream') {
    return { type: 'stream', stream: input.stream };
  } else {
    throw new Error(`Unknown input type ${(input as any).type}`);
  }
}

function intoMp4RegisterInput(input: Inputs.RegisterMp4Input): RegisterInputRequest {
  return {
    type: 'mp4',
    url: input.url,
    path: input.serverPath,
    blob: input.blob,
    loop: input.loop,
    required: input.required,
    offset_ms: input.offsetMs,
    video_decoder: input.videoDecoder,
  };
}

function intoRtpRegisterInput(input: Inputs.RegisterRtpInput): RegisterInputRequest {
  return {
    type: 'rtp_stream',
    port: input.port,
    transport_protocol: input.transportProtocol,
    video: input.video,
    audio: input.audio && intoInputAudio(input.audio),
    required: input.required,
    offset_ms: input.offsetMs,
  };
}

function intoWhipRegisterInput(input: Inputs.RegisterWhipInput): RegisterInputRequest {
  return {
    type: 'whip',
    video: input.video,
    audio: input.audio && intoInputWhipAudioOptions(input.audio),
    required: input.required,
    offset_ms: input.offsetMs,
  };
}

function intoInputWhipAudioOptions(input: Inputs.InputWhipAudioOptions): Api.InputWhipAudioOptions {
  return {
    decoder: 'opus',
    forward_error_correction: input.forwardErrorCorrection,
  };
}

function intoInputAudio(audio: Inputs.InputRtpAudioOptions): Api.InputRtpAudioOptions {
  if (audio.decoder === 'opus') {
    return {
      decoder: 'opus',
      forward_error_correction: audio.forwardErrorCorrection,
    };
  } else if (audio.decoder === 'aac') {
    return {
      decoder: 'aac',
      audio_specific_config: audio.audioSpecificConfig,
      rtp_mode: audio.rtpMode,
    };
  } else {
    throw new Error(`Unknown audio decoder type: ${(audio as any).decoder}`);
  }
}
