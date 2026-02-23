# TextStyleProps

Styling properties for the `Text` component.

```tsx
type TextStyleProps = {
    width?: number;
    height?: number;
    maxWidth?: number;    // default: 7682
    maxHeight?: number;   // default: 4320
    fontSize: number;     // REQUIRED
    lineHeight?: number;
    color?: string;
    backgroundColor?: string;
    fontFamily?: string;
    fontStyle?: "normal" | "italic" | "oblique";
    align?: "left" | "right" | "justified" | "center";
    wrap?: "none" | "glyph" | "word";
    fontWeight?:
      | "thin" | "extra_light" | "light" | "normal" | "medium"
      | "semi_bold" | "bold" | "extra_bold" | "black";
}
```

## Properties

### fontSize (required)
Font size in pixels.

### width
Texture width for rendering text. If omitted, adjusts to text up to `maxWidth`.

### height
Texture height. If omitted, adjusts to text up to `maxHeight`.
> **Note**: Providing `height` without `width` causes an error.

### maxWidth
Maximum texture width. Default: `7682`. Ignored if `width` is set.

### maxHeight
Maximum texture height. Default: `4320`. Ignored if `height` is set.

### lineHeight
Distance between lines in pixels. Default: value of `fontSize`.

### color
Font color in `#RRGGBBAA` or `#RRGGBB` format. Default: `#FFFFFFFF` (white).

### backgroundColor
Background color. Default: `#00000000` (transparent).

### fontFamily
Font family name. Default: `"Verdana"`.
> **Important**: Generic families like `"sans-serif"` are NOT supported.

### fontStyle
- `"normal"` (default), `"italic"`, `"oblique"`

### align
Text alignment: `"left"` (default), `"right"`, `"justified"`, `"center"`

### wrap
Text wrapping behavior:
- `"none"` (default) — truncates overflow
- `"glyph"` — wraps at glyph level
- `"word"` — wraps at word boundaries

### fontWeight
Weight values: `"thin"` (100), `"extra_light"` (200), `"light"` (300), `"normal"` (400, default), `"medium"` (500), `"semi_bold"` (600), `"bold"` (700), `"extra_bold"` (800), `"black"` (900)
