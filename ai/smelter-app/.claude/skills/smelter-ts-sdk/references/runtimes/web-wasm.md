# Web WASM Runtime — @swmansion/smelter-web-wasm

Runs the entire Smelter rendering engine directly in the browser via WebAssembly. No separate server needed — everything runs locally in a Web Worker.

**Supported browsers**: Google Chrome and Chromium-based browsers only.

## Installation

```bash
npm install @swmansion/smelter-web-wasm @swmansion/smelter
```

## Required Configuration

The WASM module must be hosted on your site and its URL provided before starting:

```tsx
import { setWasmBundleUrl } from "@swmansion/smelter-web-wasm";
setWasmBundleUrl('/smelter.wasm');  // call early in app initialization
```

### Next.js Configuration

```json
// package.json — add dependency:
"copy-webpack-plugin": "12.0.2"
```

```js
// next.config.mjs
import path from 'node:path';
import { createRequire } from 'node:module';
import CopyPlugin from 'copy-webpack-plugin';

const require = createRequire(import.meta.url);

export default {
  webpack: (config, { isServer }) => {
    config.plugins.push(new CopyPlugin({
      patterns: [{
        from: path.join(
          path.dirname(require.resolve('@swmansion/smelter-browser-render')),
          'smelter.wasm'
        ),
        to: path.join(import.meta.dirname, "public"),
      }],
    }));
    config.resolve.fallback = { ...config.resolve.fallback, "compositor_web_bg.wasm": false };
    if (isServer) config.externals = [...(config.externals || []), '@swmansion/smelter-web-wasm'];
    return config;
  },
};
```

### Vite Configuration

```js
// vite.config.ts
import { viteStaticCopy } from 'vite-plugin-static-copy';
import { createRequire } from 'node:module';
import path from 'node:path';
const require = createRequire(import.meta.url);

export default defineConfig({
  plugins: [
    react(),
    viteStaticCopy({
      targets: [{
        src: path.join(path.dirname(require.resolve('@swmansion/smelter-browser-render')), 'smelter.wasm'),
        dest: 'assets',
      }],
    }),
  ],
  optimizeDeps: {
    exclude: ['@swmansion/smelter-web-wasm'],
    include: ['@swmansion/smelter-web-wasm > pino'],
  },
});
// Call setWasmBundleUrl('/assets/smelter.wasm') in app code
```

---

## Smelter Class

```tsx
import Smelter from "@swmansion/smelter-web-wasm";

const smelter = new Smelter({
  framerate?: number | { num: number; den: number };
  streamFallbackTimeoutMs: number;
});
```

### Lifecycle

1. `new Smelter(options)` — create instance
2. `await smelter.init()` — initialize WASM engine
3. Register inputs/outputs/resources
4. `await smelter.start()` — begin processing
5. Register more inputs/outputs as needed
6. `await smelter.terminate()` — shut down

### Supported Outputs (WASM-specific)

```tsx
type RegisterOutput =
  | { type: 'canvas'; video: { canvas: HTMLCanvasElement; resolution: { width: number; height: number; } }; audio?: boolean; }
  | { type: 'stream'; video: { resolution: { width: number; height: number; } }; audio?: boolean; }
  | { type: 'whip_client'; endpointUrl: string; bearerToken?: string; iceServers?: RTCConfiguration['iceServers']; video: { resolution: ...; maxBitrate?: number; }; audio?: boolean; };
```

- **canvas**: Renders to `HTMLCanvasElement`, plays audio in browser tab
- **stream**: Returns `MediaStream` for use with WebRTC or other browser APIs
- **whip_client**: Streams via WHIP protocol to a server

### Supported Inputs (WASM-specific)

```tsx
type RegisterInput =
  | { type: 'mp4'; url: string }
  | { type: 'camera' }           // getUserMedia()
  | { type: 'screen_capture' }   // getDisplayMedia()
  | { type: 'stream'; stream: MediaStream }
  | { type: 'whep_client'; endpointUrl: string; bearerToken?: string };
```

- **mp4**: URL only (no serverPath in WASM); audio NOT supported
- **camera**: Captures camera + microphone via `getUserMedia()`
- **screen_capture**: Captures screen via `getDisplayMedia()`
- **stream**: Any `MediaStream` object
- **whep_client**: Connects to WHEP server

### Key Methods

Same interface as other runtimes: `registerOutput`, `unregisterOutput`, `registerInput`, `unregisterInput`, `registerImage`, `unregisterImage`, `registerShader`, `unregisterShader`, `registerFont`

> **Note**: `registerWebRenderer` is NOT available in WASM runtime.

### WASM-Specific Shader Limitation

Shaders in WASM accept only ONE texture (`texture_2d<f32>` instead of `binding_array<texture_2d<f32>, 16>`).

---

## Compatibility

| SDK version | Supported browsers | React |
|---|---|---|
| v0.2.0, v0.2.1 | Chrome and Chromium-based | 18.3.1 (recommended) |
| v0.3.0 | Chrome and Chromium-based | 18.3.1 (recommended) |
