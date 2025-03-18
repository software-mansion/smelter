import type * as Api from '../api.js';

export type RegisterRtpInput = {
  /**
   * UDP port or port range on which the smelter should listen for the stream.
   */
  port: Api.PortOrPortRange;
  /**
   * Transport protocol.
   */
  transportProtocol?: Api.TransportProtocol | null;
  /**
   * Parameters of a video source included in the RTP stream.
   */
  video?: Api.InputRtpVideoOptions | null;
  /**
   * Parameters of an audio source included in the RTP stream.
   */
  audio?: InputRtpAudioOptions | null;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered
   * on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Offset in milliseconds relative to the pipeline start (start request). If the offset is
   * not defined then the stream will be synchronized based on the delivery time of the initial
   * frames.
   */
  offsetMs?: number | null;
};

export type RegisterMp4Input = {
  /**
   * URL of the MP4 file.
   */
  url?: string | null;
  /**
   * Path to the MP4 file (location on the server where Smelter server is deployed).
   */
  serverPath?: string | null;
  /**
   * Blob of the MP4 file (only available in smelter-web-wasm).
   */
  blob?: any;
  /**
   * (**default=`false`**) If input should be played in the loop. <span class="badge badge--primary">Added in v0.4.0</span>
   */
  loop?: boolean | null;
  /**
   * (**default=`false`**) If input is required and frames are not processed
   * on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Offset in milliseconds relative to the pipeline start (start request). If offset is
   * not defined then stream is synchronized based on the first frames delivery time.
   */
  offsetMs?: number | null;
  /**
   * (**default=`ffmpeg_h264`**) The decoder to use for decoding video.
   */
  videoDecoder?: Api.VideoDecoder | null;
};

export type RegisterWhipInput = {
  /**
   * Parameters of a video source included in the RTP stream.
   */
  video?: Api.InputWhipVideoOptions | null;
  /**
   * Parameters of an audio source included in the RTP stream.
   */
  audio?: InputWhipAudioOptions | null;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered
   * on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Offset in milliseconds relative to the pipeline start (start request). If the offset is
   * not defined then the stream will be synchronized based on the delivery time of the initial
   * frames.
   */
  offsetMs?: number | null;
};

export type InputRtpAudioOptions =
  | ({ decoder: 'opus' } & InputRtpAudioOpusOptions)
  | ({ decoder: 'aac' } & InputRtpAudioAacOptions);

export type InputRtpAudioOpusOptions = {
  /**
   * (**default=`false`**) Specifies whether the stream uses forward error correction.
   * It's specific for Opus codec.
   * For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
   */
  forwardErrorCorrection?: boolean | null;
};

export type InputRtpAudioAacOptions = {
  /**
   * AudioSpecificConfig as described in MPEG-4 part 3, section 1.6.2.1
   * The config should be encoded as described in [RFC 3640](https://datatracker.ietf.org/doc/html/rfc3640#section-4.1).
   *
   * The simplest way to obtain this value when using ffmpeg to stream to the smelter is
   * to pass the additional `-sdp_file FILENAME` option to ffmpeg. This will cause it to
   * write out an sdp file, which will contain this field. Programs which have the ability
   * to stream AAC to the smelter should provide this information.
   *
   * In MP4 files, the ASC is embedded inside the esds box (note that it is not the whole
   * box, only a part of it). This also applies to fragmented MP4s downloaded over HLS, if
   * the playlist uses MP4s instead of MPEG Transport Streams
   *
   * In FLV files and the RTMP protocol, the ASC can be found in the `AACAUDIODATA` tag.
   */
  audioSpecificConfig: string;
  /**
   * (**default=`"high_bitrate"`**)
   * Specifies the [RFC 3640 mode](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.1)
   * that should be used when depacketizing this stream.
   */
  rtpMode?: Api.AacRtpMode | null;
};

export type InputWhipAudioOptions = {
  decoder: 'opus';
  /**
   * (**default=`false`**) Specifies whether the stream uses forward error correction.
   * It's specific for Opus codec.
   * For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
   */
  forwardErrorCorrection?: boolean | null;
};
