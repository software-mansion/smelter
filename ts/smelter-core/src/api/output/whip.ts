import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoOutputVideoOptions } from './common';

export function intoRegisterWhipOutput(
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
