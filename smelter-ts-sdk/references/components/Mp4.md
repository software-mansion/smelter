# Mp4 Component

Renders the content of an MP4 file directly without pre-registration. Simpler alternative to `InputStream` with fewer options.

> **Note**: `@swmansion/smelter-web-wasm` currently does NOT support audio from MP4 files.

## Type Definition

```tsx
type Mp4Props = {
  source: string;
  volume?: number;
  muted?: boolean;
}
```

## Props

### source (required)
URL or local path to the MP4 file. Path must be local to the Smelter server machine.
- **Type**: `string`

### volume
Audio volume.
- **Type**: `number`
- **Default**: `1`
- **Range**: `[0, 2]`

### muted
Mutes audio.
- **Type**: `boolean`
- **Default**: `false`

## When to Use Mp4 vs InputStream

| Use `Mp4` when... | Use `InputStream` when... |
|---|---|
| Simple one-off MP4 playback | Need looping, timing control, or offsetMs |
| No pre-registration needed | Stream source isn't an MP4 (RTP, HLS, RTMP, etc.) |
| | Need `required` flag for synchronization |
