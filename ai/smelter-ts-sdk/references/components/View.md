# View Component

The `View` component is the core layout mechanism in Smelter, analogous to `<div>` in HTML. It acts as a container with styling and can be composed/nested.

## Type Definition

```tsx
type ViewProps = {
    id?: string;
    children?: ReactNode;
    style?: ViewStyleProps;
    transition?: Transition;
}
```

## Props

### children
Content to display inside the View.
- **Type**: `ReactNode`

### id
Component ID (used for transitions between scene updates).
- **Type**: `string`
- **Default**: Value from `useId` hook

### style
Visual styling.
- **Type**: `ViewStyleProps` — see `references/props/ViewStyleProps.md`

### transition
Animation behavior during scene updates. Requires matching `id` in both old and new scene.
- **Type**: `Transition` — see `references/props/Transition.md`
- Supported animated fields: `width`, `height`, `top`, `bottom`, `left`, `right`, `rotation`

## Positioning

**Absolute**: Set `top`, `left`, `right`, `bottom`, or `rotation` — positions relative to parent. If `width`/`height` omitted, inherits from parent.

**Static**: Children without absolute positioning are placed side-by-side (row or column based on `direction`).

> **Note**: The parent `View` does not expand to fit children by default.
