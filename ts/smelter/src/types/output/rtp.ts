import type * as Api from '../../api.js';
import type { OutputEndCondition } from './common.js';

export type RtpVideoOptions = {
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
  encoder: RtpVideoEncoderOptions;
};

export type RtpVideoEncoderOptions =
  | {
      type: 'ffmpeg_h264';
      /**
       * (**default=`"fast"`**) Preset for an encoder. See `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
       */
      preset?: Api.H264EncoderPreset;
      /**
       * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
       */
      ffmpegOptions?: Api.VideoEncoderOptions['ffmpeg_options'];
    }
  | {
      type: 'ffmpeg_vp8';
      /**
       * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
       */
      ffmpegOptions?: Api.VideoEncoderOptions['ffmpeg_options'];
    }
  | {
      type: 'ffmpeg_vp9';
      /**
       * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
       */
      ffmpegOptions?: Api.VideoEncoderOptions['ffmpeg_options'];
    };

export type RtpAudioOptions = {
  channels: Api.AudioChannels;
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
  encoder: RtpAudioEncoderOptions;
};

export type RtpAudioEncoderOptions = {
  type: 'opus';
  /**
   * (**default="voip"**) Specifies preset for audio output encoder.
   */
  preset?: Api.OpusEncoderPreset;
  /**
   * (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
   */
  sampleRate?: number;
};
