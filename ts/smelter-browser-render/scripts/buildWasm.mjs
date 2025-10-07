import path from 'node:path';
import { spawn as nodeSpawn } from 'node:child_process';

async function build() {
  const dirName = import.meta.dirname;
  const smelterWasmCratePath = path.resolve(dirName, '../../../smelter-render-wasm');
  const outputPath = path.resolve(dirName, '../src/generated/smelter');
  const args = ['build', '--out-name', 'smelter', '--target', 'web', '--release', '-d', outputPath, smelterWasmCratePath];

  return await spawn('wasm-pack', args);
}

function spawn(command, args) {
  const child = nodeSpawn(command, args, {
    stdio: 'inherit',
  });

  return new Promise((resolve, reject) => {
    child.on('exit', code => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Command "${command} ${args.join(' ')}" failed with exit code ${code}.`));
      }
    });
  });
}

build();
