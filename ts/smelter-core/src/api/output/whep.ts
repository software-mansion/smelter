import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition } from './common';

export function intoRegisterWhepServerOutput(
  output: Outputs.RegisterWhepServerOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'whep_server',
    bearer_token: output.bearerToken,

    video:
      output.video &&
      initial.video &&
      intoOutputWhepServerVideoOptions(output.video, initial.video),
    audio:
      output.audio &&
      initial.audio &&
      intoOutputWhepServerAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputWhepServerVideoOptions(
  video: Outputs.WhepServerVideoOptions | null | undefined,
  initial: Api.VideoScene | undefined
): Api.OutputVideoOptions | undefined {
  if (!video || !initial) {
    return undefined;
  }

  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: video.encoder && intoWhepServerVideoEncoderOptions(video.encoder),
    initial,
  };
}

export function intoWhepServerVideoEncoderOptions(
  encoder: Outputs.WhepServerVideoEncoderOptions
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
  }
}

function intoOutputWhepServerAudioOptions(
  audio: Outputs.WhepServerAudioOptions,
  initial: Api.AudioScene
): Api.OutputWhepServerAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoWhepServerAudioEncoderOptions(audio.encoder),
    channels: audio.channels,
    initial,
  };
}

function intoWhepServerAudioEncoderOptions(
  encoder: Outputs.WhepServerAudioEncoderOptions
): Api.WhepServerAudioEncoderOptions {
  return {
    type: 'opus',
    preset: encoder.preset,
    sample_rate: encoder.sampleRate,
    forward_error_correction: encoder.forwardErrorCorrection,
    expected_packet_loss: encoder.expectedPacketLoss,
  };
}
