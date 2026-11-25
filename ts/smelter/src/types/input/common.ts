export type H264Decoder = 'ffmpeg_h264' | 'vulkan_h264';

export type SideChannel = {
  /**
   * Enable side channel for video track.
   */
  video?: boolean;
  /**
   * Enable side channel for audio track.
   */
  audio?: boolean;
  /**
   * Side channel delay in milliseconds. Frames are buffered for this duration ahead of when
   * the queue consumes them, so the side-channel subscriber receives them early and has
   * roughly this much time to process before the frame is due.
   */
  delayMs?: number;
};

export type InputBufferOptions = {
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
   * the buffer is larger, it shrinks back to `desiredMs`. The buffer still grows past this
   * value when it is needed to keep `minMs` for the chunks that are late on a regular basis.
   */
  maxMs?: number | null;
};

/**
 * Buffer a live input keeps between the live edge of the stream and playback. A number
 * value represents the `desiredMs` option.
 */
export type InputBuffer = number | InputBufferOptions;
