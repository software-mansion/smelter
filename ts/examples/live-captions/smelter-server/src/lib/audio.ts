export type ADTSHeaderOptions = {
  audioObjectType: number;
  aacFrameLength: number;
  samplingFrequencyIndex: number;
  channelConfig: number;
};

export function encodeADTSHeader(
  asc: Uint8Array<ArrayBufferLike>,
  aacFrameLength: number,
): Buffer {
  const { audioObjectType, samplingFrequencyIndex, channelConfig } =
    decodeASC(asc);

  const adtsLength = aacFrameLength + 7;
  const adts = Buffer.alloc(7);

  adts[0] = 0xff;
  adts[1] = 0xf1;
  adts[2] =
    ((audioObjectType - 1) << 6) |
    (samplingFrequencyIndex << 2) |
    (channelConfig >> 2);
  adts[3] = ((channelConfig & 3) << 6) | (adtsLength >> 11);
  adts[4] = (adtsLength >> 3) & 0xff;
  adts[5] = ((adtsLength & 7) << 5) | 0x1f;
  adts[6] = 0xfc;

  return adts;
}

function decodeASC(asc: Uint8Array<ArrayBufferLike>) {
  const audioObjectType = (asc[0]! >> 3) & 0x1f;
  const samplingFrequencyIndex =
    ((asc[0]! & 0x07) << 1) | ((asc[1]! & 0x80) >> 7);
  const channelConfig = (asc[1]! >> 3) & 0x0f;

  return {
    audioObjectType,
    samplingFrequencyIndex,
    channelConfig,
  };
}
