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
    | {
        type: 'blocked';
        blocking_promise: Promise<void>;
        nonblocking_promises?: Set<Promise<void>>;
      }
    | { type: 'running'; promises: Set<Promise<void>> }
    | { type: 'finished' };

  public constructor() {
    this.state = { type: 'finished' };
  }

  public async runBlocking<T>(fn: () => Promise<T>): Promise<T> {
    const [blocking_promise, release] = this.createGuardPromise();
    while (this.state.type !== 'finished') {
      if (this.state.type === 'blocked') {
        await Promise.allSettled(this.state?.nonblocking_promises ?? []);
        await this.state.blocking_promise;
      } else if (this.state.type === 'running') {
        this.state = {
          type: 'blocked',
          blocking_promise,
          nonblocking_promises: this.state.promises,
        };
        await Promise.allSettled(this.state?.nonblocking_promises ?? []);
      }
    }

    try {
      this.state = { type: 'blocked', blocking_promise };
      return await fn();
    } finally {
      this.state = { type: 'finished' };
      release();
    }
  }

  public async run<T>(fn: () => Promise<T>): Promise<T> {
    while (this.state.type === 'blocked') {
      await this.state.blocking_promise;
    }

    const [promise, release] = this.createGuardPromise();

    try {
      if (this.state.type === 'running') {
        this.state.promises.add(promise);
      } else if (this.state.type === 'finished') {
        this.state = { type: 'running', promises: new Set([promise]) };
      }

      return await fn();
    } finally {
      if (this.state.type === 'running') {
        this.state.promises.delete(promise);
        if (this.state.promises.size === 0) {
          this.state = { type: 'finished' };
        }
      }

      release();
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
