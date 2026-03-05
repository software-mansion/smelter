# CLAUDE.md

## Components

Pipeline orchestration: `Pipeline` in `./src/pipeline/instance.rs`. Shared state via `PipelineCtx`.

- **Inputs (demuxers)** — `./src/pipeline/{rtp,rtmp,mp4,hls,webrtc,v4l2,decklink,channel}/`
  - RTP, RTMP server, MP4 (file/URL), HLS (FFmpeg-based), WHIP (WebRTC ingest), V4L2 (Linux), DeckLink (feature-gated), RawData (channel-based)
  - Example: `RtmpServerInput` `./src/pipeline/rtmp/rtmp_input/input.rs`

- **Decoders** — `./src/pipeline/decoder/`
  - Traits: `VideoDecoder`, `AudioDecoder` in `./src/pipeline/decoder.rs`
  - Video: ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9, vulkan_h264
  - Audio: fdk_aac, libopus

- **Queue** — `./src/queue.rs`, `./src/queue/`
  - Synchronized input frame/audio collection with timestamps relative to sync point
  - `VideoQueue`, `AudioQueue`, `QueueThread`

- **Audio Mixer** — `./src/audio_mixer/`
  - `AudioMixer` in `./src/audio_mixer/mixer.rs`
  - Per-input resampling, volume scaling, channel layout mixing

- **Encoders** — `./src/pipeline/encoder/`
  - Traits: `VideoEncoder`, `AudioEncoder` in `./src/pipeline/encoder.rs`
  - Video: ffmpeg_h264, ffmpeg_vp8, ffmpeg_vp9, vulkan_h264
  - Audio: fdk_aac, libopus

- **Outputs (muxers)** — same protocol directories as inputs
  - RTP, RTMP client, MP4, HLS, WHIP, WHEP (WebRTC egress), RawData/EncodedData (channel-based)
  - Example: `RtpOutput` `./src/pipeline/rtp/rtp_output.rs`

- **Protocol support** — `./src/pipeline/rtp/` (payloader/depayloader/jitter buffer), `./src/pipeline/webrtc/server.rs` (WHIP/WHEP HTTP endpoints), `./src/pipeline/utils/` (H.264 format conversions)


## Multimedia data flow

Input (demuxer) → Decoder → Queue → Rendering/AudioMixer → Encoder → Output (muxer) 

In most cases, each element spawns at least one thread and communicates with other elements via channels.
