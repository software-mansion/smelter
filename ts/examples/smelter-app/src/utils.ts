import type { ChildProcess, SpawnOptions } from 'node:child_process';
import { spawn as nodeSpawn } from 'node:child_process';

export interface SpawnPromise extends Promise<{ stdout: string; stderr: string }> {
  child: ChildProcess;
}

export function spawn(command: string, args: string[], options: SpawnOptions): SpawnPromise {
  console.log('spawn', command, args);
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
        res({ stdout: stdout.join('\n'), stderr: stderr.join('\n') });
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

export function sleep(timeoutMs: number): Promise<void> {
  return new Promise<void>(res => {
    setTimeout(() => res(), timeoutMs);
  });
}
