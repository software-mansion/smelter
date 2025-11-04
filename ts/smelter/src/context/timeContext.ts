import type { Logger } from '../internal.js';

export interface BlockingTask {
  done(): void;
}

type Timeout = ReturnType<typeof setTimeout>;

export interface TimeContext {
  timestampMs(): number;

  addTimestamp(timestamp: TimestampObject): void;
  removeTimestamp(timestamp: TimestampObject): void;

  getSnapshot: () => number;
  subscribe: (onStoreChange: () => void) => () => void;
}

// Wrapped in object, so we can compare it by reference.
type TimestampObject = { timestamp: number };

export class OfflineTimeContext {
  private timestamps: TimestampObject[];
  private tasks: BlockingTask[];
  private onChange: () => void;
  private currentTimestamp: number = 0;
  private onChangeCallbacks: Set<() => void> = new Set();
  private logger: Logger;

  constructor(onChange: () => void, onTimeChange: (timestamp: number) => void, logger: Logger) {
    this.onChange = onChange;
    this.tasks = [];
    this.timestamps = [];
    this.onChangeCallbacks.add(() => {
      onTimeChange(this.currentTimestamp);
    });
    this.logger = logger;
  }

  public timestampMs(): number {
    return this.currentTimestamp;
  }

  public isBlocked(): boolean {
    return this.tasks.length !== 0;
  }

  public newBlockingTask(): BlockingTask {
    this.logger.trace('Start new blocking task');
    const task: BlockingTask = {} as any;
    task.done = () => {
      const originalLength = this.tasks.length;
      this.tasks = this.tasks.filter(t => t !== task);
      if (this.tasks.length < originalLength) {
        this.logger.trace('Blocking task finished');
      }
      if (this.tasks.length === 0) {
        this.onChange();
      }
    };
    this.tasks.push(task);
    return task;
  }

  public addTimestamp(timestamp: TimestampObject) {
    this.logger.trace({ timestampMs: timestamp.timestamp }, 'Add new timestamp to render.');
    this.timestamps.push(timestamp);
  }

  public removeTimestamp(timestamp: TimestampObject) {
    this.logger.trace({ timestampMs: timestamp.timestamp }, 'Remove timestamp to render.');
    this.timestamps = this.timestamps.filter(t => timestamp !== t);
  }

  public setNextTimestamp() {
    const next = this.timestamps.reduce(
      (acc, value) =>
        value.timestamp < acc.timestamp && value.timestamp > this.currentTimestamp ? value : acc,
      { timestamp: Infinity }
    );
    this.logger.debug({ timestampMs: next.timestamp }, 'Rendering new timestamp');
    this.currentTimestamp = next.timestamp;
    for (const cb of this.onChangeCallbacks) {
      cb();
    }
    this.logger.trace({ timestampMs: next.timestamp }, 'Callbacks for timestamp finished');
  }

  // callback for useSyncExternalStore
  public getSnapshot = (): number => {
    return this.currentTimestamp;
  };

  // callback for useSyncExternalStore
  public subscribe = (onStoreChange: () => void): (() => void) => {
    this.onChangeCallbacks.add(onStoreChange);
    return () => {
      this.onChangeCallbacks.delete(onStoreChange);
    };
  };
}

export class LiveTimeContext {
  private startTimestampMs: number = 0;
  private timestamps: Array<{ timestamp: TimestampObject; timeout?: Timeout }>;
  private onChangeCallbacks: Set<() => void> = new Set();

  constructor() {
    this.timestamps = [];
  }

  public timestampMs(): number {
    return this.startTimestampMs ? Date.now() - this.startTimestampMs : 0;
  }

  public initClock(timestamp: number) {
    this.startTimestampMs = timestamp;
  }

  public addTimestamp(timestamp: TimestampObject) {
    this.timestamps.push({ timestamp, timeout: this.scheduleChangeNotification(timestamp) });
  }

  public removeTimestamp(timestamp: TimestampObject) {
    const removed = this.timestamps.filter(t => timestamp === t.timestamp);
    this.timestamps = this.timestamps.filter(t => timestamp !== t.timestamp);
    removed.forEach(ts => clearTimeout(ts.timeout));
  }

  private scheduleChangeNotification(timestamp: TimestampObject): Timeout | undefined {
    const timeLeft = timestamp.timestamp - this.timestampMs();
    if (timeLeft < 0) {
      return;
    }
    return setTimeout(() => {
      for (const cb of this.onChangeCallbacks) {
        cb();
      }
    }, timeLeft + 100);
  }

  // callback for useSyncExternalStore
  public getSnapshot = (): number => {
    return this.timestampMs();
  };

  // callback for useSyncExternalStore
  public subscribe = (onStoreChange: () => void): (() => void) => {
    this.onChangeCallbacks.add(onStoreChange);
    return () => {
      this.onChangeCallbacks.delete(onStoreChange);
    };
  };
}
