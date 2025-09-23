import type { ChildProcess, SpawnOptions } from 'child_process';
import { spawn as nodeSpawn } from 'child_process';
import { sleep } from './utils';

export interface SpawnPromise extends Promise<void> {
  child: ChildProcess;
}

export function spawn(command: string, args: string[], options: SpawnOptions): SpawnPromise {
  const child = nodeSpawn(command, args, {
    stdio: 'inherit',
    ...options,
  });
  let stdout: string[] = [];
  let stderr: string[] = [];
  const promise = new Promise((res, rej) => {
    child.on('error', err => {
      rej(err);
    });
    child.on('exit', code => {
      if (code === 0) {
        res();
      } else {
        let err = new Error(
          `Command "${command} ${args.join(' ')}" failed with exit code ${code}.`
        );
        (err as any).stdout = stdout.length > 0 ? stdout.join('\n') : undefined;
        (err as any).stderr = stderr.length > 0 ? stderr.join('\n') : undefined;
        rej(err);
      }
    });
    child.stdout?.on('data', chunk => {
      if (stdout.length >= 100) {
        stdout.shift();
      }
      stdout.push(chunk.toString());
    });
    child.stderr?.on('data', chunk => {
      if (stderr.length >= 100) {
        stderr.shift();
      }
      stderr.push(chunk.toString());
    });
  }) as SpawnPromise;
  promise.child = child;
  return promise;
}

export function spawn_ffmpeg(options: SpawnOptions): Promise<{ stdout?: string; stderr?: string }> {
  const child = nodeSpawn('ffmpeg', ['-version'], {
    stdio: 'pipe',
    ...options,
  });
  let stdout: string = '';
  let stderr: string = '';
  const promise = new Promise((res, rej) => {
    child.on('error', err => {
      rej(err);
    });
    child.on('exit', code => {
      if (code === 0) {
        res({ stdout, stderr });
      } else {
        let err = new Error(`FFmpeg failed with exit code ${code}.`);
        (err as any).stdout = stdout;
        (err as any).stderr = stderr;
        rej(err);
      }
    });
    child.stdout?.on('data', chunk => {
      stdout += chunk.toString();
    });
    child.stderr?.on('data', chunk => {
      stderr += chunk.toString();
    });
  }) as Promise<{ stdout?: string; stderr?: string }>;
  return promise;
}

export async function killProcess(spawnPromise: SpawnPromise): Promise<void> {
  spawnPromise.child.kill('SIGINT');
  const start = Date.now();
  while (isProcessRunning(spawnPromise)) {
    if (Date.now() - start > 5000) {
      spawnPromise.child.kill('SIGKILL');
    }
    await sleep(100);
  }
}

function isProcessRunning(spawnPromise: SpawnPromise): boolean {
  try {
    return !!spawnPromise.child.kill(0);
  } catch (e: any) {
    return e.code === 'EPERM';
  }
}
