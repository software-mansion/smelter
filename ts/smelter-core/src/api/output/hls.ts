import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVideoEncoderBitrate } from './common';

export function intoRegisterHlsOutput(
  output: Outputs.RegisterHlsOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'hls',
    path: output.serverPath,
    max_playlist_size: output.maxPlaylistSize,
    video: output.video && initial.video && intoOutputHlsVideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputHlsAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputHlsVideoOptions(
  video: Outputs.HlsVideoOptions,
  initial: Api.VideoScene
): Api.OutputHlsVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoHlsVideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoHlsVideoEncoderOptions(
  encoder: Outputs.HlsVideoEncoderOptions
): Api.HlsVideoEncoderOptions {
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
        bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
      };
  }
}

function intoOutputHlsAudioOptions(
  audio: Outputs.HlsAudioOptions,
  initial: Api.AudioScene
): Api.OutputHlsAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    channels: audio.channels,
    encoder: intoHlsAudioEncoderOptions(audio.encoder),
    initial,
  };
}

function intoHlsAudioEncoderOptions(
  encoder: Outputs.HlsAudioEncoderOptions
): Api.HlsAudioEncoderOptions {
  return {
    type: 'aac',
    sample_rate: encoder.sampleRate,
  };
}
