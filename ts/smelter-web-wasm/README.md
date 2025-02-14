# `@swmansion/smelter-web-wasm`

Provides API to create and manage Smelter instances for browser environment. Smelter rendering engine is compiled to WASM and
runs entirely in the browser without any additional infrastructure.

## Usage

```tsx
import { useCallback } from 'react';
import Smelter, { setWasmBundleUrl } from '@swmansion/smelter-web-wasm';
import { View, Text, InputStream } from '@swmansion/smelter';

setWasmBundleUrl('/assets/smelter.wasm'); // URL to WASM bundle

function SmelterApp() {
  return (
    <View>
      <InputStream inputId="camera-input" />
      <Text style={{ fontSize: 20 }}>Hello world</Text>
    </View>
  );
}

function BrowserApp() {
  const videoRef = useCallback(
    async (video: HTMLVideoElement | null) => {
      if (video) {
        await startSmelterInstance(video);
      }
    },
    []
  );

  return <video ref={videoRef} />
}

async function startSmelterInstance(video: HTMLVideoElement) {
  const smelter = new Smelter();
  await smelter.init();

  // register input/outputs/images/shaders/...

  await smelter.registerInput('camera-input', { type: 'camera' });

  const { stream } = await smelter.registerOutput('output', <SmelterApp />, {
    type: 'whip',
    endpointUrl: 'https://example.com/whip',
    bearerToken: '<EXAMPLE TOKEN>',
    video: {
      resolution: { width: 1920, height: 1080 },
    },
    audio: true,
  });

  await smelter.start();

  video.srcObject = stream;
  await video.play();
}
```

In this example, `BrowserApp` represents regular React website that hosts the smelter instance
and `SmelterApp` is a totally separate React tree that represents a rendered video.

The final video is both sent over WHIP protocol and the preview is displayed with `<video />` tag on
the website.

See our [docs](https://smelter.dev/docs) to learn more.

## Configuration

In the example above, we are calling `setWasmBundleUrl` that should point to the WASM bundle that
is part of the `@swmansion/smelter-browser-render` package.

For example, in vite projects you can configure with `vite.config.ts` like this:

```js
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { viteStaticCopy } from 'vite-plugin-static-copy';
import { createRequire } from 'node:module';
import path from 'node:path';

const require = createRequire(import.meta.url);

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
    viteStaticCopy({
      targets: [
        {
          src: path.join(
            path.dirname(require.resolve('@swmansion/smelter-browser-render')),
            'smelter.wasm'
          ),
          dest: 'assets',
        },
      ],
    }),
  ],
  optimizeDeps: {
    exclude: ['@rollup/browser'],
  },
});
```

## License

`@swmansion/smelter-web-wasm` is licensed under a [custom license](https://github.com/software-mansion/smelter/blob/master/LICENSE).

## Smelter is created by Software Mansion

[![swm](https://logo.swmansion.com/logo?color=white&variant=desktop&width=150&tag=smelter-github 'Software Mansion')](https://swmansion.com)

Since 2012 [Software Mansion](https://swmansion.com) is a software agency with experience in building web and mobile apps as well as complex multimedia solutions. We are Core React Native Contributors and experts in live streaming and broadcasting technologies. We can help you build your next dream product – [Hire us](https://swmansion.com/contact/projects?utm_source=smelter&utm_medium=readme).
