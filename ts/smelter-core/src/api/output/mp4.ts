import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoOutputVideoOptions } from './common';

export function intoRegisterMp4Output(
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

function intoMp4AudioEncoderOptions(
  encoder: Outputs.Mp4AudioEncoderOptions
): Api.Mp4AudioEncoderOptions {
  return {
    type: 'aac',
    channels: encoder.channels,
    sample_rate: encoder.sampleRate,
  };
}
