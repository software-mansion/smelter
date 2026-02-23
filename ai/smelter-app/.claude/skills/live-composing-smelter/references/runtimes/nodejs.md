# Node.js Runtime — @swmansion/smelter-node

Controls a Smelter server from a Node.js process. React code runs in Node.js; updates are transmitted to the Smelter server via HTTP.

## Installation

```bash
npm install @swmansion/smelter-node @swmansion/smelter
```

## Smelter (Live Processing)

For dynamic, real-time scenarios. Supports adding/removing inputs and outputs at any time.

```tsx
import Smelter from "@swmansion/smelter-node";

async function run() {
  const smelter = new Smelter();  // default: LocallySpawnedInstanceManager
  await smelter.init();

  await smelter.registerOutput("out", <MyScene />, {
    type: "rtmp_client",
    url: "rtmp://example.com/app/stream_key",
    video: {
      encoder: { type: "ffmpeg_h264" },
      resolution: { width: 1920, height: 1080 },
    },
    audio: { channels: "stereo", encoder: { type: "aac" } },
  });

  await smelter.start();
  // Can register more inputs/outputs after start
}
void run();
```

### Lifecycle

1. `new Smelter(manager?)` — create instance
2. `await smelter.init()` — spawn/connect server
3. Register inputs/outputs/resources (optional before start)
4. `await smelter.start()` — begin producing streams
5. Register more inputs/outputs as needed
6. `await smelter.terminate()` — shut down

### Key Methods

| Method | Description |
|---|---|
| `registerOutput(id, root, options)` | Register an output stream with React root |
| `unregisterOutput(id)` | Stop and remove an output |
| `registerInput(id, options)` | Register an input stream |
| `unregisterInput(id)` | Remove an input |
| `registerImage(id, options)` | Register image asset |
| `registerShader(id, options)` | Register WGSL shader |
| `registerWebRenderer(id, options)` | Register web renderer instance |
| `registerFont(source)` | Register font (URL or ArrayBuffer) |

### Supported Outputs (Node.js)

`rtp_stream`, `mp4`, `hls`, `whip_client`, `whep_server`, `rtmp_client`

### Supported Inputs (Node.js)

`rtp_stream`, `mp4`, `hls`, `whip_server`, `whep_client`, `rtmp_server`

---

## OfflineSmelter (Offline Processing)

For processing static files to produce a single output. Simplified API: define inputs before start, only one output.

```tsx
import { OfflineSmelter } from "@swmansion/smelter-node";

async function run() {
  const smelter = new OfflineSmelter();
  await smelter.init();

  await smelter.registerInput("vid", { type: "mp4", serverPath: "./input.mp4" });

  await smelter.render(<MyScene />, {
    type: "mp4",
    serverPath: "./output.mp4",
    video: {
      encoder: { type: "ffmpeg_h264" },
      resolution: { width: 1920, height: 1080 },
    },
    audio: { channels: "stereo", encoder: { type: "aac" } },
  });
}
void run();
```

### Lifecycle

1. `new OfflineSmelter(manager?)` — create instance
2. `await smelter.init()` — spawn/connect server
3. Register inputs/resources
4. `await smelter.render(root, output, durationMs?)` — renders and blocks until complete

---

## SmelterManager

Controls how Node.js connects to the Smelter server.

### LocallySpawnedInstanceManager (default)

Downloads and spawns a Smelter binary locally.

```tsx
import Smelter, { LocallySpawnedInstanceManager } from "@swmansion/smelter-node";

const manager = new LocallySpawnedInstanceManager({
  port: 8000,
  workingdir?: string,        // CWD and temp downloads dir
  executablePath?: string,    // Custom binary path
  enableWebRenderer?: boolean, // default: false
});
const smelter = new Smelter(manager);
```

### ExistingInstanceManager

Connects to an already-running Smelter server.

```tsx
import Smelter, { ExistingInstanceManager } from "@swmansion/smelter-node";

const manager = new ExistingInstanceManager({
  url: "http://127.0.0.1:8000",  // http → ws, https → wss for WebSocket
});
const smelter = new Smelter(manager);
```

---

## Compatibility

| SDK version | Smelter server | React |
|---|---|---|
| v0.2.0, v0.2.1 | v0.4.0, v0.4.1 | 18.3.1 (recommended) |
| v0.3.0 | v0.5.0 | 18.3.1 (recommended) |
