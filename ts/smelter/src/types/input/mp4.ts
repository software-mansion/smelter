import type { H264Decoder } from './common.js';

export type InputMp4VideoOptions = {
  /**
   * Configures decoders for the provided codecs.
   */
  decoders?: InputMp4DecoderPreferences;
};

export type InputMp4DecoderPreferences = {
  h264?: H264Decoder;
};
