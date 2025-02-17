import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react-swc'
import { viteStaticCopy } from 'vite-plugin-static-copy';
import { createRequire } from 'node:module';
import path from 'node:path';

const require = createRequire(import.meta.url);

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
    exclude: ['@swmansion/smelter-web-wasm'],
    include: ['pino']
  },
});
