# useAudioInput Hook

Controls audio configuration for an input stream without rendering the stream visually. Alternative to the `volume` and `muted` props on `<InputStream />`.

> **Warning**: Using both `<InputStream />` and `useAudioInput()` for the same input ID, or using either more than once, will **sum the volumes**.

```tsx
function useAudioInput(inputId: string, audioOptions: AudioOptions): void

type AudioOptions = {
    volume: number;  // range [0, 2]
}
```

## Arguments

### inputId
ID of an input registered via `smelter.registerInput()`.
- **Type**: `string`

### audioOptions.volume
Volume multiplier.
- **Type**: `number`
- **Range**: `[0, 2]`

## When to Use

Use `useAudioInput` when you need audio from an input stream **without** rendering it visually (no `<InputStream>` component). Example: background music that isn't part of the visual layout.

## Example

```tsx
function BackgroundAudio() {
  useAudioInput("bgmusic", { volume: 0.3 });
  return null; // no visual output
}
```
