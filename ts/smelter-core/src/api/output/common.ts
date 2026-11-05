import type { Api, Outputs, _smelterInternals } from '@swmansion/smelter';

export function intoVideoEncoderBitrate(
  bitrate: Outputs.VideoEncoderBitrate
): Api.VideoEncoderBitrate {
  if (typeof bitrate === 'number') {
    return bitrate;
  }

  return {
    average_bitrate: bitrate.averageBitrate,
    max_bitrate: bitrate.maxBitrate,
  };
}
