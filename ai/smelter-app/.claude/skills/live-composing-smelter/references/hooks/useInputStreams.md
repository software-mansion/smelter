# useInputStreams Hook

Returns a record of all registered input streams and their current state. Use to reactively update the scene based on stream state (e.g., show placeholder when stream not yet playing).

```tsx
function useInputStreams(): Record<string, InputStreamInfo>
```

## Returns

A map from `inputId` to `InputStreamInfo`.

## InputStreamInfo Type

```tsx
type InputStreamInfo = {
  inputId: string;
  videoState?: "ready" | "playing" | "finished";
  audioState?: "ready" | "playing" | "finished";
  offsetMs?: number;
  videoDurationMs?: number;  // only for inputs that support it (e.g., mp4)
  audioDurationMs?: number;  // only for inputs that support it (e.g., mp4)
}
```

### Properties

- **inputId**: ID registered via `smelter.registerInput()`
- **videoState**: `"ready"` (received, not started), `"playing"`, `"finished"`
- **audioState**: `"ready"`, `"playing"`, `"finished"`
- **offsetMs**: Timestamp (relative to queue start) when input was added
- **videoDurationMs**: Total video duration (if available, e.g., MP4)
- **audioDurationMs**: Total audio duration (if available, e.g., MP4)

## Example

```tsx
function Scene() {
  const streams = useInputStreams();
  const camera = streams["camera1"];

  return (
    <View style={{ width: 1920, height: 1080 }}>
      {camera?.videoState === "playing" ? (
        <InputStream inputId="camera1" />
      ) : (
        <Text style={{ fontSize: 48 }}>Waiting for stream...</Text>
      )}
    </View>
  );
}
```
