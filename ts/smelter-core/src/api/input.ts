import type { Api } from '../api';
import type {
  RegisterMp4Input,
  RegisterHlsInput,
  RegisterRtpInput,
  Inputs,
  RegisterWhipServerInput,
  RegisterWhepClientInput,
  RegisterRtmpServerInput,
} from '@swmansion/smelter';
import { _smelterInternals } from '@swmansion/smelter';

/**
 * It represents HTTP request that can be sent to
 * to compositor, but also additional variants that are specific to WASM like camera
 */
export type RegisterInputRequest =
  | RegisterRtpStreamInputRequest
  | RegisterMp4InputRequest
  | RegisterHlsInputRequest
  | RegisterWhipServerInputRequest
  | RegisterWhepClientInputRequest
  | RegisterRtmpServerInputRequest
  | RegisterDecklinkInputRequest
  | { type: 'camera' }
  | { type: 'screen_capture' }
  | { type: 'stream'; stream: any }
  | { type: 'web-wasm-whep'; endpointUrl: string; bearerToken?: string };

export type RegisterRtpStreamInputRequest = Extract<Api.RegisterInput, { type: 'rtp_stream' }>;
export type RegisterMp4InputRequest = { blob?: any } & Extract<Api.RegisterInput, { type: 'mp4' }>;
export type RegisterHlsInputRequest = Extract<Api.RegisterInput, { type: 'hls' }>;
export type RegisterWhipServerInputRequest = Extract<Api.RegisterInput, { type: 'whip_server' }>;
export type RegisterWhepClientInputRequest = Extract<Api.RegisterInput, { type: 'whep_client' }>;
export type RegisterRtmpServerInputRequest = Extract<Api.RegisterInput, { type: 'rtmp_server' }>;
export type RegisterDecklinkInputRequest = Extract<Api.RegisterInput, { type: 'decklink' }>;

export type InputRef = _smelterInternals.InputRef;
export const inputRefIntoRawId = _smelterInternals.inputRefIntoRawId;
export const parseInputRef = _smelterInternals.parseInputRef;

export type RegisterInput =
  | ({ type: 'rtp_stream' } & RegisterRtpInput)
  | ({ type: 'mp4' } & RegisterMp4Input)
  | ({ type: 'hls' } & RegisterHlsInput)
  | ({ type: 'whip_server' } & RegisterWhipServerInput)
  | ({ type: 'whep_client' } & RegisterWhepClientInput)
  | ({ type: 'rtmp_server' } & RegisterRtmpServerInput)
  | { type: 'camera' }
  | { type: 'screen_capture' }
  | { type: 'stream'; stream: any }
  | { type: 'web-wasm-whep'; endpointUrl: string; bearerToken?: string };

/**
 * Converts object passed by user (or modified by platform specific interface) into
 * HTTP request
 */
export function intoRegisterInput(inputId: string, input: RegisterInput): RegisterInputRequest {
  if (input.type === 'mp4') {
    return intoMp4RegisterInput(input);
  } else if (input.type === 'hls') {
    return intoHlsRegisterInput(input);
  } else if (input.type === 'rtp_stream') {
    return intoRtpRegisterInput(input);
  } else if (input.type === 'whip_server') {
    return intoWhipRegisterInput(inputId, input);
  } else if (input.type === 'whep_client') {
    return intoWhepRegisterInput(input);
  } else if (input.type === 'rtmp_server') {
    return intoRtmpRegisterInput(input);
  } else if (input.type === 'camera') {
    return { type: 'camera' };
  } else if (input.type === 'screen_capture') {
    return { type: 'screen_capture' };
  } else if (input.type === 'stream') {
    return { type: 'stream', stream: input.stream };
  } else if (input.type === 'web-wasm-whep') {
    return {
      type: 'web-wasm-whep',
      endpointUrl: input.endpointUrl,
      bearerToken: input.bearerToken,
    };
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
    decoder_map: input.decoderMap,
  };
}

function intoHlsRegisterInput(input: Inputs.RegisterHlsInput): RegisterInputRequest {
  return {
    type: 'hls',
    url: input.url,
    required: input.required,
    offset_ms: input.offsetMs,
    decoder_map: input.decoderMap,
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

function intoWhipRegisterInput(
  inputId: string,
  input: Inputs.RegisterWhipServerInput
): RegisterInputRequest {
  return {
    type: 'whip_server',
    video: input.video && intoInputWhipVideoOptions(input.video),
    bearer_token: input.bearerToken,
    endpoint_override: inputId,
    required: input.required,
    offset_ms: input.offsetMs,
  };
}

function intoWhepRegisterInput(input: Inputs.RegisterWhepClientInput): RegisterInputRequest {
  return {
    type: 'whep_client',
    endpoint_url: input.endpointUrl,
    bearer_token: input.bearerToken,
    video: input.video && intoInputWhepVideoOptions(input.video),
    required: input.required,
    offset_ms: input.offsetMs,
  };
}

function intoRtmpRegisterInput(input: Inputs.RegisterHlsInput): RegisterInputRequest {
  return {
    type: 'rtmp_server',
    url: input.url,
    required: input.required,
    offset_ms: input.offsetMs,
    decoder_map: input.decoderMap,
  };
}

function intoInputAudio(audio: Inputs.InputRtpAudioOptions): Api.InputRtpAudioOptions {
  if (audio.decoder === 'opus') {
    return {
      decoder: 'opus',
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

function intoInputWhipVideoOptions(video: Inputs.InputWhipVideoOptions): Api.InputWhipVideoOptions {
  return {
    decoder_preferences: video.decoderPreferences,
  };
}

function intoInputWhepVideoOptions(video: Inputs.InputWhepVideoOptions): Api.InputWhepVideoOptions {
  return {
    decoder_preferences: video.decoderPreferences,
  };
}
