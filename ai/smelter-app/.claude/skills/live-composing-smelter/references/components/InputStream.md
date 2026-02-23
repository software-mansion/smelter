# InputStream Component

Displays a registered media input stream (video and/or audio). Requires pre-registration via `smelter.registerInput()` with a matching `inputId`. **IMPORTANT**: It does NOT have a `style` prop.

## Type Definition

```tsx
type InputStreamProps = {
  id?: string;
  inputId: string;
  volume?: number;
  muted?: boolean;
}
```

## Props

### inputId (required)
ID matching a stream registered via `smelter.registerInput()`.
- **Type**: `string`

### id
Component ID.
- **Type**: `string`
- **Default**: Value from `useId` hook

### volume
Audio volume (0 = silent, 1 = normal, 2 = double).
- **Type**: `number`
- **Default**: `1`
- **Range**: `[0, 2]`

### muted
Mutes the audio track.
- **Type**: `boolean`
- **Default**: `false`

## Usage Pattern

```tsx
// First register the input
await smelter.registerInput("camera1", { type: "whip_server" });

// Then use in component tree
function Scene() {
  return (
    <View style={{ width: 1920, height: 1080 }}>
      <InputStream inputId="camera1" volume={1.0} />
    </View>
  );
}
```

> Adding `<InputStream />` or `useAudioInput()` more than once for the same input will sum the configured volumes.
