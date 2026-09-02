import type * as Api from '../../api.js';
import type { OutputEndCondition, VideoEncoderBitrate } from './common.js';

export type RtmpClientVideoOptions = {
  /**
   * Output resolution in pixels.
   */
  resolution: Api.Resolution;
  /**
   * Defines when output stream should end if some of the input streams are finished. If output
   * includes both audio and video streams, then EOS needs to be sent on both.
   */
  sendEosWhen?: OutputEndCondition | null;
  /**
   * Video encoder options.
   */
  encoder: RtmpClientVideoEncoderOptions;
};

export type RtmpClientVideoEncoderOptions =
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
       * Preset for an encoder. See https://trac.ffmpeg.org/wiki/Encode/H.264#Preset for more.
       *
       * Defaults to `"fast"`.
       */
      preset?: Api.H264EncoderPreset;
      /**
       * Encoder pixel format. Defaults to `"yuv420p"`.
       */
      pixelFormat?: Api.PixelFormat;
      /**
       * Raw FFmpeg encoder options. See https://ffmpeg.org/ffmpeg-codecs.html for more.
       */
      ffmpegOptions?: Record<string, string>;
    }
  | {
      /**
       * Requires Enhanced RTMP support on the receiver side.
       */
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
       * Raw FFmpeg encoder options. See https://ffmpeg.org/ffmpeg-codecs.html for more.
       */
      ffmpegOptions?: Record<string, string>;
    }
  | {
      /**
       * Requires Enhanced RTMP support on the receiver side.
       */
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
       * Raw FFmpeg encoder options. See https://ffmpeg.org/ffmpeg-codecs.html for more.
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

export type RtmpClientAudioOptions = {
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
  encoder: RtmpClientAudioEncoderOptions;
};

export type RtmpClientAudioEncoderOptions =
  | {
      type: 'aac';
      /**
       * Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000]. Defaults to `44100`.
       */
      sampleRate?: number;
    }
  | {
      /**
       * Requires Enhanced RTMP support on the receiver side.
       */
      type: 'opus';
      /**
       * Audio output encoder preset. Defaults to `"voip"`.
       */
      preset?: Api.OpusEncoderPreset;
      /**
       * Sample rate. Allowed values: [8000, 16000, 24000, 48000]. Defaults to `48000`.
       */
      sampleRate?: number;
    };
