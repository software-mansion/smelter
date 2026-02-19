# ViewStyleProps

Styling properties for the `View` component.

```tsx
type ViewStyleProps = {
    width?: number;
    height?: number;
    direction?: "row" | "column";
    top?: number;
    left?: number;
    bottom?: number;
    right?: number;
    rotation?: number;
    overflow?: "visible" | "hidden" | "fit";
    backgroundColor?: string;
    padding?: number;
    paddingVertical?: number;
    paddingHorizontal?: number;
    paddingTop?: number;
    paddingBottom?: number;
    paddingLeft?: number;
    paddingRight?: number;
}
```

## Properties

### width
Width in pixels. **Required** when parent is not a layout component.

### height
Height in pixels. **Required** when parent is not a layout component.

### direction
How static children are positioned.
- `"row"` (default) — left to right
- `"column"` — top to bottom

### top / left / bottom / right
Distance in pixels from corresponding parent edge. Setting any of these makes the component **absolutely positioned**.

### rotation
Rotation in degrees. Makes the component absolutely positioned.

### overflow
Controls behavior when children exceed parent area.
- `"hidden"` (default) — clips content to parent bounds
- `"visible"` — renders everything including overflow
- `"fit"` — scales everything inside to fit; components with unknown sizes treated as 0 when calculating scaling factor

### backgroundColor
Background color in `#RRGGBBAA` or `#RRGGBB` format.
- **Default**: `#00000000` (transparent)

### padding / paddingVertical / paddingHorizontal / paddingTop / paddingBottom / paddingLeft / paddingRight
Padding in pixels for all or specific sides.
