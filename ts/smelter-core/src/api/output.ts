import type {
  Api,
  Outputs,
  RegisterRtpOutput,
  RegisterMp4Output,
  RegisterWhipOutput,
  _smelterInternals,
} from '@swmansion/smelter';
import { inputRefIntoRawId } from './input';

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
  } else if (['web-wasm-canvas', 'web-wasm-whip', 'web-wasm-stream'].includes(output.type)) {
    // just pass wasm types as they are, they will not be serialized
    return { ...output, initial };
  } else {
    throw new Error(`Unknown output type ${(output as any).type}`);
  }
}

function intoRegisterRtpOutput(
  output: Outputs.RegisterRtpOutput,
  initial: { video?: Api.Video; audio?: Api.Audio }
): RegisterOutputRequest {
  return {
    type: 'rtp_stream',
    port: output.port,
    ip: output.ip,
    transport_protocol: output.transportProtocol,
    video: output.video && initial.video && intoOutputVideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputRtpAudioOptions(output.audio, initial.audio),
  };
}

function intoRegisterMp4Output(
  output: Outputs.RegisterMp4Output,
  initial: { video?: Api.Video; audio?: Api.Audio }
): RegisterOutputRequest {
  return {
    type: 'mp4',
    path: output.serverPath,
    video: output.video && initial.video && intoOutputVideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputMp4AudioOptions(output.audio, initial.audio),
  };
}

function intoRegisterWhipOutput(
  output: Outputs.RegisterWhipOutput,
  initial: { video?: Api.Video; audio?: Api.Audio }
): RegisterOutputRequest {
  return {
    type: 'whip',
    endpoint_url: output.endpointUrl,
    bearer_token: output.bearerToken,

    video: output.video && initial.video && intoOutputVideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputWhipAudioOptions(output.audio, initial.audio),
  };
}

function intoOutputVideoOptions(
  video: Outputs.RtpVideoOptions | Outputs.Mp4VideoOptions | Outputs.WhipVideoOptions,
  initial: Api.Video
): Api.OutputVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoVideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoVideoEncoderOptions(
  encoder:
    | Outputs.RtpVideoEncoderOptions
    | Outputs.Mp4VideoEncoderOptions
    | Outputs.WhipVideoEncoderOptions
): Api.VideoEncoderOptions {
  return {
    type: 'ffmpeg_h264',
    preset: encoder.preset,
    ffmpeg_options: encoder.ffmpegOptions,
  };
}

function intoOutputRtpAudioOptions(
  audio: Outputs.RtpAudioOptions,
  initial: Api.Audio
): Api.OutputRtpAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoRtpAudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoOutputMp4AudioOptions(
  audio: Outputs.Mp4AudioOptions,
  initial: Api.Audio
): Api.OutputMp4AudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoMp4AudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoOutputWhipAudioOptions(
  audio: Outputs.WhipAudioOptions,
  initial: Api.Audio
): Api.OutputWhipAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoWhipAudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoRtpAudioEncoderOptions(
  encoder: Outputs.RtpAudioEncoderOptions
): Api.RtpAudioEncoderOptions {
  return {
    type: 'opus',
    preset: encoder.preset,
    channels: encoder.channels,
    sample_rate: encoder.sampleRate,
  };
}

function intoMp4AudioEncoderOptions(
  encoder: Outputs.Mp4AudioEncoderOptions
): Api.Mp4AudioEncoderOptions {
  return {
    type: 'aac',
    channels: encoder.channels,
    sample_rate: encoder.sampleRate,
  };
}

function intoWhipAudioEncoderOptions(
  encoder: Outputs.WhipAudioEncoderOptions
): Api.WhipAudioEncoderOptions {
  return {
    type: 'opus',
    channels: encoder.channels,
    preset: encoder.preset,
    sample_rate: encoder.sampleRate,
  };
}

export function intoAudioInputsConfiguration(inputs: _smelterInternals.AudioConfig): Api.Audio {
  return {
    inputs: inputs.map(input => ({
      input_id: inputRefIntoRawId(input.inputRef),
      volume: input.volume,
    })),
  };
}

function intoOutputEosCondition(condition: Outputs.OutputEndCondition): Api.OutputEndCondition {
  if ('anyOf' in condition) {
    return { any_of: condition.anyOf };
  } else if ('allOf' in condition) {
    return { all_of: condition.allOf };
  } else if ('allInputs' in condition) {
    return { all_inputs: condition.allInputs };
  } else if ('anyInput' in condition) {
    return { any_input: condition.anyInput };
  } else {
    throw new Error('Invalid "send_eos_when" value.');
  }
}
