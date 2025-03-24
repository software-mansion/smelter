import type { OutputFrame } from '@swmansion/smelter-browser-render';

export interface OutputSink {
  send(frame: OutputFrame): Promise<void>;
}
