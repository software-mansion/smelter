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
};
