import type { Framerate } from './compositor/compositor';

export type Interval = ReturnType<typeof setInterval>;
export type Timeout = ReturnType<typeof setTimeout>;

export function assert<T>(value: T, msg?: string): asserts value {
  if (!value) {
    if (msg) {
      throw new Error(msg);
    } else {
      throw new Error('Assertion failed');
    }
  }
}

export async function sleep(timeoutMs: number): Promise<void> {
  await new Promise<void>(res => {
    setTimeout(() => {
      res();
    }, timeoutMs);
  });
}

export function framerateToDurationMs(framerate: Framerate): number {
  return (1000 * framerate.den) / framerate.num;
}

export async function downloadToArrayBuffer(url: string): Promise<ArrayBuffer> {
  const response = await fetch(url);
  return await response.arrayBuffer();
}
