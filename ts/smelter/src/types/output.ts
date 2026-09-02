import type * as Api from '../api.js';
import type { Mp4AudioOptions, Mp4VideoOptions } from './output/mp4.js';
import type { HlsAudioOptions, HlsVideoOptions } from './output/hls.js';
import type { RtmpClientAudioOptions, RtmpClientVideoOptions } from './output/rtmp.js';
import type {
  MoqClientAudioOptions,
  MoqClientVideoOptions,
  MoqOutputContainer,
} from './output/moq.js';
import type { RtpAudioOptions, RtpVideoOptions } from './output/rtp.js';
import type { WhipAudioOptions, WhipVideoOptions } from './output/whip.js';
import type { WhepAudioOptions, WhepVideoOptions } from './output/whep.js';

export * from './output/mp4.js';
export * from './output/hls.js';
export * from './output/whip.js';
export * from './output/whep.js';
export * from './output/rtp.js';
export * from './output/common.js';
export * from './output/rtmp.js';
export * from './output/moq.js';

export type RegisterRtpOutput = {
  /**
   * Depends on the value of the `transport_protocol` field:
   * - `udp` - An UDP port number that RTP packets will be sent to.
   * - `tcp_server` - A local TCP port number or a port range that Smelter will listen for incoming
   *   connections.
   */
  port: Api.PortOrPortRange;
  /**
   * Only valid if `transport_protocol="udp"`. IP address where RTP packets should be sent to.
   */
  ip?: string | null;
  /**
   * Transport layer protocol that will be used to send RTP packets. Defaults to `"udp"`.
   */
  transportProtocol?: Api.TransportProtocol;
  video?: RtpVideoOptions;
  audio?: RtpAudioOptions;
};

export type RegisterMp4Output = {
  /**
   * Path to output MP4 file (location on the server where Smelter server is deployed).
   */
  serverPath: string;
  /**
   * Video track configuration.
   */
  video?: Mp4VideoOptions;
  /**
   * Audio track configuration.
   */
  audio?: Mp4AudioOptions;
  /**
   * Raw FFmpeg muxer options. See https://ffmpeg.org/ffmpeg-formats.html for more.
   */
  ffmpegOptions?: Record<string, string>;
  /**
   * Time in milliseconds when this output should start producing data. Value `0` represents the
   * time of the `Smelter.start()` call. Output is always created when it is registered (e.g. file
   * is created), only the moment it starts receiving frames/samples is delayed.
   */
  startAtMs?: number | null;
};

export type RegisterHlsOutput = {
  /**
   * Path to output HLS playlist (location on the server where Smelter server is deployed).
   */
  serverPath: string;
  /**
   * Number of segments kept in the playlist. When the limit is reached the oldest segment is
   * removed. If not specified, no segments will removed.
   */
  maxPlaylistSize?: number | null;
  /**
   * Video track configuration.
   */
  video?: HlsVideoOptions;
  /**
   * Audio track configuration.
   */
  audio?: HlsAudioOptions;
  /**
   * Raw FFmpeg muxer options. See https://ffmpeg.org/ffmpeg-formats.html for more.
   * Note: keys here may override defaults, including `hls_list_size` derived from
   * `maxPlaylistSize`.
   */
  ffmpegOptions?: Record<string, string>;
  /**
   * Time in milliseconds when this output should start producing data. Value `0` represents the
   * time of the `Smelter.start()` call. Output is always created when it is registered (e.g.
   * playlist is created), only the moment it starts receiving frames/samples is delayed.
   */
  startAtMs?: number | null;
};

export type RegisterWhipClientOutput = {
  /**
   * WHIP server endpoint.
   */
  endpointUrl: string;
  /**
   * Token for authenticating comunication with the WHIP server.
   */
  bearerToken?: string | null;
  /**
   * Video track configuration.
   */
  video?: WhipVideoOptions | null;
  /**
   * Audio track configuration.
   */
  audio?: true | WhipAudioOptions | null;
};

export type RegisterWhepServerOutput = {
  /**
   * Token for authenticating comunication with the WHEP server.
   */
  bearerToken?: string | null;
  /**
   * Video track configuration.
   */
  video?: WhepVideoOptions | null;
  /**
   * Audio track configuration.
   */
  audio?: WhepAudioOptions | null;
};

export type RegisterRtmpClientOutput = {
  /**
   * RTMP url.
   */
  url: string;
  /**
   * Video track configuration.
   */
  video?: RtmpClientVideoOptions | null;
  /**
   * Audio track configuration.
   */
  audio?: RtmpClientAudioOptions | null;
};

export type RegisterMoqClientOutput = {
  /**
   * URL of the MoQ relay to connect to. Must use the `https://` scheme.
   */
  endpointUrl: string;
  /**
   * Path the broadcast will be published under on the relay.
   */
  broadcastPath: string;
  /**
   * Container used to frame encoded media. Defaults to `"cmaf"`.
   */
  container?: MoqOutputContainer | null;
  /**
   * Video track configuration.
   */
  video?: MoqClientVideoOptions | null;
  /**
   * Audio track configuration.
   */
  audio?: MoqClientAudioOptions | null;
};
