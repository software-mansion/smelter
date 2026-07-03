import type * as Api from '../api.js';
import type { InputHlsDecoderMap } from './input/hls.js';
import type { InputMp4DecoderMap } from './input/mp4.js';
import type { InputRtpAudioOptions, InputRtpVideoOptions } from './input/rtp.js';
import type { SideChannel } from './input/common.js';
import type { InputWhipVideoOptions } from './input/whip.js';
import type { InputWhepVideoOptions } from './input/whep.js';
import type { InputRtmpDecoderMap } from './input/rtmp.js';
import type { InputMoqDecoderMap } from './input/moq.js';

export * from './input/mp4.js';
export * from './input/hls.js';
export * from './input/whip.js';
export * from './input/whep.js';
export * from './input/rtp.js';
export * from './input/rtmp.js';
export * from './input/moq.js';
export * from './input/common.js';

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
  video?: InputRtpVideoOptions | null;
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
  /**
   * Size of the jitter buffer in milliseconds. Controls how long packets are held to
   * absorb network jitter and reorder out-of-order packets. Higher values increase
   * latency but improve resilience to packet loss and reordering.
   */
  bufferSizeMs?: number | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
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
   * Start playing from a specific timestamp in milliseconds. If loop is enabled after
   * first iteration is done it will start from the beginning.
   */
  seekMs?: number | null;
  /**
   * Assigns which decoder should be used for media encoded with a specific codec.
   */
  decoderMap?: InputMp4DecoderMap | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterHlsInput = {
  /**
   * URL of the HLS playlist.
   */
  url: string;
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
   * Assigns which decoder should be used for media encoded with a specific codec.
   */
  decoderMap?: InputHlsDecoderMap | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterWhipServerInput = {
  /**
   * Parameters of a video source included in the RTP stream.
   */
  video?: InputWhipVideoOptions | null;
  /**
   * Bearer token used for authenticating WHIP connection. If not provided, a random token
   * will be generated and returned from the register input call.
   */
  bearerToken?: string;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered
   * on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Reference/desired jitter buffer size in milliseconds. The adaptive buffer converges
   * toward this value in steady state; it shifts as conditions change. Higher values
   * trade latency for resilience.
   */
  bufferSizeMs?: number | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterWhepClientInput = {
  /**
   * WHEP server endpoint URL.
   */
  endpointUrl: string;
  /**
   * Bearer token used for authenticating WHEP connection.
   */
  bearerToken?: string;
  /**
   * Parameters of a video source included in the RTP stream.
   */
  video?: InputWhepVideoOptions | null;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered
   * on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Reference/desired jitter buffer size in milliseconds. The adaptive buffer converges
   * toward this value in steady state; it shifts as conditions change. Higher values
   * trade latency for resilience.
   */
  bufferSizeMs?: number | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterRtmpServerInput = {
  type: 'rtmp_server';
  /**
   * The RTMP stream key.
   *
   * In most RTMP clients you will need to provide url in following format
   * `rtmp://<ip_address>:<port>/<input_id>/<stream_key>`
   */
  streamKey: string;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Offset in milliseconds relative to the pipeline start (start request). If the offset is not defined then the stream will be synchronized based on the delivery time of the initial frames.
   */
  offsetMs?: number | null;
  /**
   * Assigns which decoder should be used for media encoded with a specific codec.
   */
  decoderMap?: InputRtmpDecoderMap | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterMoqServerInput = {
  type: 'moq_server';
  /**
   * Token used for authentication in MoQ server input. The broadcaster must provide it as a
   * `token` query parameter when connecting.
   */
  authToken: string;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Assigns which decoder should be used for media encoded with a specific codec.
   */
  decoderMap?: InputMoqDecoderMap | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterMoqClientInput = {
  type: 'moq_client';
  /**
   * URL of the MoQ relay to connect to. Must use the `https://` scheme.
   */
  endpointUrl: string;
  /**
   * Path of the broadcast to subscribe to on the relay.
   */
  broadcastPath: string;
  /**
   * (**default=`false`**) Skips validation of the relay's TLS certificate. Only enable this on
   * trusted networks — it leaves the connection vulnerable to man-in-the-middle attacks.
   */
  disableTlsVerification?: boolean | null;
  /**
   * (**default=`false`**) If input is required and the stream is not delivered on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Assigns which decoder should be used for media encoded with a specific codec.
   */
  decoderMap?: InputMoqDecoderMap | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};

export type RegisterV4l2Input = {
  type: 'v4l2';
  /**
   * Path to the V4L2 device.
   *
   * Typically looks like either of:
   *  - `/dev/video[N]`, where `[N]` is the OS-assigned device number
   *  - `/dev/v4l/by-id/[ID]`, where `[ID]` is the unique device id
   *  - `/dev/v4l/by-path/[PATH]`, where `[PATH]` is the PCI/USB device path
   *
   * While the numbers assigned in `/dev/video<N>` paths can differ depending on device
   * detection order, the `by-id` paths are always the same for a given device, and
   * the `by-path` paths should be the same for specific ports.
   */
  path: string;
  /**
   * The format that will be negotiated with the device.
   */
  format: Api.V4L2InputFormat;
  /**
   * The resolution that will be negotiated with the device.
   *
   * If not provided, the input will use the default resolution for the given format.
   */
  resolution?: Api.Resolution | null;
  /**
   * The framerate that will be negotiated with the device.
   *
   * Must by either an unsigned integer, or a string in the \"NUM/DEN\" format, where NUM and DEN are both unsigned integers.
   * If not provided, the input will use the default framerate for the given format and resolution.
   */
  framerate?: Api.Framerate | null;
  /**
   * (**default=`false`**) If input is required and frames are not processed on time, then Smelter will delay producing output frames.
   */
  required?: boolean | null;
  /**
   * Enable side channel for video and/or audio track.
   */
  sideChannel?: SideChannel;
};
