# useBlockingTask Hook

Runs an async function and returns its result. In **offline processing**, it additionally blocks rendering for the next timestamp until the Promise resolves — ensuring async work completes before the scene advances.

Works for live processing too, but primarily intended for offline processing.

```tsx
function useBlockingTask<T>(fn: () => Promise<T>): T | undefined
```

## Arguments

### fn
Async function to execute. Called once (like `useMemo` — stable reference required).
- **Type**: `() => Promise<T>`

## Returns

The resolved value of `fn`, or `undefined` while pending.
- **Type**: `T | undefined`

## Example — Offline Processing

```tsx
function Scene() {
  // Fetch subtitle data, blocking offline rendering until loaded
  const subtitles = useBlockingTask(async () => {
    const response = await fetch("https://example.com/subtitles.json");
    return response.json();
  });

  return (
    <View style={{ width: 1920, height: 1080 }}>
      <InputStream inputId="video" />
      {subtitles && (
        <Text style={{ fontSize: 36, bottom: 80 }}>{subtitles[currentTime]}</Text>
      )}
    </View>
  );
}
```

## Offline Processing Guarantee

In offline mode, Smelter won't render frames for the next timestamp until all pending `useBlockingTask` promises resolve. This lets you load remote data, process files, or do any async work before the scene is captured.
