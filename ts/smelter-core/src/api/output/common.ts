import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';

export function intoOutputVideoOptions(
  video: Outputs.RtpVideoOptions | Outputs.Mp4VideoOptions | Outputs.WhipVideoOptions,
  initial: Api.Video
): Api.OutputVideoOptions {
  return {
    resolution: video.resolution,
    send_eos_when: video.sendEosWhen && intoOutputEosCondition(video.sendEosWhen),
    encoder: intoVideoEncoderOptions(video.encoder),
    initial,
  };
}

function intoVideoEncoderOptions(
  encoder:
    | Outputs.RtpVideoEncoderOptions
    | Outputs.Mp4VideoEncoderOptions
    | Outputs.WhipVideoEncoderOptions
): Api.VideoEncoderOptions {
  return {
    type: 'ffmpeg_h264',
    preset: encoder.preset,
    ffmpeg_options: encoder.ffmpegOptions,
  };
}

export function intoOutputEosCondition(
  condition: Outputs.OutputEndCondition
): Api.OutputEndCondition {
  if ('anyOf' in condition) {
    return { any_of: condition.anyOf };
  } else if ('allOf' in condition) {
    return { all_of: condition.allOf };
  } else if ('allInputs' in condition) {
    return { all_inputs: condition.allInputs };
  } else if ('anyInput' in condition) {
    return { any_input: condition.anyInput };
  } else {
    throw new Error('Invalid "send_eos_when" value.');
  }
}
