import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoOutputVideoOptions } from './common';

export function intoRegisterRtmpClientOutput(
  output: Outputs.RegisterRtmpClientOutput,
  initial: { video?: Api.Video; audio?: Api.Audio }
): RegisterOutputRequest {
  return {
    type: 'rtmp_client',
    url: output.url,

    video: output.video && initial.video && intoOutputVideoOptions(output.video, initial.video),
    audio:
      output.audio &&
      initial.audio &&
      intoOutputRtmpClientAudioOptions(output.audio, initial.audio),
  };
}

function intoOutputRtmpClientAudioOptions(
  audio: Outputs.RtmpClientAudioOptions,
  initial: Api.Audio
): Api.OutputRtmpClientAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoRtmpClientAudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoRtmpClientAudioEncoderOptions(
  encoder: Outputs.RtmpClientAudioEncoderOptions
): Api.RtmpClientAudioEncoderOptions {
  return {
    type: 'aac',
    channels: encoder.channels,
    sample_rate: encoder.sampleRate,
  };
}
