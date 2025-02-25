import path from 'node:path';
import { createRequire } from 'node:module';
import CopyPlugin from 'copy-webpack-plugin';

const require = createRequire(import.meta.url);

/** @type {import('next').NextConfig} */
const nextConfig =  {
  webpack: (config, { isServer }) => {
    config.plugins.push(
      new CopyPlugin({
        patterns: [
          {
            from: path.join(
              path.dirname(require.resolve('@swmansion/smelter-browser-render')),
              'smelter.wasm'
            ),
            to: 'static/chunks',
          },
        ],
      })
    );

    if (isServer) {
      config.externals = config.externals || [];
      config.externals.push('@swmansion/smelter-web-wasm');
    }

    return config;
  },
};

export default nextConfig;
