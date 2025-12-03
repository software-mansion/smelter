import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';
import type { RegisterOutputRequest } from '../output';
import { intoOutputEosCondition, intoVideoEncoderBitrate } from './common';

export function intoRegisterRtpOutput(
  output: Outputs.RegisterRtpOutput,
  initial: { video?: Api.VideoScene; audio?: Api.AudioScene }
): RegisterOutputRequest {
  return {
    type: 'rtp_stream',
    port: output.port,
    ip: output.ip,
    transport_protocol: output.transportProtocol,
    video: output.video && initial.video && intoOutputRtpVideoOptions(output.video, initial.video),
    audio: output.audio && initial.audio && intoOutputRtpAudioOptions(output.audio, initial.audio),
  };
}

export function intoOutputRtpVideoOptions(
  video: Outputs.RtpVideoOptions,
  initial: Api.VideoScene
): Api.OutputRtpVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: video.encoder && intoRtpVideoEncoderOptions(video.encoder),
    initial,
  };
}

export function intoRtpVideoEncoderOptions(
  encoder: Outputs.RtpVideoEncoderOptions
): Api.RtpVideoEncoderOptions {
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
  }
}

function intoOutputRtpAudioOptions(
  audio: Outputs.RtpAudioOptions,
  initial: Api.AudioScene
): Api.OutputRtpAudioOptions {
  return {
    send_eos_when: audio.sendEosWhen && intoOutputEosCondition(audio.sendEosWhen),
    encoder: intoRtpAudioEncoderOptions(audio.encoder),
    channels: audio.channels,
    initial,
  };
}

function intoRtpAudioEncoderOptions(
  encoder: Outputs.RtpAudioEncoderOptions
): Api.RtpAudioEncoderOptions {
  return {
    type: 'opus',
    preset: encoder.preset,
    sample_rate: encoder.sampleRate,
    forward_error_correction: encoder.forwardErrorCorrection,
    expected_packet_loss: encoder.expectedPacketLoss,
  };
}
