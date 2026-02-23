# WebView Component

Renders a website using Chromium embedded in the Smelter instance. Requires pre-registration via `smelter.registerWebRenderer()` and the `web-renderer` feature enabled.

> **Requirement**: Enable `SMELTER_WEB_RENDERER_ENABLE` env var and use a Smelter binary built with web rendering support (e.g., `LocallySpawnedInstanceManager({ enableWebRenderer: true })`).
>
> **Constraint**: Only ONE component can use a specific `instanceId` at a time.

## Type Definition

```tsx
type WebViewProps = {
    id?: string;
    children?: ReactElement[];
    instanceId: string;
}
```

## Props

### instanceId (required)
ID matching a web renderer registered via `smelter.registerWebRenderer()`.
- **Type**: `string`

### children
Content to display within the WebView.
- **Type**: `ReactNode`

### id
Component ID.
- **Type**: `string`
- **Default**: Value from `useId` hook

## Setup Example

```tsx
// Register with web renderer enabled
const manager = new LocallySpawnedInstanceManager({ port: 8000, enableWebRenderer: true });
const smelter = new Smelter(manager);
await smelter.init();

await smelter.registerWebRenderer("browser1", {
  url: "https://example.com",
  resolution: { width: 1920, height: 1080 },
  embeddingMethod: "chromium_embedding",
});

// Use in scene
<WebView instanceId="browser1" />
```
