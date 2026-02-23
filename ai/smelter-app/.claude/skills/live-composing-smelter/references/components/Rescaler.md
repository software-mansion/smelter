# Rescaler Component

A layout component that resizes its single child to match the Rescaler's own dimensions, always preserving the child's aspect ratio.

## Type Definition

```tsx
type RescalerProps = {
  id?: string;
  children: ReactElement;
  style?: RescalerStyleProps;
  transition?: Transition;
}
```

## Props

### children (required)
Exactly one child component to resize.
- **Type**: `ReactElement`

### id
Component ID (used for transitions).
- **Type**: `string`
- **Default**: Value from `useId` hook

### style
Styling and positioning options.
- **Type**: `RescalerStyleProps` — see `references/props/RescalerStyleProps.md`

### transition
Animation behavior during scene updates. Requires matching `id` in both old and new scene.
- **Type**: `Transition` — see `references/props/Transition.md`
- Supported animated fields: `width`, `height`, `top`, `bottom`, `left`, `right`, `rotation`

## Resize Modes

Configured via `style.mode`:
- `"fit"` (default) — fits child fully inside, may leave empty space
- `"fill"` — covers entire area, may clip child

## Common Use Case

Wrapping `InputStream` or `Mp4` to fit into a specific area:
```tsx
<Rescaler style={{ width: 1920, height: 1080 }}>
  <InputStream inputId="camera" />
</Rescaler>
```
