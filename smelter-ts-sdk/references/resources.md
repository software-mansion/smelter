# Resources Reference

Resources are registered assets (images, shaders, web renderers) that can be used by components. Register before use; unregister when done.

## Image

Used with the `<Image imageId="..." />` component.

```tsx
import { Renderers } from "@swmansion/smelter";

await smelter.registerImage("myImage", {
  assetType: "jpeg" | "png" | "gif" | "svg" | "auto",
  url?: string,         // remote URL (mutually exclusive with serverPath)
  serverPath?: string,  // local path on server (mutually exclusive with url)
});

// Later
await smelter.unregisterImage("myImage");
```

- `assetType: "auto"` — format detected from file content
- Exactly one of `url` or `serverPath` must be set

---

## Shader

Used with the `<Shader shaderId="..." />` component. Shader source must be WGSL.

```tsx
import { Renderers } from "@swmansion/smelter";

await smelter.registerShader("myShader", {
  source: string,  // WGSL shader source code
});

// Later
await smelter.unregisterShader("myShader");
```

> **WASM note**: Shader header differs for `@swmansion/smelter-web-wasm`. Shaders in WASM accept only one texture (`texture_2d<f32>` instead of `binding_array<texture_2d<f32>, 16>`).

---

## WebRenderer

Used with the `<WebView instanceId="..." />` component. Renders a website using Chromium embedded in Smelter.

> **Requirements**:
> - Set `SMELTER_WEB_RENDERER_ENABLE=true` env var
> - Use Smelter binary built with web rendering support
> - In Node.js: `new LocallySpawnedInstanceManager({ enableWebRenderer: true })`
> - NOT available in WASM runtime

```tsx
import { Renderers } from "@swmansion/smelter";

await smelter.registerWebRenderer("myBrowser", {
  url: string,                    // website URL
  resolution: { width: number; height: number },
  embeddingMethod?:
    | "chromium_embedding"             // pass frames as JS buffers (slower)
    | "native_embedding_over_content"  // overlay inputs on top of website
    | "native_embedding_under_content" // underlay inputs beneath website
});

// Later
await smelter.unregisterWebRenderer("myBrowser");
```

**Embedding methods**:
- `"native_embedding_over_content"` — site renders without inputs, inputs overlaid on top
- `"native_embedding_under_content"` — site renders without inputs, inputs underlaid beneath
- `"chromium_embedding"` — raw input frames sent as JS buffers for use in `<canvas>` (significant performance cost with many inputs)

> **Constraint**: Only one `<WebView>` component may use a specific `instanceId` at a time.

---

## Font

Register custom fonts for use with `<Text>` components.

```tsx
// Node.js and Web Client: URL or ArrayBuffer
await smelter.registerFont("https://example.com/MyFont.ttf");
await smelter.registerFont(fontArrayBuffer);

// WASM: URL only
await smelter.registerFont("https://example.com/MyFont.ttf");
```

No unregistration method — fonts persist for the session.
