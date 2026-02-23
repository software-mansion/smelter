# Transition

Controls animated transitions between scene updates for `View`, `Tiles`, and `Rescaler` components. Requires matching component `id` in both old and new scene.

```tsx
type Transition = {
    durationMs: number;
    easingFunction?: EasingFunction | null;
}
```

## Properties

### durationMs (required)
Duration of the transition animation in milliseconds.
- **Type**: `number`

### easingFunction
Interpolation function for the animation.
- **Type**: `EasingFunction` — see `references/props/EasingFunction.md`
- **Default**: `"linear"`

## Supported Animated Fields (View & Rescaler)

- `width` / `height` — only within the same positioning mode
- `top` / `bottom` / `left` / `right` / `rotation` — only when the same field is defined in both old and new scene

## Transition Constraints

- Component must have the same `id` in both old and new scene
- If positioning mode changes (absolute ↔ static), transitions are not applied
- For `Tiles`, transitions control reorder/add/remove animations (not size)

## Example

```tsx
<View
  id="overlay"
  style={{ top: showOverlay ? 0 : -200, width: 400, height: 100 }}
  transition={{ durationMs: 500, easingFunction: "bounce" }}
>
  <Text style={{ fontSize: 32 }}>Breaking News</Text>
</View>
```
