import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVulkanH264EncoderBitrate } from './common';

export function intoRegisterWhepOutput(
  output: Outputs.RegisterWhepOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'whep_server',
    bearer_token: output.bearerToken,

    video: output.video && initial.video && intoOutputWhepVideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputWhepAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputWhepVideoOptions(
  video: Outputs.WhepVideoOptions | null | undefined,
  initial: Api.VideoScene | undefined
): Api.OutputVideoOptions | undefined {
  if (!video || !initial) {
    return undefined;
  }

  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: video.encoder && intoWhepVideoEncoderOptions(video.encoder),
    initial,
  };
}

export function intoWhepVideoEncoderOptions(
  encoder: Outputs.WhepVideoEncoderOptions
): Api.VideoEncoderOptions {
  switch (encoder.type) {
    case 'ffmpeg_vp9':
      return {
        type: 'ffmpeg_vp9',
        pixel_format: encoder.pixelFormat,
        ffmpeg_options: encoder.ffmpegOptions,
      };
    case 'ffmpeg_vp8':
      return {
        type: 'ffmpeg_vp8',
        ffmpeg_options: encoder.ffmpegOptions,
      };
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

function intoOutputWhepAudioOptions(
  audio: Outputs.WhepAudioOptions,
  initial: Api.AudioScene
): Api.OutputWhepAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoWhepAudioEncoderOptions(audio.encoder),
    channels: audio.channels,
    initial,
  };
}

function intoWhepAudioEncoderOptions(
  encoder: Outputs.WhepAudioEncoderOptions
): Api.WhepAudioEncoderOptions {
  return {
    type: 'opus',
    preset: encoder.preset,
    sample_rate: encoder.sampleRate,
    forward_error_correction: encoder.forwardErrorCorrection,
    expected_packet_loss: encoder.expectedPacketLoss,
  };
}
