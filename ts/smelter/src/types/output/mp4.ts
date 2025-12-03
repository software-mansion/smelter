import type * as Api from '../../api.js';
import type { OutputEndCondition, VideoEncoderBitrate } from './common.js';

export type Mp4VideoOptions = {
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
  encoder: Mp4VideoEncoderOptions;
};

export type Mp4VideoEncoderOptions =
  | {
      type: 'ffmpeg_h264';
      /**
       * Encoding bitrate. Default value depends on chosen encoder.
       */
      bitrate?: VideoEncoderBitrate;
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
      ffmpegOptions?: Record<string, string>;
    }
  | {
      type: 'vulkan_h264';
      /**
       * Encoding bitrate in bits/second. If not provided, bitrate is calculated based on resolution and framerate.
       * For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250 kbit/s.
       */
      bitrate?: VideoEncoderBitrate;
    };

export type Mp4AudioOptions = {
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
  encoder: Mp4AudioEncoderOptions;
};

export type Mp4AudioEncoderOptions = {
  type: 'aac';
  /**
   * (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
   */
  sampleRate?: number;
};
