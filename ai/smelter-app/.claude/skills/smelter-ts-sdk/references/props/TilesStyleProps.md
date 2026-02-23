# TilesStyleProps

Styling properties for the `Tiles` component.

```tsx
type TilesStyleProps = {
    width?: number;
    height?: number;
    backgroundColor?: string;
    tileAspectRatio?: string;   // default: "16:9"
    margin?: number;            // default: 0
    padding?: number;           // default: 0
    horizontalAlign?: "left" | "right" | "justified" | "center";  // default: "center"
    verticalAlign?: "top" | "center" | "bottom" | "justified";    // default: "center"
}
```

## Properties

### width
Width in pixels. **Required** when parent is not a layout component.

### height
Height in pixels. **Required** when parent is not a layout component.

### backgroundColor
Background color in `#RRGGBBAA` or `#RRGGBB` format. Default: `#00000000` (transparent).

### tileAspectRatio
Aspect ratio of each tile in `W:H` format (integers). Default: `"16:9"`.

### margin
Margin around each tile in pixels. Default: `0`.

### padding
Padding inside each tile in pixels. Default: `0`.

### horizontalAlign
Horizontal alignment of tiles: `"left"`, `"right"`, `"justified"`, `"center"` (default).

### verticalAlign
Vertical alignment of tiles: `"top"`, `"center"` (default), `"bottom"`, `"justified"`.
