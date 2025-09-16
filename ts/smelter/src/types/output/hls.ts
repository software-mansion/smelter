import type * as Api from '../../api.js';
import type { OutputEndCondition, VulkanH264EncoderBitrate } from './common.js';

export type HlsVideoOptions = {
  /**
   * Output resolution in pixels.
   */
  resolution: Api.Resolution;
  /**
   * Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both.
   */
  sendEosWhen?: OutputEndCondition;
  /**
   * Video encoder options.
   */
  encoder: HlsVideoEncoderOptions;
};

export type HlsVideoEncoderOptions =
  | {
      type: 'ffmpeg_h264';
      /**
       * (**default=`"fast"`**) Preset for an encoder. See `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
       */
      preset?: Api.H264EncoderPreset;
      /**
       * (**default=`"yuv420p"`**) Encoder pixel format
       */
      pixelFormat?: Api.PixelFormat;
      /**
       * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
       */
      ffmpegOptions?: Extract<Api.VideoEncoderOptions, { type: 'ffmpeg_h264' }>['ffmpeg_options'];
    }
  | {
      type: 'vulkan_h264';
      /**
       * Encoding bitrate in bits/second. If not provided, bitrate is calculated based on resolution and framerate.
       * For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250 kbit/s.
       */
      bitrate?: VulkanH264EncoderBitrate;
    };

export type HlsAudioOptions = {
  /**
   * (**default="stereo"**) Specifies channels configuration.
   */
  channels?: Api.AudioChannels | null;
  /**
   * (**default="sum_clip"**) Specifies how audio should be mixed.
   */
  mixingStrategy?: Api.AudioMixingStrategy | null;
  /**
   * Condition for termination of output stream based on the input streams states.
   */
  sendEosWhen?: OutputEndCondition | null;
  /**
   * Audio encoder options.
   */
  encoder: HlsAudioEncoderOptions;
};

export type HlsAudioEncoderOptions = {
  type: 'aac';
  /**
   * (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
   */
  sampleRate?: number;
};
