# Show Component

Schedules when child content should be visible, based on timestamps or a delay after mount. Primarily useful for offline processing. For live use cases, prefer `useEffect` + `setTimeout`.

## Type Definition

```tsx
type ShowProps = {
  children: ReactNode;
  timeRangeMs?: { start?: number; end?: number };
  delayMs?: number;
}
```

Either `timeRangeMs` or `delayMs` must be specified.

## Props

### children (required)
Content displayed when the time condition is met.
- **Type**: `ReactNode`

### timeRangeMs
Time range using absolute timestamps (relative to pipeline start). At least one of `start` or `end` must be defined.
- **Type**: `{ start?: number; end?: number }`

### delayMs
Duration after this component mounts before children become visible.
- **Type**: `number`

## Example

```tsx
// Show content from 5s to 10s (offline processing)
<Show timeRangeMs={{ start: 5000, end: 10000 }}>
  <Text style={{ fontSize: 48 }}>Lower Third</Text>
</Show>

// Show content 2 seconds after mount
<Show delayMs={2000}>
  <Image source="logo.png" />
</Show>
```
