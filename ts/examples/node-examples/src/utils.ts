import path from 'path';
import fs, { mkdirp, pathExists } from 'fs-extra';
import type { ChildProcess } from 'child_process';
import { spawn as nodeSpawn } from 'child_process';
import { promisify } from 'util';
import { Readable, Stream } from 'stream';

const pipeline = promisify(Stream.pipeline);

export async function ffplayStartRtmpServerAsync(
  port: number
): Promise<{ spawn_promise: SpawnPromise }> {
  const promise = spawn('bash', [
    '-c',
    `ffmpeg -f flv -listen 1 -i rtmp://0.0.0.0:${port} -vcodec copy  -f flv - | ffplay -f flv -i -`,
  ]);
  await sleep(2000);
  return { spawn_promise: promise };
}

interface SpawnPromise extends Promise<void> {
  child: ChildProcess;
}

function spawn(command: string, args: string[]): SpawnPromise {
  console.log(`Spawning: ${command} ${args.join(' ')}`);
  const child = nodeSpawn(command, args, {
    stdio: process.env.DEBUG ? 'inherit' : 'ignore',
  });

  return new Promise<void>((resolve, reject) => {
    child.on('exit', code => {
      if (code === 0) {
        console.log(`Command "${command} ${args.join(' ')}" completed successfully.`);
        resolve();
      } else {
        const errorMessage = `Command "${command} ${args.join(' ')}" failed with exit code ${code}.`;
        console.error(errorMessage);
        reject(new Error(errorMessage));
      }
    });
  }) as SpawnPromise;
}

export async function sleep(timeoutMs: number): Promise<void> {
  await new Promise<void>(res => {
    setTimeout(() => {
      res();
    }, timeoutMs);
  });
}

const exampleAssets = [
  {
    path: 'BigBuckBunny.mp4',
    url: 'https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/BigBuckBunny720p24fps490s.mp4',
  },
  {
    path: 'ElephantsDream.mp4',
    url: 'https://github.com/smelter-labs/smelter-snapshot-tests/raw/refs/heads/main/assets/ElephantsDream720p24fps60s.mp4',
  },
];

export async function downloadAllAssets(): Promise<void> {
  const downloadDir = path.join(__dirname, '../.assets');
  await mkdirp(downloadDir);

  for (const asset of exampleAssets) {
    if (!(await pathExists(path.join(downloadDir, asset.path)))) {
      await download(asset.url, path.join(downloadDir, asset.path));
    }
  }
}

async function download(url: string, destination: string): Promise<void> {
  const response = await fetch(url, { method: 'GET' });
  if (response.status >= 400) {
    const err: any = new Error(`Request to ${url} failed. \n${response.body}`);
    err.response = response;
    throw err;
  }
  if (response.body) {
    await pipeline(Readable.fromWeb(response.body as any), fs.createWriteStream(destination));
  } else {
    throw Error(`Response with empty body.`);
  }
}
