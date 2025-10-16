export type WhepVideoDecoder = 'ffmpeg_h264' | 'ffmpeg_vp8' | 'ffmpeg_vp9' | 'vulkan_h264' | 'any';

export type InputWhepVideoOptions = {
  decoderPreferences?: WhepVideoDecoder[] | null;
};
