import type { H264Decoder } from './common.js';

export type InputHlsVideoOptions = {
  /**
   * Configures decoders for the provided codecs.
   */
  decoders?: InputHlsDecoderPreferences;
};

export type InputHlsDecoderPreferences = {
  h264?: H264Decoder;
};
