import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVideoEncoderBitrate } from './common';

export function intoRegisterWhipClientOutput(
  output: Outputs.RegisterWhipClientOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'whip_client',
    endpoint_url: output.endpointUrl,
    bearer_token: output.bearerToken,

    video: intoOutputWhipVideoOptions(output.video, initial.video),
    audio: intoOutputWhipAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputWhipVideoOptions(
  video: Outputs.WhipVideoOptions | null | undefined,
  initial: Api.VideoScene | undefined
): Api.OutputWhipVideoOptions | undefined {
  if (!video || !initial) {
    return undefined;
  }

  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder_preferences:
      video.encoderPreferences && intoWhipVideoEncoderPreferences(video.encoderPreferences),
    initial,
  };
}

function intoWhipVideoEncoderPreferences(
  encoder_preferences: Outputs.WhipVideoEncoderOptions[]
): Api.WhipVideoEncoderOptions[] {
  return encoder_preferences.map(encoder => {
    switch (encoder.type) {
      case 'ffmpeg_vp9':
        return {
          type: 'ffmpeg_vp9',
          bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
          pixel_format: encoder.pixelFormat,
          ffmpeg_options: encoder.ffmpegOptions,
        };
      case 'ffmpeg_vp8':
        return {
          type: 'ffmpeg_vp8',
          bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
          ffmpeg_options: encoder.ffmpegOptions,
        };
      case 'ffmpeg_h264':
        return {
          type: 'ffmpeg_h264',
          bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
          preset: encoder.preset,
          pixel_format: encoder.pixelFormat,
          ffmpeg_options: encoder.ffmpegOptions,
        };
      case 'vulkan_h264':
        return {
          type: 'vulkan_h264',
          bitrate: encoder.bitrate && intoVideoEncoderBitrate(encoder.bitrate),
        };
      case 'any':
        return {
          type: 'any',
        };
    }
  });
}

function intoOutputWhipAudioOptions(
  audio: true | Outputs.WhipAudioOptions | null | undefined,
  initial: Api.AudioScene | undefined
): Api.OutputWhipAudioOptions | undefined {
  if (!audio || !initial) {
    return undefined;
  }

  if (audio === true) {
    return { initial };
  }

  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    channels: audio.channels,
    encoder_preferences:
      audio.encoderPreferences && intoWhipAudioEncoderPreferences(audio.encoderPreferences),
    initial,
  };
}

function intoWhipAudioEncoderPreferences(
  encoder_preferences: Outputs.WhipAudioEncoderOptions[]
): Api.WhipAudioEncoderOptions[] {
  return encoder_preferences.map(encoder => {
    switch (encoder.type) {
      case 'opus':
        return {
          type: 'opus',
          preset: encoder.preset,
          sample_rate: encoder.sampleRate,
          forward_error_correction: encoder.forwardErrorCorrection,
        };
      case 'any':
        return {
          type: 'any',
        };
    }
  });
}
