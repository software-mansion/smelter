import typescript from '@rollup/plugin-typescript';
import { dts } from 'rollup-plugin-dts';
import copy from 'rollup-plugin-copy';

export default [
  {
    input: 'src/index.ts',
    output: {
      file: 'dist/index.js',
      format: 'es',
    },
    plugins: [
      typescript(),
      removeCompositorWebOccurences(),
      copy({
        targets: [
          {
            src: 'src/generated/smelter/compositor_web_bg.wasm',
            dest: 'dist',
            rename: 'smelter.wasm',
          },
          {
            src: 'src/generated/LICENSE',
            dest: 'dist',
            rename: 'LICENSE-smelter-wasm-bundle',
          },
        ],
      }),
    ],
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

function removeCompositorWebOccurences() {
  return {
    name: 'remove-compositor-web-bg-occurences',
    transform(code, id) {
      if (id.includes('compositor_web.js')) {
        const new_code = code.replace(
          "module_or_path = new URL('compositor_web_bg.wasm', import.meta.url)",
          'throw new Error("WASM module path not provided")'
        );

        if (new_code === code) {
          this.error('Failed to remove \'compositor_web_bg.wasm\' path');
          return null;
        }

        return new_code;
      }
      return code;
    },
  };
}
