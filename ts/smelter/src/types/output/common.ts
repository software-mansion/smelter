export type VideoEncoderBitrate =
  | number
  | {
      /**
       * Average bitrate measured in bits/second. Encoder will try to keep the bitrate around the
       * provided average, but may temporarily increase it to the provided max bitrate.
       */
      averageBitrate: number;
      /**
       * Max bitrate measured in bits/second.
       */
      maxBitrate: number;
    };
