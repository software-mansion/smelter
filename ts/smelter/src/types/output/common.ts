import type * as Api from '../../api.js';

export type OutputEndCondition =
  | {
      /**
       * Terminate output stream if any of the input streams from the list are finished.
       */
      anyOf: Api.InputId[];
    }
  | {
      /**
       * Terminate output stream if all the input streams from the list are finished.
       */
      allOf: Api.InputId[];
    }
  | {
      /**
       * Terminate output stream if any of the input streams ends. This includes streams added after the output was registered. In particular, output stream will **not be** terminated if no inputs were ever connected.
       */
      anyInput: boolean;
    }
  | {
      /**
       * Terminate output stream if all the input streams finish. In particular, output stream will **be** terminated if no inputs were ever connected.
       */
      allInputs: boolean;
    };

export type VideoEncoderBitrate =
  | number
  | {
      /**
       * Average bitrate measured in bits/second. Encoder will try to keep the bitrate around the provided average,
       * but may temporarily increase it to the provided max bitrate.
       */
      averageBitrate: number;
      /**
       * Max bitrate measured in bits/second.
       */
      maxBitrate: number;
    };
