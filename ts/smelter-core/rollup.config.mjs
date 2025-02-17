import typescript from '@rollup/plugin-typescript';
import { dts } from 'rollup-plugin-dts';
import resolve from '@rollup/plugin-node-resolve'
import commonjs from '@rollup/plugin-commonjs';

export default [
  {
    input: 'src/index.ts',
    output: [
      {
        file: 'dist/cjs/index.cjs',
        format: "cjs",
      },
      {
        file: 'dist/esm/index.mjs',
        format: "esm",
      },
    ],
    plugins: [
      resolve(),
      commonjs(),
      typescript(),
    ],
    external: [
      'react',
      '@swmansion/smelter'
    ]
  },
  {
    input: './src/index.ts',
    output: {
      file: 'dist/index.d.ts',
      format: 'es',
    },
    plugins: [dts()],
  },
];
