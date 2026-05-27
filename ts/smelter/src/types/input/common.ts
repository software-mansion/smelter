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
