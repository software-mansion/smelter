import type * as Api from '../../api.js';
import type { OutputEndCondition } from './common.js';

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

export type Mp4VideoEncoderOptions = {
  type: 'ffmpeg_h264';
  /**
   * (**default=`"fast"`**) Preset for an encoder. See `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
   */
  preset: Api.H264EncoderPreset;
  /**
   * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
   */
  ffmpegOptions?: Api.VideoEncoderOptions['ffmpeg_options'];
};

export type Mp4AudioOptions = {
  /**
   * (**default="sum_clip"**) Specifies how audio should be mixed.
   */
  mixingStrategy?: Api.MixingStrategy | null;
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
  channels: Api.AudioChannels;
  /**
   * (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
   */
  sampleRate?: number;
};
