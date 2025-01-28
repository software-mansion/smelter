/* eslint-disable */
/**
 * This file was automatically generated by json-schema-to-typescript.
 * DO NOT MODIFY IT BY HAND. Instead, modify the source JSONSchema file,
 * and run json-schema-to-typescript to regenerate this file.
 */

/**
 * This enum is used to generate JSON schema for all API types.
 * This prevents repeating types in generated schema.
 */
export type ApiTypes = RegisterInput | RegisterOutput | ImageSpec | WebRendererSpec | ShaderSpec | UpdateOutputRequest;
export type RegisterInput =
  | {
      type: "rtp_stream";
      /**
       * UDP port or port range on which the compositor should listen for the stream.
       */
      port: PortOrPortRange;
      /**
       * Transport protocol.
       */
      transport_protocol?: TransportProtocol | null;
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
       * on time, then LiveCompositor will delay producing output frames.
       */
      required?: boolean | null;
      /**
       * Offset in milliseconds relative to the pipeline start (start request). If the offset is
       * not defined then the stream will be synchronized based on the delivery time of the initial
       * frames.
       */
      offset_ms?: number | null;
    }
  | {
      type: "mp4";
      /**
       * URL of the MP4 file.
       */
      url?: string | null;
      /**
       * Path to the MP4 file.
       */
      path?: string | null;
      /**
       * (**default=`false`**) If input should be played in the loop. <span class="badge badge--primary">Added in v0.4.0</span>
       */
      loop?: boolean | null;
      /**
       * (**default=`false`**) If input is required and frames are not processed
       * on time, then LiveCompositor will delay producing output frames.
       */
      required?: boolean | null;
      /**
       * Offset in milliseconds relative to the pipeline start (start request). If offset is
       * not defined then stream is synchronized based on the first frames delivery time.
       */
      offset_ms?: number | null;
      /**
       * (**default=`ffmpeg_h264`**) The decoder to use for decoding video.
       */
      video_decoder?: VideoDecoder | null;
    }
  | {
      type: "whip";
      /**
       * Parameters of a video source included in the RTP stream.
       */
      video?: InputWhipVideoOptions | null;
      /**
       * Parameters of an audio source included in the RTP stream.
       */
      audio?: InputWhipAudioOptions | null;
      /**
       * (**default=`false`**) If input is required and the stream is not delivered
       * on time, then LiveCompositor will delay producing output frames.
       */
      required?: boolean | null;
      /**
       * Offset in milliseconds relative to the pipeline start (start request). If the offset is
       * not defined then the stream will be synchronized based on the delivery time of the initial
       * frames.
       */
      offset_ms?: number | null;
    }
  | {
      type: "decklink";
      /**
       * Single DeckLink device can consist of multiple sub-devices. This field defines
       * index of sub-device that should be used.
       *
       * The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
       * All of them need to match the device if they are specified. If nothing is matched, the error response
       * will list available devices.
       */
      subdevice_index?: number | null;
      /**
       * Select sub-device to use based on the display name. This is the value you see in e.g.
       * Blackmagic Media Express app. like "DeckLink Quad HDMI Recorder (3)"
       *
       * The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
       * All of them need to match the device if they are specified. If nothing is matched, the error response
       * will list available devices.
       */
      display_name?: string | null;
      /**
       * Persistent ID of a device represented by 32-bit hex number. Each DeckLink sub-device has a separate id.
       *
       * The input device is selected based on fields `subdevice_index`, `persistent_id` **AND** `display_name`.
       * All of them need to match the device if they are specified. If nothing is matched, the error response
       * will list available devices.
       */
      persistent_id?: string | null;
      /**
       * (**default=`true`**) Enable audio support.
       */
      enable_audio?: boolean | null;
      /**
       * (**default=`false`**) If input is required and frames are not processed
       * on time, then LiveCompositor will delay producing output frames.
       */
      required?: boolean | null;
    };
