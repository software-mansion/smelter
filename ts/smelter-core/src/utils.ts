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

export class StateGuard {
  private state:
    | { type: 'unique'; promise: Promise<void> }
    | { type: 'shared'; promises: Set<Promise<void>> }
    | { type: 'open' };

  public constructor() {
    this.state = { type: 'open' };
  }

  public async runBlocking<T>(fn: () => Promise<T>): Promise<T> {
    const [promise, promiseResolveFn] = this.createGuardPromise();
    while (this.state.type !== 'open') {
      if (this.state.type === 'unique') {
        if (this.state.promise === promise) {
          break;
        }
        await this.state.promise;
      } else if (this.state.type === 'shared') {
        const unblockingPromises = this.state.promises;
        this.state = { type: 'unique', promise };
        await Promise.allSettled(unblockingPromises);
      }
    }

    this.state = { type: 'unique', promise };

    try {
      return await fn();
    } finally {
      this.state = { type: 'open' };
      promiseResolveFn();
    }
  }

  public async run<T>(fn: () => Promise<T>): Promise<T> {
    while (this.state.type === 'unique') {
      await this.state.promise;
    }

    const [promise, promiseResolveFn] = this.createGuardPromise();

    if (this.state.type === 'shared') {
      this.state.promises.add(promise);
    } else if (this.state.type === 'open') {
      this.state = { type: 'shared', promises: new Set([promise]) };
    }

    try {
      return await fn();
    } finally {
      if (this.state.type === 'shared') {
        this.state.promises.delete(promise);
        if (this.state.promises.size === 0) {
          this.state = { type: 'open' };
        }
      }

      promiseResolveFn();
    }
  }

  private createGuardPromise(): [Promise<void>, () => void] {
    let release: (() => void) | undefined;
    const promise = new Promise<void>(res => {
      release = res;
    });

    return [promise, release as () => void];
  }
}
