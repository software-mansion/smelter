import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition } from './common';

export function intoRegisterMp4Output(
  output: Outputs.RegisterMp4Output,
  initial: { video?: Api.Video; audio?: Api.Audio }
): RegisterOutputRequest {
  return {
    type: 'mp4',
    path: output.serverPath,
    video: output.video && initial.video && intoOutputMp4VideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputMp4AudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputMp4VideoOptions(
  video: Outputs.Mp4VideoOptions,
  initial: Api.Video
): Api.OutputVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoMp4VideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoMp4VideoEncoderOptions(
  encoder: Outputs.Mp4VideoEncoderOptions
): Api.VideoEncoderOptions {
  return {
    type: 'ffmpeg_h264',
    preset: encoder.preset,
    ffmpeg_options: encoder.ffmpegOptions,
  };
}

function intoOutputMp4AudioOptions(
  audio: Outputs.Mp4AudioOptions,
  initial: Api.Audio
): Api.OutputMp4AudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    channels: audio.channels,
    encoder: intoMp4AudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoMp4AudioEncoderOptions(
  encoder: Outputs.Mp4AudioEncoderOptions
): Api.Mp4AudioEncoderOptions {
  return {
    type: 'aac',
    sample_rate: encoder.sampleRate,
  };
}
