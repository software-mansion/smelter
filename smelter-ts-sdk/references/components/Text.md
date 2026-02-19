# Text Component

Renders text content with configurable font, size, color, and layout options.

## Type Definition

```tsx
type TextProps = {
  id?: string;
  children?: (string | number)[] | string | number;
  style?: TextStyleProps;
}
```

## Props

### children
Text content to display.
- **Type**: `string | number | (string | number)[]`

### id
Component ID.
- **Type**: `string`
- **Default**: Value from `useId` hook

### style
Text styling properties.
- **Type**: `TextStyleProps` — see `references/props/TextStyleProps.md`

## Key Behaviors

- `fontSize` is required in `TextStyleProps`
- If only `width` is set (not `height`), texture adjusts height up to `maxHeight`
- If only `height` is set, an error occurs (height requires width)
- Default font is `"Verdana"` — generic families like `"sans-serif"` are NOT supported
- Text can be wrapped at glyph or word boundaries via `wrap` prop
- Custom fonts can be registered via `smelter.registerFont()`
