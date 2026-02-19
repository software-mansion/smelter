# Inputs Reference

Inputs are registered via `smelter.registerInput(id, options)`. The `type` field determines which kind of input is registered. Use `<InputStream inputId="id" />` to display a registered input in the scene.

## Table of Contents

- [MP4](#mp4) — Node.js, Web Client, Web WASM
- [RTP](#rtp) — Node.js, Web Client
- [HLS](#hls) — Node.js
- [WHIP Server](#whip-server) — Node.js, Web Client
- [WHEP Client](#whep-client) — Node.js, Web Client
- [RTMP Server](#rtmp-server) — Node.js, Web Client (Experimental)
- [Camera (WASM)](#camera-wasm) — Web WASM only
- [Screen Capture (WASM)](#screen-capture-wasm) — Web WASM only
- [MediaStream (WASM)](#mediastream-wasm) — Web WASM only
- [WHEP Client (WASM)](#whep-client-wasm) — Web WASM only

---

## MP4

Reads static MP4 files. Supports H264 video and AAC audio. Only first video/audio tracks used.

> **WASM**: Audio from MP4 NOT supported.

```tsx
type RegisterMp4Input = {
  type: "mp4";
  url?: string;          // Node.js, WASM
  serverPath?: string;   // Node.js only
  loop?: boolean;        // Node.js only, default: false
  required?: boolean;    // Node.js only, default: false
  offsetMs?: number;     // Node.js only
  decoderMap?: { h264?: 'ffmpeg_h264' | 'vulkan_h264' };
}
```

Exactly one of `url` or `serverPath` must be defined.

---

## RTP

Streams video/audio over RTP (UDP or TCP server mode).

```tsx
type RegisterRtpInput = {
  type: "rtp_stream";
  port: string | number;  // number or "START:END" range
  transportProtocol?: "udp" | "tcp_server";  // default: udp
  video?: { decoder: "ffmpeg_h264" | "vulkan_h264" | "ffmpeg_vp8" | "ffmpeg_vp9" };
  audio?: { decoder: "opus" } | { decoder: "aac"; audioSpecificConfig: string; rtpMode?: "low_bitrate" | "high_bitrate" };
  required?: boolean;
  offsetMs?: number;
}
```

At least one of `video` or `audio` must be defined.

For AAC audio, `audioSpecificConfig` is a hex string from the SDP file. Get it with:
```bash
ffmpeg -v 0 -i input.mp4 -t 0 -vn -c:a copy -sdp_file /dev/stdout -f rtp 'rtp://127.0.0.1:1111'
```
Look for `config=<HEX_STRING>` in the output.

---

## HLS

Consumes HLS playlists.

```tsx
type RegisterHlsInput = {
  type: "hls";
  url: string;
  required?: boolean;
  offsetMs?: number;
  decoderMap?: { h264?: 'ffmpeg_h264' | 'vulkan_h264' };
}
```

---

## WHIP Server

Provides a WHIP server endpoint for incoming WebRTC streams. Smelter listens on port 9000 (configurable via `SMELTER_WHIP_WHEP_SERVER_PORT`) at `/whip/:input_id`.

```tsx
type RegisterWhipServerInput = {
  type: "whip_server";
  video?: { decoderPreferences?: ("ffmpeg_h264" | "vulkan_h264" | "ffmpeg_vp8" | "ffmpeg_vp9" | "any")[] };
  bearerToken?: string;  // auto-generated if omitted
  required?: boolean;
  offsetMs?: number;
}
```

After registration, connect to `http://localhost:9000/whip/<inputId>`.

---

## WHEP Client

Connects to a WHEP server to receive a live stream. Only Opus audio supported. Video decoder auto-negotiated if no preferences given.

```tsx
type RegisterWhepClientInput = {
  type: "whep_client";
  endpointUrl: string;
  bearerToken?: string;
  video?: { decoderPreferences?: ("ffmpeg_h264" | "vulkan_h264" | "ffmpeg_vp8" | "ffmpeg_vp9" | "any")[] };
  required?: boolean;
  offsetMs?: number;
}
```

---

## RTMP Server

Experimental. Each input starts a separate RTMP server. Push from OBS, FFmpeg, or any RTMP broadcaster.

> **Limitation**: No stream key validation, no RTMPS support. Use nginx with `nginx-rtmp-module` as a proxy in production.

```tsx
type RegisterRtmpServerInput = {
  type: "rtmp_server";
  url: string;   // e.g., "rtmp://127.0.0.1:1935"
  required?: boolean;
  offsetMs?: number;
  decoderMap?: { h264?: 'ffmpeg_h264' | 'vulkan_h264' };
}
```

---

## Camera (WASM)

Captures camera + microphone using `getUserMedia()`.

```tsx
await smelter.registerInput("cam", { type: "camera" });
```

---

## Screen Capture (WASM)

Captures screen output and audio using `getDisplayMedia()`.

```tsx
await smelter.registerInput("screen", { type: "screen_capture" });
```

---

## MediaStream (WASM)

Accepts any browser `MediaStream` object.

```tsx
const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
await smelter.registerInput("stream1", { type: "stream", stream });
```

---

## WHEP Client (WASM)

Connects to a WHEP server to receive a live media stream.

```tsx
type RegisterWhepClientInput = {
  type: "whep_client";
  endpointUrl: string;
  bearerToken?: string;
}
```

---

## Common Options

### required
When `true`, Smelter waits for this input before producing output frames.
- **Default**: `false`

### offsetMs
Timing offset relative to pipeline start. If unspecified, synced based on when first frames arrive.

### decoderMap / decoderPreferences
Controls which decoder to use:
- `"ffmpeg_h264"` — software H264 via FFmpeg
- `"vulkan_h264"` — hardware H264 via Vulkan (requires GPU support)
- `"ffmpeg_vp8"` / `"ffmpeg_vp9"` — software VP8/VP9 via FFmpeg
- `"any"` — auto-select any supported decoder
