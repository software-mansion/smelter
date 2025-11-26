import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVulkanH264EncoderBitrate } from './common';

export function intoRegisterRtmpClientOutput(
  output: Outputs.RegisterRtmpClientOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'rtmp_client',
    url: output.url,

    video:
      output.video &&
      initial.video &&
      intoOutputRtmpClientVideoOptions(output.video, initial.video),
    audio:
      output.audio &&
      initial.audio &&
      intoOutputRtmpClientAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputRtmpClientVideoOptions(
  video: Outputs.RtmpClientVideoOptions,
  initial: Api.VideoScene
): Api.OutputRtmpClientVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoRtmpClientVideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoRtmpClientVideoEncoderOptions(
  encoder: Outputs.RtmpClientVideoEncoderOptions
): Api.RtmpClientVideoEncoderOptions {
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

function intoOutputRtmpClientAudioOptions(
  audio: Outputs.RtmpClientAudioOptions,
  initial: Api.AudioScene
): Api.OutputRtmpClientAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    channels: audio.channels,
    encoder: intoRtmpClientAudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoRtmpClientAudioEncoderOptions(
  encoder: Outputs.RtmpClientAudioEncoderOptions
): Api.RtmpClientAudioEncoderOptions {
  return {
    type: 'aac',
    sample_rate: encoder.sampleRate,
  };
}
