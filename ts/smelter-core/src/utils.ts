import type { Logger } from 'pino';

type ThrottleOptions = {
  logger: Logger;
  timeoutMs: number;
};

export class ThrottledFunction {
  private fn: () => Promise<void>;
  private shouldCall: boolean = false;
  private runningPromise?: Promise<void> = undefined;
  private opts: ThrottleOptions;

  constructor(fn: () => Promise<void>, opts: ThrottleOptions) {
    this.opts = opts;
    this.fn = fn;
  }

  public scheduleCall() {
    this.shouldCall = true;
    if (this.runningPromise) {
      return;
    }
    this.runningPromise = this.doCall();
  }

  public async waitForPendingCalls(): Promise<void> {
    while (this.runningPromise) {
      await this.runningPromise;
    }
  }

  public setFn(fn: () => Promise<void>) {
    this.fn = fn;
  }

  private async doCall() {
    while (this.shouldCall) {
      const start = Date.now();
      this.shouldCall = false;

      try {
        await this.fn();
      } catch (error) {
        this.opts.logger.error(error);
      }

      const timeoutLeft = start + this.opts.timeoutMs - Date.now();
      if (timeoutLeft > 0) {
        await sleep(timeoutLeft);
      }
      this.runningPromise = undefined;
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

export class PromiseScheduler {
  private state:
    | { type: 'blocked', by: Promise<void> }
    | { type: 'running', promises: Set<Promise<void>> }
    | { type: 'finished' };

  public constructor() {
    this.state = { type: 'finished' };
  }

  async runBlocking<T>(fn: () => Promise<T>): Promise<T> {
    while (this.state.type !== 'finished') {
      if (this.state.type === 'blocked') {
        await this.state.by;
      }
      if (this.state.type === 'running') {
        await Promise.allSettled(this.state.promises);
      }
    }

    let resFn: (() => void) | undefined;
    let rejFn: ((reason?: any) => void) | undefined;
    const promise = new Promise<void>((res, rej) => {
      resFn = res;
      rejFn = rej;
    });

    try {
      this.state = { type: 'blocked', by: promise };
      return await fn();
    } catch (e) {
      if (rejFn) {
        rejFn(e);
        resFn = undefined;
      }
      throw e;
    } finally {
      this.state = { type: 'finished' };
      if (resFn) {
        resFn();
      }
    }
  }

  async run<T>(fn: () => Promise<T>): Promise<T> {
    while (this.state.type === 'blocked') {
      await this.state.by;
    }

    let resFn: (() => void) | undefined;
    let rejFn: ((reason?: any) => void) | undefined;
    const promise = new Promise<void>((res, rej) => {
      resFn = res;
      rejFn = rej;
    });

    try {
      if (this.state.type === 'running') {
        this.state.promises.add(promise);
      } else if (this.state.type === 'finished') {
        this.state = { type: 'running', promises: new Set([promise]) }
      }

      return await fn();
    } catch (e) {
      if (rejFn) {
        rejFn(e);
        resFn = undefined;
      }
      throw e;
    } finally {
      if (this.state.type === 'running' && promise) {
        this.state.promises.delete(promise);
        if (this.state.promises.size === 0) {
          this.state = { type: 'finished' };
        }
      }
      if (resFn) {
        resFn();
      }
    }
  }
};
