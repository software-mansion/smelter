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
   * Buffer the input aims to keep, in milliseconds. At the start it should buffer at least
   * that much media before producing first chunk.
   */
  desiredMs?: number | null;
  /**
   * Lower range of what is considered stable state. If buffer is smaller than this value
   * then media will be slightly "stretched" so the buffer converges on desired value.
   */
  minMs?: number | null;
  /**
   * Upper range of what is considered stable state. If buffer is larger than this value
   * then media will be slightly "squashed" so the buffer converges on desired value.
   */
  maxMs?: number | null;
};

/**
 * Buffer a live input keeps between the live edge of the stream and playback. A number
 * value represents the `desiredMs` option.
 */
export type InputBuffer = number | InputBufferOptions;
