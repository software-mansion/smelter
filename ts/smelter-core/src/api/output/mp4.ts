import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVulkanH264EncoderBitrate } from './common';

export function intoRegisterMp4Output(
  output: Outputs.RegisterMp4Output,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'mp4',
    path: output.serverPath,
    video: output.video && initial.video && intoOutputMp4VideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputMp4AudioOptions(output.audio, initial.audio),
    ffmpeg_options: output.ffmpegOptions,
  };
}

export function intoOutputMp4VideoOptions(
  video: Outputs.Mp4VideoOptions,
  initial: Api.VideoScene
): Api.OutputMp4VideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoMp4VideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoMp4VideoEncoderOptions(
  encoder: Outputs.Mp4VideoEncoderOptions
): Api.Mp4VideoEncoderOptions {
  switch (encoder.type) {
    case 'ffmpeg_h264':
      return {
        type: 'ffmpeg_h264',
        preset: encoder.preset,
        pixel_format: encoder.pixelFormat,
        ffmpeg_options: encoder.ffmpegOptions,
      };
    case 'vulkan_h264':
      return {
        type: 'vulkan_h264',
        bitrate: encoder.bitrate && intoVulkanH264EncoderBitrate(encoder.bitrate),
      };
  }
}

function intoOutputMp4AudioOptions(
  audio: Outputs.Mp4AudioOptions,
  initial: Api.AudioScene
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
