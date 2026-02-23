# Web Client Runtime — @swmansion/smelter-web-client

Controls a Smelter server from the browser. React runs in the browser; updates sent to an already-deployed Smelter server via HTTP. No server can be started from the browser — must provide connection URL.

## Installation

```bash
npm install @swmansion/smelter-web-client @swmansion/smelter
```

## Smelter (Live Processing)

```tsx
import Smelter from "@swmansion/smelter-web-client";

async function run() {
  const smelter = new Smelter({ url: "http://127.0.0.1:8081" });
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
}
void run();
```

### Constructor

```tsx
new Smelter(options: { url: string })
```

- `url` — HTTP URL of the running Smelter server.

### Lifecycle

1. `new Smelter({ url })` — create instance
2. `await smelter.init()` — connect and reset server state
3. Register inputs/outputs/resources
4. `await smelter.start()` — begin producing streams
5. Register more inputs/outputs as needed
6. `await smelter.terminate()` — disconnect

### Key Methods

Same as Node.js `Smelter`:
`registerOutput`, `unregisterOutput`, `registerInput`, `unregisterInput`, `registerImage`, `unregisterImage`, `registerShader`, `unregisterShader`, `registerWebRenderer`, `unregisterWebRenderer`, `registerFont`

### Supported Outputs

`rtp_stream`, `mp4`, `hls`, `whip_client`, `whep_server`, `rtmp_client`

### Supported Inputs

`rtp_stream`, `mp4`, `hls`, `whip_server`, `whep_client`, `rtmp_server`

---

## OfflineSmelter (Offline Processing)

```tsx
import { OfflineSmelter } from "@swmansion/smelter-web-client";

async function run() {
  const smelter = new OfflineSmelter({ url: "http://127.0.0.1:8081" });
  await smelter.init();
  await smelter.registerInput("vid", { type: "mp4", serverPath: "./input.mp4" });
  await smelter.render(<MyScene />, {
    type: "mp4",
    serverPath: "./output.mp4",
    video: {
      encoder: { type: "ffmpeg_h264" },
      resolution: { width: 1920, height: 1080 },
    },
  });
}
void run();
```

### Key Difference from Node.js OfflineSmelter

Constructor takes `{ url: string }` instead of a `SmelterManager`.

---

## Compatibility

| SDK version | Smelter server | React |
|---|---|---|
| v0.3.0 | v0.5.0 | 18.3.1 (recommended) |
