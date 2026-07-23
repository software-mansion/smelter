import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVideoEncoderBitrate } from './common';

export function intoRegisterMoqClientOutput(
  output: Outputs.RegisterMoqClientOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'moq_client',
    endpoint_url: output.endpointUrl,
    broadcast_path: output.broadcastPath,
    container: output.container,

    video:
      output.video && initial.video && intoOutputMoqClientVideoOptions(output.video, initial.video),
    audio:
      output.audio && initial.audio && intoOutputMoqClientAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputMoqClientVideoOptions(
  video: Outputs.MoqClientVideoOptions,
  initial: Api.VideoScene
): Api.OutputMoqClientVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoMoqClientVideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoMoqClientVideoEncoderOptions(
  encoder: Outputs.MoqClientVideoEncoderOptions
): Api.MoqClientVideoEncoderOptions {
  switch (encoder.type) {
    case 'ffmpeg_h264':
      return {
        type: 'ffmpeg_h264',
        preset: encoder.preset,
        bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
        keyframe_interval_ms: encoder.keyframeIntervalMs,
        pixel_format: encoder.pixelFormat,
        ffmpeg_options: encoder.ffmpegOptions,
      };
    case 'ffmpeg_vp8':
      return {
        type: 'ffmpeg_vp8',
        bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
        keyframe_interval_ms: encoder.keyframeIntervalMs,
        ffmpeg_options: encoder.ffmpegOptions,
      };
    case 'ffmpeg_vp9':
      return {
        type: 'ffmpeg_vp9',
        bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
        keyframe_interval_ms: encoder.keyframeIntervalMs,
        pixel_format: encoder.pixelFormat,
        ffmpeg_options: encoder.ffmpegOptions,
      };
    case 'vulkan_h264':
      return {
        type: 'vulkan_h264',
        bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
        keyframe_interval_ms: encoder.keyframeIntervalMs,
      };
  }
}

function intoOutputMoqClientAudioOptions(
  audio: Outputs.MoqClientAudioOptions,
  initial: Api.AudioScene
): Api.OutputMoqClientAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    channels: audio.channels,
    mixing_strategy: audio.mixingStrategy,
    encoder: intoMoqClientAudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoMoqClientAudioEncoderOptions(
  encoder: Outputs.MoqClientAudioEncoderOptions
): Api.MoqClientAudioEncoderOptions {
  switch (encoder.type) {
    case 'aac':
      return {
        type: 'aac',
        sample_rate: encoder.sampleRate,
      };
    case 'opus':
      return {
        type: 'opus',
        preset: encoder.preset,
        sample_rate: encoder.sampleRate,
        forward_error_correction: encoder.forwardErrorCorrection,
        expected_packet_loss: encoder.expectedPacketLoss,
      };
  }
}
