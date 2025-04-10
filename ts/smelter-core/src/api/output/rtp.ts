import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoOutputVideoOptions } from './common';

export function intoRegisterRtpOutput(
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
