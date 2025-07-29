import Smelter from '@swmansion/smelter-node';
import App from './App';
import fs from 'node:fs/promises';

export const smelter = new Smelter();

export async function initializeSmelterInstance() {
  await smelter.init();

  // Shaders

  await registerShaderFromFile('fade-in', './shaders/fade-in.wgsl');
  await registerShaderFromFile('ascii-filter', './shaders/ascii-filter.wgsl');

  // Fonts

  const fontBuffer = await fs.readFile('./assets/JetBrainsMonoNL-Regular.ttf');

  const fontArrayBuffer = fontBuffer.buffer.slice(
    fontBuffer.byteOffset,
    fontBuffer.byteOffset + fontBuffer.byteLength
  ) as ArrayBuffer;

  await smelter.registerFont(fontArrayBuffer);

  // Input

  await smelter.registerInput('bunny', {
    type: 'mp4',
    serverPath: './assets/bunny.mp4',
  });

  // Output

  await smelter.registerOutput('broadcast_box', <App />, {
    type: 'whip',
    endpointUrl: 'http://localhost:8080/api/whip',
    bearerToken: 'example',
    video: {
      encoderPreferences: [{ type: 'ffmpeg_h264', preset: 'ultrafast' }],
      resolution: { width: 1920, height: 1080 },
    },
    audio: {
      encoderPreferences: [{ type: 'opus' }],
    },
  });

  await smelter.start();
}

// Util for loading shaders from path.
async function registerShaderFromFile(shaderId: string, file: string) {
  const source = await fs.readFile(file, { encoding: 'utf-8' });

  await smelter.registerShader(shaderId, {
    source,
  });
}
