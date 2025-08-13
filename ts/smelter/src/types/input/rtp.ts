import type * as Api from '../../api.js';

export type RtpVideoDecoder = 'ffmpeg_h264' | 'ffmpeg_vp8' | 'ffmpeg_vp9' | 'vulkan_h264';

export type InputRtpVideoOptions = {
  decoder: RtpVideoDecoder;
};

export type InputRtpAudioOptions =
  | { decoder: 'opus' }
  | ({ decoder: 'aac' } & InputRtpAudioAacOptions);

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