export type PortOrPortRange = string | number;
export type TransportProtocol = "udp" | "tcp_server";
export type VideoDecoder = "ffmpeg_h264" | "vulkan_video";
export type InputRtpAudioOptions =
  | {
      decoder: "opus";
      /**
       * (**default=`false`**) Specifies whether the stream uses forward error correction.
       * It's specific for Opus codec.
       * For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
       */
      forward_error_correction?: boolean | null;
    }
  | {
      decoder: "aac";
      /**
       * AudioSpecificConfig as described in MPEG-4 part 3, section 1.6.2.1
       * The config should be encoded as described in [RFC 3640](https://datatracker.ietf.org/doc/html/rfc3640#section-4.1).
       *
       * The simplest way to obtain this value when using ffmpeg to stream to the compositor is
       * to pass the additional `-sdp_file FILENAME` option to ffmpeg. This will cause it to
       * write out an sdp file, which will contain this field. Programs which have the ability
       * to stream AAC to the compositor should provide this information.
       *
       * In MP4 files, the ASC is embedded inside the esds box (note that it is not the whole
       * box, only a part of it). This also applies to fragmented MP4s downloaded over HLS, if
       * the playlist uses MP4s instead of MPEG Transport Streams
       *
       * In FLV files and the RTMP protocol, the ASC can be found in the `AACAUDIODATA` tag.
       */
      audio_specific_config: string;
      /**
       * (**default=`"high_bitrate"`**)
       * Specifies the [RFC 3640 mode](https://datatracker.ietf.org/doc/html/rfc3640#section-3.3.1)
       * that should be used when depacketizing this stream.
       */
      rtp_mode?: AacRtpMode | null;
    };
export type AacRtpMode = "low_bitrate" | "high_bitrate";
export type InputWhipAudioOptions = {
  decoder: "opus";
  /**
   * (**default=`false`**) Specifies whether the stream uses forward error correction.
   * It's specific for Opus codec.
   * For more information, check out [RFC](https://datatracker.ietf.org/doc/html/rfc6716#section-2.1.7).
   */
  forward_error_correction?: boolean | null;
};
export type RegisterOutput =
  | {
      type: "rtp_stream";
      /**
       * Depends on the value of the `transport_protocol` field:
       * - `udp` - An UDP port number that RTP packets will be sent to.
       * - `tcp_server` - A local TCP port number or a port range that LiveCompositor will listen for incoming connections.
       */
      port: PortOrPortRange;
      /**
       * Only valid if `transport_protocol="udp"`. IP address where RTP packets should be sent to.
       */
      ip?: string | null;
      /**
       * (**default=`"udp"`**) Transport layer protocol that will be used to send RTP packets.
       */
      transport_protocol?: TransportProtocol | null;
      /**
       * Video stream configuration.
       */
      video?: OutputVideoOptions | null;
      /**
       * Audio stream configuration.
       */
      audio?: OutputRtpAudioOptions | null;
    }
  | {
      type: "mp4";
      /**
       * Path to output MP4 file.
       */
      path: string;
      /**
       * Video track configuration.
       */
      video?: OutputVideoOptions | null;
      /**
       * Audio track configuration.
       */
      audio?: OutputMp4AudioOptions | null;
    }
  | {
      type: "whip";
      /**
       * WHIP server endpoint
       */
      endpoint_url: string;
      bearer_token?: string | null;
      /**
       * Video track configuration.
       */
      video?: OutputVideoOptions | null;
      /**
       * Audio track configuration.
       */
      audio?: OutputWhipAudioOptions | null;
    };
export type InputId = string;
export type VideoEncoderOptions = {
  type: "ffmpeg_h264";
  /**
   * (**default=`"fast"`**) Preset for an encoder. See `FFmpeg` [docs](https://trac.ffmpeg.org/wiki/Encode/H.264#Preset) to learn more.
   */
  preset?: H264EncoderPreset | null;
  /**
   * Raw FFmpeg encoder options. See [docs](https://ffmpeg.org/ffmpeg-codecs.html) for more.
   */
  ffmpeg_options?: {
    [k: string]: string;
  } | null;
};
export type H264EncoderPreset =
  | "ultrafast"
  | "superfast"
  | "veryfast"
  | "faster"
  | "fast"
  | "medium"
  | "slow"
  | "slower"
  | "veryslow"
  | "placebo";
