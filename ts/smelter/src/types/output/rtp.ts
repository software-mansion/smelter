import type * as Api from '../../api.js';
import type { OutputEndCondition, VideoEncoderBitrate } from './common.js';

export type RtpVideoOptions = {
  /**
   * Output resolution in pixels.
   */
  resolution: Api.Resolution;
  /**
   * Defines when output stream should end if some of the input streams are finished. If output
   * includes both audio and video streams, then EOS needs to be sent on both.
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
       * Encoding bitrate. Default value depends on chosen encoder.
       */
      bitrate?: VideoEncoderBitrate;
      /**
       * Maximal interval between keyframes, in milliseconds. Defaults to `5000`.
       */
      keyframeIntervalMs?: number;
      /**
       * Preset for an encoder. See `FFmpeg`
       * [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
       *
       * Defaults to `"fast"`.
       */
      preset?: Api.H264EncoderPreset;
      /**
       * Encoder pixel format. Defaults to `"yuv420p"`.
       */
      pixelFormat?: Api.PixelFormat;
      /**
       * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
       */
      ffmpegOptions?: Record<string, string>;
    }
  | {
      type: 'ffmpeg_vp8';
      /**
       * Encoding bitrate. If not provided, bitrate is calculated based on resolution and framerate.
       * For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250
       * kbit/s.
       */
      bitrate?: VideoEncoderBitrate;
      /**
       * Maximal interval between keyframes, in milliseconds. Defaults to `5000`.
       */
      keyframeIntervalMs?: number;
      /**
       * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
       */
      ffmpegOptions?: Record<string, string>;
    }
  | {
      type: 'ffmpeg_vp9';
      /**
       * Encoding bitrate. If not provided, bitrate is calculated based on resolution and framerate.
       * For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max bitrate is 6250
       * kbit/s.
       */
      bitrate?: VideoEncoderBitrate;
      /**
       * Maximal interval between keyframes, in milliseconds. Defaults to `5000`.
       */
      keyframeIntervalMs?: number;
      /**
       * Encoder pixel format. Defaults to `"yuv420p"`.
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
       * Encoding bitrate in bits/second. If not provided, bitrate is calculated based on resolution
       * and framerate. For example at 1080p 30 FPS the average bitrate is 5000 kbit/s and max
       * bitrate is 6250 kbit/s.
       */
      bitrate?: VideoEncoderBitrate;
      /**
       * Interval between keyframes, in milliseconds. Defaults to `5000`.
       */
      keyframeIntervalMs?: number;
    };

export type RtpAudioOptions = {
  /**
   * Specifies channels configuration. Defaults to `"stereo"`.
   */
  channels?: Api.AudioChannels | null;
  /**
   * Specifies how audio should be mixed. Defaults to `"sum_clip"`.
   */
  mixingStrategy?: Api.AudioMixingStrategy | null;
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
   * Specifies preset for audio output encoder. Defaults to `"voip"`.
   */
  preset?: Api.OpusEncoderPreset;
  /**
   * Sample rate. Allowed values: [8000, 16000, 24000, 48000]. Defaults to `48000`.
   */
  sampleRate?: number;
  /**
   * Specifies if forward error correction (FEC) should be used. Defaults to `false`.
   */
  forwardErrorCorrection?: boolean;
  /**
   * Expected packet loss. When `forward_error_correction` is set to `true`, then this value should
   * be greater than `0`. Allowed values: [0, 100];
   *
   * Defaults to `0`.
   */
  expectedPacketLoss?: number;
};
