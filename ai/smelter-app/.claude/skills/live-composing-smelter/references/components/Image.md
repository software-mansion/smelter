# Image Component

Renders an image from a URL or local server path. Requires either `imageId` (pre-registered) or `source` (direct URL/path) — exactly one must be defined.

## Type Definition

```tsx
type ImageProps = {
    id?: string;
    imageId?: string;
    source?: string;
    style?: ImageStyleProps;
}

type ImageStyleProps = {
    width?: number;
    height?: number;
}
```

## Props

### imageId
ID of an image registered via `smelter.registerImage()`.
- **Type**: `string`

### source
URL or local server path to the image file.
- **Type**: `string`

### id
Component ID.
- **Type**: `string`
- **Default**: Value from `useId` hook

### style.width
Width in pixels. If omitted with `height` set, auto-adjusts to maintain aspect ratio.
- **Type**: `number`

### style.height
Height in pixels. If omitted with `width` set, auto-adjusts to maintain aspect ratio.
- **Type**: `number`

## Usage Pattern

For reusable images, register first then use `imageId`:
```tsx
await smelter.registerImage("logo", { assetType: "png", url: "https://example.com/logo.png" });
// In component:
<Image imageId="logo" style={{ width: 200 }} />
```

For one-off images, use `source` directly:
```tsx
<Image source="https://example.com/photo.jpg" style={{ width: 400, height: 300 }} />
```