export type Component =
  | {
      type: "input_stream";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * Id of an input. It identifies a stream registered using a [`RegisterInputStream`](../routes.md#register-input) request.
       */
      input_id: InputId;
    }
  | {
      type: "view";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * List of component's children.
       */
      children?: Component[] | null;
      /**
       * Width of a component in pixels (without a border). Exact behavior might be different
       * based on the parent component:
       * - If the parent component is a layout, check sections "Absolute positioning" and "Static
       * positioning" of that component.
       * - If the parent component is not a layout, then this field is required.
       */
      width?: number | null;
      /**
       * Height of a component in pixels (without a border). Exact behavior might be different
       * based on the parent component:
       * - If the parent component is a layout, check sections "Absolute positioning" and "Static
       * positioning" of that component.
       * - If the parent component is not a layout, then this field is required.
       */
      height?: number | null;
      /**
       * Direction defines how static children are positioned inside a View component.
       */
      direction?: ViewDirection | null;
      /**
       * Distance in pixels between this component's top edge and its parent's top edge (including a border).
       * If this field is defined, then the component will ignore a layout defined by its parent.
       */
      top?: number | null;
      /**
       * Distance in pixels between this component's left edge and its parent's left edge (including a border).
       * If this field is defined, this element will be absolutely positioned, instead of being
       * laid out by its parent.
       */
      left?: number | null;
      /**
       * Distance in pixels between the bottom edge of this component and the bottom edge of its
       * parent (including a border). If this field is defined, this element will be absolutely
       * positioned, instead of being laid out by its parent.
       */
      bottom?: number | null;
      /**
       * Distance in pixels between this component's right edge and its parent's right edge.
       * If this field is defined, this element will be absolutely positioned, instead of being
       * laid out by its parent.
       */
      right?: number | null;
      /**
       * Rotation of a component in degrees. If this field is defined, this element will be
       * absolutely positioned, instead of being laid out by its parent.
       */
      rotation?: number | null;
      /**
       * Defines how this component will behave during a scene update. This will only have an
       * effect if the previous scene already contained a `View` component with the same id.
       */
      transition?: Transition | null;
      /**
       * (**default=`"hidden"`**) Controls what happens to content that is too big to fit into an area.
       */
      overflow?: Overflow | null;
      /**
       * (**default=`"#00000000"`**) Background color in a `"#RRGGBBAA"` format.
       */
      background_color?: RGBAColor | null;
      /**
       * (**default=`0.0`**) Radius of a rounded corner.
       */
      border_radius?: number | null;
      /**
       * (**default=`0.0`**) Border width.
       */
      border_width?: number | null;
      /**
       * (**default=`"#00000000"`**) Border color in a `"#RRGGBBAA"` format.
       */
      border_color?: RGBAColor | null;
      /**
       * List of box shadows.
       */
      box_shadow?: BoxShadow[] | null;
    }
  | {
      type: "web_view";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * List of component's children.
       */
      children?: Component[] | null;
      /**
       * Id of a web renderer instance. It identifies an instance registered using a
       * [`register web renderer`](../routes.md#register-web-renderer-instance) request.
       *
       * :::warning
       * You can only refer to specific instances in one Component at a time.
       * :::
       */
      instance_id: RendererId;
    }
  | {
      type: "shader";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * List of component's children.
       */
      children?: Component[] | null;
      /**
       * Id of a shader. It identifies a shader registered using a [`register shader`](../routes.md#register-shader) request.
       */
      shader_id: RendererId;
      /**
       * Object that will be serialized into a `struct` and passed inside the shader as:
       *
       * ```wgsl
       * @group(1) @binding(0) var<uniform>
       * ```
       * :::note
       * This object's structure must match the structure defined in a shader source code.
       * Currently, we do not handle memory layout automatically. To achieve the correct memory
       * alignment, you might need to pad your data with additional fields. See
       * [WGSL documentation](https://www.w3.org/TR/WGSL/#alignment-and-size) for more details.
       * :::
       */
      shader_param?: ShaderParam | null;
      /**
       * Resolution of a texture where shader will be executed.
       */
      resolution: Resolution;
    }
  | {
      type: "image";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * Id of an image. It identifies an image registered using a [`register image`](../routes.md#register-image) request.
       */
      image_id: RendererId;
    }
  | {
      type: "text";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * Text that will be rendered.
       */
      text: string;
      /**
       * Width of a texture that text will be rendered on. If not provided, the resulting texture
       * will be sized based on the defined text but limited to `max_width` value.
       */
      width?: number | null;
      /**
       * Height of a texture that text will be rendered on. If not provided, the resulting texture
       * will be sized based on the defined text but limited to `max_height` value.
       * It's an error to provide `height` if `width` is not defined.
       */
      height?: number | null;
      /**
       * (**default=`7682`**) Maximal `width`. Limits the width of the texture that the text will be rendered on.
       * Value is ignored if `width` is defined.
       */
      max_width?: number | null;
      /**
       * (**default=`4320`**) Maximal `height`. Limits the height of the texture that the text will be rendered on.
       * Value is ignored if height is defined.
       */
      max_height?: number | null;
      /**
       * Font size in pixels.
       */
      font_size: number;
      /**
       * Distance between lines in pixels. Defaults to the value of the `font_size` property.
       */
      line_height?: number | null;
      /**
       * (**default=`"#FFFFFFFF"`**) Font color in `#RRGGBBAA` format.
       */
      color?: RGBAColor | null;
      /**
       * (**default=`"#00000000"`**) Background color in `#RRGGBBAA` format.
       */
      background_color?: RGBAColor | null;
      /**
       * (**default=`"Verdana"`**) Font family. Provide [family-name](https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#family-name-value)
       * for a specific font. "generic-family" values like e.g. "sans-serif" will not work.
       */
      font_family?: string | null;
      /**
       * (**default=`"normal"`**) Font style. The selected font needs to support the specified style.
       */
      style?: TextStyle | null;
      /**
       * (**default=`"left"`**) Text align.
       */
      align?: HorizontalAlign | null;
      /**
       * (**default=`"none"`**) Text wrapping options.
       */
      wrap?: TextWrapMode | null;
      /**
       * (**default=`"normal"`**) Font weight. The selected font needs to support the specified weight.
       */
      weight?: TextWeight | null;
    }
  | {
      type: "tiles";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * List of component's children.
       */
      children?: Component[] | null;
      /**
       * Width of a component in pixels. Exact behavior might be different based on the parent
       * component:
       * - If the parent component is a layout, check sections "Absolute positioning" and "Static
       * positioning" of that component.
       * - If the parent component is not a layout, then this field is required.
       */
      width?: number | null;
      /**
       * Height of a component in pixels. Exact behavior might be different based on the parent
       * component:
       * - If the parent component is a layout, check sections "Absolute positioning" and "Static
       * positioning" of that component.
       * - If the parent component is not a layout, then this field is required.
       */
      height?: number | null;
      /**
       * (**default=`"#00000000"`**) Background color in a `"#RRGGBBAA"` format.
       */
      background_color?: RGBAColor | null;
      /**
       * (**default=`"16:9"`**) Aspect ratio of a tile in `"W:H"` format, where W and H are integers.
       */
      tile_aspect_ratio?: AspectRatio | null;
      /**
       * (**default=`0`**) Margin of each tile in pixels.
       */
      margin?: number | null;
      /**
       * (**default=`0`**) Padding on each tile in pixels.
       */
      padding?: number | null;
      /**
       * (**default=`"center"`**) Horizontal alignment of tiles.
       */
      horizontal_align?: HorizontalAlign | null;
      /**
       * (**default=`"center"`**) Vertical alignment of tiles.
       */
      vertical_align?: VerticalAlign | null;
      /**
       * Defines how this component will behave during a scene update. This will only have an
       * effect if the previous scene already contained a `Tiles` component with the same id.
       */
      transition?: Transition | null;
      border_radius?: number | null;
    }
  | {
      type: "rescaler";
      /**
       * Id of a component.
       */
      id?: ComponentId | null;
      /**
       * List of component's children.
       */
      child: Component;
      /**
       * (**default=`"fit"`**) Resize mode:
       */
      mode?: RescaleMode | null;
      /**
       * (**default=`"center"`**) Horizontal alignment.
       */
      horizontal_align?: HorizontalAlign | null;
      /**
       * (**default=`"center"`**) Vertical alignment.
       */
      vertical_align?: VerticalAlign | null;
      /**
       * Width of a component in pixels (without a border). Exact behavior might be different
       * based on the parent component:
       * - If the parent component is a layout, check sections "Absolute positioning" and "Static
       * positioning" of that component.
       * - If the parent component is not a layout, then this field is required.
       */
      width?: number | null;
      /**
       * Height of a component in pixels (without a border). Exact behavior might be different
       * based on the parent component:
       * - If the parent component is a layout, check sections "Absolute positioning" and "Static
       * positioning" of that component.
       * - If the parent component is not a layout, then this field is required.
       */
      height?: number | null;
      /**
       * Distance in pixels between this component's top edge and its parent's top edge (including a border).
       * If this field is defined, then the component will ignore a layout defined by its parent.
       */
      top?: number | null;
      /**
       * Distance in pixels between this component's left edge and its parent's left edge (including a border).
       * If this field is defined, this element will be absolutely positioned, instead of being
       * laid out by its parent.
       */
      left?: number | null;
      /**
       * Distance in pixels between the bottom edge of this component and the bottom edge of its
       * parent (including a border). If this field is defined, this element will be absolutely
       * positioned, instead of being laid out by its parent.
       */
      bottom?: number | null;
      /**
       * Distance in pixels between this component's right edge and its parent's right edge.
       * If this field is defined, this element will be absolutely positioned, instead of being
       * laid out by its parent.
       */
      right?: number | null;
      /**
       * Rotation of a component in degrees. If this field is defined, this element will be
       * absolutely positioned, instead of being laid out by its parent.
       */
      rotation?: number | null;
      /**
       * Defines how this component will behave during a scene update. This will only have an
       * effect if the previous scene already contained a `Rescaler` component with the same id.
       */
      transition?: Transition | null;
      /**
       * (**default=`0.0`**) Radius of a rounded corner.
       */
      border_radius?: number | null;
      /**
       * (**default=`0.0`**) Border width.
       */
      border_width?: number | null;
      /**
       * (**default=`"#00000000"`**) Border color in a `"#RRGGBBAA"` format.
       */
      border_color?: RGBAColor | null;
      /**
       * List of box shadows.
       */
      box_shadow?: BoxShadow[] | null;
    };
export type ComponentId = string;
export type ViewDirection = "row" | "column";
/**
 * Easing functions are used to interpolate between two values over time.
 *
 * Custom easing functions can be implemented with cubic Bézier.
 * The control points are defined with `points` field by providing four numerical values: `x1`, `y1`, `x2` and `y2`. The `x1` and `x2` values have to be in the range `[0; 1]`. The cubic Bézier result is clamped to the range `[0; 1]`.
 * You can find example control point configurations [here](https://easings.net/).
 */
export type EasingFunction =
  | {
      function_name: "linear";
    }
  | {
      function_name: "bounce";
    }
  | {
      function_name: "cubic_bezier";
      /**
       * @minItems 4
       * @maxItems 4
       */
      points: [number, number, number, number];
    };
export type Overflow = "visible" | "hidden" | "fit";
export type RGBAColor = string;
export type RendererId = string;
export type ShaderParam =
  | {
      type: "f32";
      value: number;
    }
  | {
      type: "u32";
      value: number;
    }
  | {
      type: "i32";
      value: number;
    }
  | {
      type: "list";
      value: ShaderParam[];
    }
  | {
      type: "struct";
      value: ShaderParamStructField[];
    };
export type ShaderParamStructField = {
  field_name: string;
} & ShaderParamStructField1;
export type ShaderParamStructField1 =
  | {
      type: "f32";
      value: number;
      field_name?: string;
    }
  | {
      type: "u32";
      value: number;
      field_name?: string;
    }
  | {
      type: "i32";
      value: number;
      field_name?: string;
    }
  | {
      type: "list";
      value: ShaderParam[];
      field_name?: string;
    }
  | {
      type: "struct";
      value: ShaderParamStructField[];
      field_name?: string;
    };
export type TextStyle = "normal" | "italic" | "oblique";
export type HorizontalAlign = "left" | "right" | "justified" | "center";
export type TextWrapMode = "none" | "glyph" | "word";
/**
 * Font weight, based on the [OpenType specification](https://learn.microsoft.com/en-gb/typography/opentype/spec/os2#usweightclass).
 */
export type TextWeight =
  | "thin"
  | "extra_light"
  | "light"
  | "normal"
  | "medium"
  | "semi_bold"
  | "bold"
  | "extra_bold"
  | "black";
export type AspectRatio = string;
export type VerticalAlign = "top" | "center" | "bottom" | "justified";
export type RescaleMode = "fit" | "fill";
export type MixingStrategy = "sum_clip" | "sum_scale";
export type RtpAudioEncoderOptions = {
  type: "opus";
  /**
   * Specifies channels configuration.
   */
  channels: AudioChannels;
  /**
   * (**default="voip"**) Specifies preset for audio output encoder.
   */
  preset?: OpusEncoderPreset | null;
  /**
   * (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
   */
  sample_rate?: number | null;
};
export type AudioChannels = "mono" | "stereo";
export type OpusEncoderPreset = "quality" | "voip" | "lowest_latency";
export type Mp4AudioEncoderOptions = {
  type: "aac";
  channels: AudioChannels;
  /**
   * (**default=`44100`**) Sample rate. Allowed values: [8000, 16000, 24000, 44100, 48000].
   */
  sample_rate?: number | null;
};
export type WhipAudioEncoderOptions = {
  type: "opus";
  /**
   * Specifies channels configuration.
   */
  channels: AudioChannels;
  /**
   * (**default="voip"**) Specifies preset for audio output encoder.
   */
  preset?: OpusEncoderPreset | null;
  /**
   * (**default=`48000`**) Sample rate. Allowed values: [8000, 16000, 24000, 48000].
   */
  sample_rate?: number | null;
};
export type ImageSpec =
  | {
      asset_type: "png";
      url?: string | null;
      path?: string | null;
    }
  | {
      asset_type: "jpeg";
      url?: string | null;
      path?: string | null;
    }
  | {
      asset_type: "svg";
      url?: string | null;
      path?: string | null;
      resolution?: Resolution | null;
    }
  | {
      asset_type: "gif";
      url?: string | null;
      path?: string | null;
    };
export type WebEmbeddingMethod =
  | "chromium_embedding"
  | "native_embedding_over_content"
  | "native_embedding_under_content";

export interface InputRtpVideoOptions {
  decoder: VideoDecoder;
}
export interface InputWhipVideoOptions {
  decoder: VideoDecoder;
}
export interface OutputVideoOptions {
  /**
   * Output resolution in pixels.
   */
  resolution: Resolution;
  /**
   * Defines when output stream should end if some of the input streams are finished. If output includes both audio and video streams, then EOS needs to be sent on both.
   */
  send_eos_when?: OutputEndCondition | null;
  /**
   * Video encoder options.
   */
  encoder: VideoEncoderOptions;
  /**
   * Root of a component tree/scene that should be rendered for the output. Use [`update_output` request](../routes.md#update-output) to update this value after registration. [Learn more](../../concept/component.md).
   */
  initial: Video;
}
export interface Resolution {
  /**
   * Width in pixels.
   */
  width: number;
  /**
   * Height in pixels.
   */
  height: number;
}
/**
 * This type defines when end of an input stream should trigger end of the output stream. Only one of those fields can be set at the time.
 * Unless specified otherwise the input stream is considered finished/ended when:
 * - TCP connection was dropped/closed.
 * - RTCP Goodbye packet (`BYE`) was received.
 * - Mp4 track has ended.
 * - Input was unregistered already (or never registered).
 */
export interface OutputEndCondition {
  /**
   * Terminate output stream if any of the input streams from the list are finished.
   */
  any_of?: InputId[] | null;
  /**
   * Terminate output stream if all the input streams from the list are finished.
   */
  all_of?: InputId[] | null;
  /**
   * Terminate output stream if any of the input streams ends. This includes streams added after the output was registered. In particular, output stream will **not be** terminated if no inputs were ever connected.
   */
  any_input?: boolean | null;
  /**
   * Terminate output stream if all the input streams finish. In particular, output stream will **be** terminated if no inputs were ever connected.
   */
  all_inputs?: boolean | null;
}
export interface Video {
  root: Component;
}
export interface Transition {
  /**
   * Duration of a transition in milliseconds.
   */
  duration_ms: number;
  /**
   * (**default=`"linear"`**) Easing function to be used for the transition.
   */
  easing_function?: EasingFunction | null;
}
export interface BoxShadow {
  offset_x?: number | null;
  offset_y?: number | null;
  color?: RGBAColor | null;
  blur_radius?: number | null;
}
export interface OutputRtpAudioOptions {
  /**
   * (**default="sum_clip"**) Specifies how audio should be mixed.
   */
  mixing_strategy?: MixingStrategy | null;
  /**
   * Condition for termination of output stream based on the input streams states.
   */
  send_eos_when?: OutputEndCondition | null;
  /**
   * Audio encoder options.
   */
  encoder: RtpAudioEncoderOptions;
  /**
   * Initial audio mixer configuration for output.
   */
  initial: Audio;
}
export interface Audio {
  inputs: InputAudio[];
}
export interface InputAudio {
  input_id: InputId;
  /**
   * (**default=`1.0`**) float in `[0, 1]` range representing input volume
   */
  volume?: number | null;
}
export interface OutputMp4AudioOptions {
  /**
   * (**default="sum_clip"**) Specifies how audio should be mixed.
   */
  mixing_strategy?: MixingStrategy | null;
  /**
   * Condition for termination of output stream based on the input streams states.
   */
  send_eos_when?: OutputEndCondition | null;
  /**
   * Audio encoder options.
   */
  encoder: Mp4AudioEncoderOptions;
  /**
   * Initial audio mixer configuration for output.
   */
  initial: Audio;
}
export interface OutputWhipAudioOptions {
  /**
   * (**default="sum_clip"**) Specifies how audio should be mixed.
   */
  mixing_strategy?: MixingStrategy | null;
  /**
   * Condition for termination of output stream based on the input streams states.
   */
  send_eos_when?: OutputEndCondition | null;
  /**
   * Audio encoder options.
   */
  encoder: WhipAudioEncoderOptions;
  /**
   * Initial audio mixer configuration for output.
   */
  initial: Audio;
}
export interface WebRendererSpec {
  /**
   * Url of a website that you want to render.
   */
  url: string;
  /**
   * Resolution.
   */
  resolution: Resolution;
  /**
   * Mechanism used to render input frames on the website.
   */
  embedding_method?: WebEmbeddingMethod | null;
}
export interface ShaderSpec {
  /**
   * Shader source code. [Learn more.](../../concept/shaders)
   */
  source: string;
}
export interface UpdateOutputRequest {
  video?: Video | null;
  audio?: Audio | null;
  schedule_time_ms?: number | null;
}
