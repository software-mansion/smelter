import type * as Api from '../../api.js';
import type { OutputEndCondition } from './common.js';

export type WhipVideoOptions = {
  /**
   * Output resolution in pixels.
   */
  resolution: Api.Resolution;
  /**
   * Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both.
   */
  sendEosWhen?: OutputEndCondition | null;
  /**
   * Video encoder options.
   */
  encoder: WhipVideoEncoderOptions;
};

export type WhipVideoEncoderOptions = {
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

export type WhipAudioOptions = {
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
  encoder: WhipAudioEncoderOptions;
};

export type WhipAudioEncoderOptions = {
  type: 'opus';
  channels: Api.AudioChannels;
  /**
   * (**default="voip"**) Specifies preset for audio output encoder.
   */
  preset?: Api.OpusEncoderPreset;
  /**
   * (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
   */
  sampleRate?: number;
};
