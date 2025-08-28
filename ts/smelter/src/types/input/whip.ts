export type WhipVideoDecoder = 'ffmpeg_h264' | 'ffmpeg_vp8' | 'ffmpeg_vp9' | 'vulkan_h264' | 'any';

export type InputWhipVideoOptions = {
  decoderPreferences?: WhipVideoDecoder[] | null;
};
