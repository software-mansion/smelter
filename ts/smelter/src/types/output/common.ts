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

export type VulkanH264EncoderRateControl =
  | {
      /**
       * Uses the default setting of the encoder implementation. **It's not necessarily a good default**, for most use cases, `vbr` is the correct option.
       */
      type: 'encoder_default';
    }
  | {
      /**
       * Variable bitrate rate control. This setting fits most use cases. The encoder will try to
       * keep the bitrate around the average, but may increase it temporarily up to the max when
       * necessary. Bitrate is measured in bits/second.
       */
      type: 'vbr';
      averageBitrate: number;
      maxBitrate: number;
    }
  | {
      /**
       * Rate control is turned off, frames are compressed with a constant rate. A more complicated
       * frame will just be bigger.
       */
      type: 'disabled';
    };
