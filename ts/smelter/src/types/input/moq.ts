import type { H264Decoder } from './common.js';

export type InputMoqDecoderMap = {
  h264?: H264Decoder;
};

export type InputMoqBufferOptions = {
  /**
   * Buffer the input starts with, in milliseconds, measured for the chunks that arrive the
   * fastest. It is also the value the buffer is brought back to when `minMs` or `maxMs` is
   * not met.
   */
  desiredMs?: number | null;
  /**
   * Smallest buffer, in milliseconds, that chunks arriving late on a regular basis (network
   * jitter, stalls that keep coming back) should be left with. When they have less, the
   * buffer grows.
   */
  minMs?: number | null;
  /**
   * Largest buffer, in milliseconds, measured for the chunks that arrive the fastest. When
   * the buffer is larger, it shrinks back to `desiredMs`.
   */
  maxMs?: number | null;
  /**
   * What happens when the stream can't be on time with a buffer of `maxMs`.
   * - `"extend"` - The buffer grows past `maxMs` as far as it is needed to keep `minMs` for
   *   the chunks that are late on a regular basis. Nothing is lost.
   * - `"enforce"` - Playback is never more than `maxMs` behind the live edge of the stream,
   *   measured on audio when the input has it. Content that arrives too late is not played
   *   on time: late audio is dropped and late video is shown late.
   *
   * Defaults to `"enforce"`.
   */
  maxPolicy?: 'extend' | 'enforce' | null;
};

/**
 * Buffer a MoQ input keeps between the live edge of the stream and playback. A number
 * value represents the `desiredMs` option.
 */
export type InputMoqBuffer = number | InputMoqBufferOptions;
