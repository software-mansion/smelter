import { useContext, useState } from 'react';
import { SmelterContext } from './index.js';
import type { Logger } from '../types/logger.js';

let nextStreamNumber = 1;

/*
 * Generates unique input stream id that can be used in e.g. Mp4 component
 */
export function useInternalStreamId(): string {
  const ctx = useContext(SmelterContext);
  const [streamNumber, _setStreamNumber] = useState(() => {
    const result = nextStreamNumber;
    nextStreamNumber += 1;
    return result;
  });
  return `output-specific-input:${streamNumber}:${ctx.outputId}`;
}

export type StreamState = 'ready' | 'playing' | 'finished';

export type InputStreamInfo<Id> = {
  inputId: Id;
  videoState?: StreamState;
  audioState?: StreamState;
  offsetMs?: number | null;
  videoDurationMs?: number;
  audioDurationMs?: number;
};

type InstanceContext<Id = string> = Record<string, InputStreamInfo<Id>>;

export interface InputStreamStore<Id> {
  getSnapshot: () => InstanceContext<Id>;
  subscribe: (onStoreChange: () => void) => () => void;
}

type UpdateAction<Id> =
  | { type: 'update_input'; input: InputStreamInfo<Id> }
  | { type: 'add_input'; input: InputStreamInfo<Id> }
  | { type: 'remove_input'; inputId: Id };

export class LiveInputStreamStore<Id> {
  private context: Record<string, InputStreamInfo<Id>> = {};
  private onChangeCallbacks: Set<() => void> = new Set();
  private eventQueue?: UpdateAction<Id>[];
  private logger: Logger;
  private pendingPromise?: Promise<any>;

  constructor(logger: Logger) {
    this.logger = logger;
  }

  /**
   * Apply update immediately if there are no `runBlocking` calls in progress.
   * Otherwise wait for `runBlocking call to finish`.
   */
  public dispatchUpdate(update: UpdateAction<Id>) {
    if (this.eventQueue) {
      this.eventQueue.push(update);
    } else {
      this.applyUpdate(update);
    }
  }

  /**
   * No dispatch events will be processed while `fn` function executes.
   * Argument passed to the callback should be used instead of `this.dispatchUpdate`
   * to update the store from inside `fn`
   */
  public async runBlocking<T = void>(
    fn: (update: (action: UpdateAction<Id>) => void) => Promise<T>
  ): Promise<T> {
    while (this.pendingPromise) {
      await this.pendingPromise.catch(() => {});
    }
    this.eventQueue = [];
    try {
      this.pendingPromise = fn(a => this.applyUpdate(a));
      return await this.pendingPromise;
    } finally {
      this.pendingPromise = undefined;
      for (const event of this.eventQueue) {
        this.applyUpdate(event);
      }
      this.eventQueue = undefined;
    }
  }

  private applyUpdate(update: UpdateAction<Id>) {
    if (update.type === 'add_input') {
      this.addInput(update.input);
    } else if (update.type === 'update_input') {
      this.updateInput(update.input);
    } else if (update.type === 'remove_input') {
      this.removeInput(update.inputId);
    }
  }

  private addInput(input: InputStreamInfo<Id>) {
    if (this.context[String(input.inputId)]) {
      this.logger.warn(`Adding input ${input.inputId}. Input already exists.`);
    }
    this.context = { ...this.context, [String(input.inputId)]: input };
    this.signalUpdate();
  }

  private updateInput(update: InputStreamInfo<Id>) {
    const oldInput = this.context[String(update.inputId)];
    if (!oldInput) {
      this.logger.warn(`Updating input ${update.inputId}. Input does not exist.`);
      return;
    }
    this.context = {
      ...this.context,
      [String(update.inputId)]: { ...oldInput, ...update },
    };
    this.signalUpdate();
  }

  private removeInput(inputId: Id) {
    const context = { ...this.context };
    delete context[String(inputId)];
    this.context = context;
    this.signalUpdate();
  }

  private signalUpdate() {
    for (const cb of this.onChangeCallbacks) {
      cb();
    }
  }

  // callback for useSyncExternalStore
  public getSnapshot = (): InstanceContext<Id> => {
    return this.context;
  };

  // callback for useSyncExternalStore
  public subscribe = (onStoreChange: () => void): (() => void) => {
    this.onChangeCallbacks.add(onStoreChange);
    return () => {
      this.onChangeCallbacks.delete(onStoreChange);
    };
  };
}

type OfflineAddInput<Id> = {
  inputId: Id;
  offsetMs: number;
  videoDurationMs?: number;
  audioDurationMs?: number;
};

export class OfflineInputStreamStore<Id> {
  private lastTimestamp: number = 0;
  private context: InstanceContext<Id> = {};
  private inputs: OfflineAddInput<Id>[] = [];
  private onChangeCallbacks: Set<() => void> = new Set();

  public addInput(update: OfflineAddInput<Id>) {
    this.inputs.push(update);
    this.setCurrentTimestamp(this.lastTimestamp);
  }

  // TimeContext should call that function. It will always trigger re-render, but there
  // is no point to optimize it right now.
  public setCurrentTimestamp(timestampMs: number) {
    this.lastTimestamp = timestampMs;
    this.context = Object.fromEntries(
      this.inputs
        .filter(input => timestampMs >= input.offsetMs)
        .map(input => {
          // TODO: We could add "unknown" state if Mp4 duration is not known
          const inputState = {
            inputId: input.inputId,
            videoState:
              input.offsetMs + (input.videoDurationMs ?? 0) <= timestampMs ? 'finished' : 'playing',
            audioState:
              input.offsetMs + (input.audioDurationMs ?? 0) <= timestampMs ? 'finished' : 'playing',
            videoDurationMs: input.videoDurationMs,
            audioDurationMs: input.audioDurationMs,
            offsetMs: input.offsetMs,
          } as const;
          return [input.inputId, inputState];
        })
    );
    this.signalUpdate();
  }

  private signalUpdate() {
    for (const cb of this.onChangeCallbacks) {
      cb();
    }
  }

  // callback for useSyncExternalStore
  public getSnapshot = (): InstanceContext<Id> => {
    return this.context;
  };

  // callback for useSyncExternalStore
  public subscribe = (onStoreChange: () => void): (() => void) => {
    this.onChangeCallbacks.add(onStoreChange);
    return () => {
      this.onChangeCallbacks.delete(onStoreChange);
    };
  };
}
