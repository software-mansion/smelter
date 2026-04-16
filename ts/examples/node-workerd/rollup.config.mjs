import { createRequire } from 'module';
import path from 'path';
import typescript from '@rollup/plugin-typescript';
import resolve from '@rollup/plugin-node-resolve'
import commonjs from '@rollup/plugin-commonjs';
import terser from '@rollup/plugin-terser';
import json from '@rollup/plugin-json';
import alias from '@rollup/plugin-alias';
import inject from '@rollup/plugin-inject';
import { env, cloudflare, nodeless } from 'unenv';

const require = createRequire(import.meta.url);
const mockEnv = env(nodeless, cloudflare, {});
const unenvDir = path.join(path.dirname(require.resolve('unenv')), '..');

function resolveImports(imports) {
  return Object.fromEntries(
    Object.entries(imports).map(([key, value]) => {
      if (Array.isArray(value)) {
        value[0] = require.resolve(value[0]);
      } else {
        value = require.resolve(value);
      }

      return [key, value];
    })
  );
}

export default [
  {
    input: 'src/index.tsx',
    output: [
      {
        file: 'dist/index.mjs',
        format: "esm",
        inlineDynamicImports: true,
      },
    ],
    plugins: [
      alias({
        entries: resolveImports(mockEnv.alias),
      }),
      resolve({
        preferBuiltins: true,
        browser: true,
      }),
      json(),
      commonjs(),
      inject({
        ...resolveImports(mockEnv.inject),
        // workerd provides an empty `process` object and if there's already a `process` defined in `globalThis`,
        // unenv won't inject its fully mocked `process`. So we forcefully insert it, because some
        // dependencies rely on fields like `process.version`.
        process: path.join(unenvDir, 'runtime/node/process/index.mjs'),
      }),
      typescript(),
      terser(),
    ],
    treeshake: true,
    external: mockEnv.external,
  },
];
