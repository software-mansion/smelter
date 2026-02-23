# useAfterTimestamp Hook

Returns `true` when the specified timestamp (ms since pipeline start) has passed. Works for both live and offline processing, but primarily intended for **offline processing** where `useEffect` + `setTimeout` won't work reliably.

```tsx
function useAfterTimestamp(timestampMs: number): boolean
```

## Arguments

### timestampMs
Timestamp in milliseconds (relative to pipeline start).
- **Type**: `number`

## Returns

- `true` if the given timestamp has already passed
- `false` otherwise

## Example

```tsx
function Scene() {
  const showLowerThird = useAfterTimestamp(5000); // after 5 seconds
  const hideLowerThird = useAfterTimestamp(10000); // after 10 seconds
  const visible = showLowerThird && !hideLowerThird;

  return (
    <View style={{ width: 1920, height: 1080 }}>
      <InputStream inputId="main" />
      {visible && (
        <View style={{ bottom: 100, left: 50, width: 600, height: 80 }}>
          <Text style={{ fontSize: 36 }}>Speaker Name</Text>
        </View>
      )}
    </View>
  );
}
```

## vs. Show Component

`useAfterTimestamp` gives you a boolean for conditional rendering logic. The `<Show>` component is a declarative wrapper around the same concept.
