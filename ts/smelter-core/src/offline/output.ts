import type { RegisterMp4Input, Renderers } from '@swmansion/smelter';
import { _smelterInternals } from '@swmansion/smelter';
import type { ReactElement } from 'react';
import { createElement } from 'react';
import type { ApiClient, Api } from '../api';
import Renderer from '../renderer';
import type { RegisterOutput } from '../api/output';
import { intoAudioInputsConfiguration } from '../api/output';
import { sleep } from '../utils';
import { OFFLINE_OUTPUT_ID } from './compositor';
import { OutputRootComponent } from '../rootComponent';
import type { Logger } from 'pino';
import type { ImageRef } from '../api/image';

type AudioContext = _smelterInternals.AudioContext;
type OfflineTimeContext = _smelterInternals.OfflineTimeContext;
type OfflineInputStreamStore<Id> = _smelterInternals.OfflineInputStreamStore<Id>;
type SmelterOutputContext = _smelterInternals.SmelterOutputContext;
type ChildrenLifetimeContext = _smelterInternals.ChildrenLifetimeContext;

type Timeout = ReturnType<typeof setTimeout>;

class OfflineOutput {
  api: ApiClient;
  outputId: string;
  audioContext: AudioContext;
  timeContext: OfflineTimeContext;
  childrenLifetimeContext: ChildrenLifetimeContext;
  internalInputStreamStore: OfflineInputStreamStore<number>;
  logger: Logger;

  durationMs?: number;
  updateTracker?: UpdateTracker;

  supportsAudio: boolean;
  supportsVideo: boolean;

  renderer: Renderer;

  constructor(
    root: ReactElement,
    registerRequest: RegisterOutput,
    api: ApiClient,
    store: OfflineInputStreamStore<string>,
    logger: Logger,
    durationMs?: number
  ) {
    this.api = api;
    this.logger = logger;
    this.outputId = OFFLINE_OUTPUT_ID;
    this.durationMs = durationMs;

    this.supportsAudio = 'audio' in registerRequest && !!registerRequest.audio;
    this.supportsVideo = 'video' in registerRequest && !!registerRequest.video;

    const onUpdate = () => this.updateTracker?.onUpdate();
    this.audioContext = new _smelterInternals.AudioContext(onUpdate);
    this.internalInputStreamStore = new _smelterInternals.OfflineInputStreamStore();
    this.timeContext = new _smelterInternals.OfflineTimeContext(
      onUpdate,
      (timestamp: number) => {
        store.setCurrentTimestamp(timestamp);
        this.internalInputStreamStore.setCurrentTimestamp(timestamp);
      },
      this.logger
    );
    this.childrenLifetimeContext = new _smelterInternals.ChildrenLifetimeContext(() => {});

    const rootElement = createElement(OutputRootComponent, {
      outputContext: new OutputContext(this, this.outputId, store),
      outputRoot: root,
      childrenLifetimeContext: this.childrenLifetimeContext,
    });

    this.renderer = new Renderer({
      rootElement,
      onUpdate,
      idPrefix: `${this.outputId}-`,
      logger: logger.child({ element: 'react-renderer' }),
    });
  }

  public scene(): { video?: Api.VideoScene; audio?: Api.AudioScene; schedule_time_ms: number } {
    const audio = this.supportsAudio
      ? intoAudioInputsConfiguration(this.audioContext.getAudioConfig())
      : undefined;
    const video = this.supportsVideo ? { root: this.renderer.scene() } : undefined;
    const schedule_time_ms = this.timeContext.timestampMs();
    if (schedule_time_ms === Infinity) {
      throw new Error('Generating a scene without timestamp.');
    }
    return {
      video,
      audio,
      schedule_time_ms,
    };
  }

  public async scheduleAllUpdates(): Promise<void> {
    this.updateTracker = new UpdateTracker(this.logger);

    while (this.timeContext.timestampMs() <= (this.durationMs ?? Infinity)) {
      while (true) {
        await waitForBlockingTasks(this.timeContext);
        await this.updateTracker.waitForRenderEnd();
        if (!this.timeContext.isBlocked()) {
          break;
        }
      }

      const scene = this.scene();
      await this.api.updateScene(this.outputId, scene);

      const timestampMs = this.timeContext.timestampMs();
      if (this.childrenLifetimeContext.isDone() && this.durationMs === undefined) {
        await this.api.unregisterOutput(OFFLINE_OUTPUT_ID, { schedule_time_ms: timestampMs });
        break;
      }

      this.timeContext.setNextTimestamp();
    }
    this.renderer.stop();
  }
}

class OutputContext implements SmelterOutputContext {
  public readonly globalInputStreamStore: _smelterInternals.InputStreamStore<string>;
  public readonly internalInputStreamStore: _smelterInternals.InputStreamStore<number>;
  public readonly audioContext: _smelterInternals.AudioContext;
  public readonly timeContext: _smelterInternals.TimeContext;
  public readonly outputId: string;
  public readonly logger: Logger;
  private output: OfflineOutput;

  constructor(
    output: OfflineOutput,
    outputId: string,
    store: _smelterInternals.InputStreamStore<string>
  ) {
    this.output = output;
    this.globalInputStreamStore = store;
    this.internalInputStreamStore = output.internalInputStreamStore;
    this.audioContext = output.audioContext;
    this.timeContext = output.timeContext;
    this.outputId = outputId;
    this.logger = output.logger;
  }

  public async registerMp4Input(
    inputId: number,
    registerRequest: RegisterMp4Input
  ): Promise<{ videoDurationMs?: number; audioDurationMs?: number }> {
    const inputRef = {
      type: 'output-specific-input',
      outputId: this.outputId,
      id: inputId,
    } as const;
    const offsetMs = this.timeContext.timestampMs();
    const { video_duration_ms: videoDurationMs, audio_duration_ms: audioDurationMs } =
      await this.output.api.registerInput(inputRef, {
        type: 'mp4',
        offset_ms: offsetMs,
        path: registerRequest.serverPath,
        url: registerRequest.url,
        required: registerRequest.required,
        video_decoder: registerRequest.videoDecoder,
      });
    this.output.internalInputStreamStore.addInput({
      inputId,
      offsetMs,
      videoDurationMs,
      audioDurationMs,
    });
    if (registerRequest.offsetMs) {
      this.timeContext.addTimestamp({ timestamp: offsetMs });
    }
    if (videoDurationMs) {
      this.timeContext.addTimestamp({
        timestamp: (registerRequest.offsetMs ?? 0) + videoDurationMs,
      });
    }
    if (audioDurationMs) {
      this.timeContext.addTimestamp({
        timestamp: (registerRequest.offsetMs ?? 0) + audioDurationMs,
      });
    }
    return {
      videoDurationMs,
      audioDurationMs,
    };
  }
  public async unregisterMp4Input(inputId: number): Promise<void> {
    const schedule_time_ms = this.timeContext.timestampMs();
    if (schedule_time_ms === Infinity) {
      return;
    }
    await this.output.api.unregisterInput(
      {
        type: 'output-specific-input',
        outputId: this.outputId,
        id: inputId,
      },
      { schedule_time_ms }
    );
  }
  public async registerImage(imageId: number, imageSpec: Renderers.RegisterImage) {
    const imageRef = {
      type: 'output-specific-image',
      outputId: this.outputId,
      id: imageId,
    } as const satisfies ImageRef;

    await this.output.api.registerImage(imageRef, {
      url: imageSpec.url,
      path: imageSpec.serverPath,
      asset_type: imageSpec.assetType,
    });
  }
  public async unregisterImage(imageId: number) {
    const schedule_time_ms = this.timeContext.timestampMs();
    if (schedule_time_ms === Infinity) {
      return;
    }
    await this.output.api.unregisterImage(
      {
        type: 'output-specific-image',
        outputId: this.outputId,
        id: imageId,
      },
      { schedule_time_ms }
    );
  }
}

async function waitForBlockingTasks(offlineContext: OfflineTimeContext): Promise<void> {
  while (offlineContext.isBlocked()) {
    await sleep(100);
  }
}

const MAX_NO_UPDATE_TIMEOUT_MS = 200;
const MAX_RENDER_TIMEOUT_MS = 2000;

/**
 * Instance that tracks updates, this utils allows us to
 * to monitor when last update happened in the react tree.
 *
 * If there were no updates for MAX_NO_UPDATE_TIMEOUT_MS or
 * MAX_RENDER_TIMEOUT_MS already passed since we started rendering
 * specific PTS then assume it's ready to grab a snapshot of a tree
 */
class UpdateTracker {
  private promise: Promise<void> = new Promise(() => {});
  private promiseRes: () => void = () => {};
  private updateTimeout?: Timeout;
  private renderTimeout?: Timeout;
  private logger: Logger;

  constructor(logger: Logger) {
    this.promise = new Promise((res, _rej) => {
      this.promiseRes = res;
    });
    this.onUpdate();
    this.logger = logger;
  }

  public onUpdate() {
    clearTimeout(this.updateTimeout);
    this.updateTimeout = setTimeout(() => {
      this.promiseRes();
    }, MAX_NO_UPDATE_TIMEOUT_MS);
  }

  public async waitForRenderEnd(): Promise<void> {
    this.promise = new Promise((res, _rej) => {
      this.promiseRes = res;
    });
    clearTimeout(this.renderTimeout);
    this.renderTimeout = setTimeout(() => {
      this.logger.warn(
        "Render for a specific timestamp took too long, make sure you don't have infinite update loop."
      );
      this.promiseRes();
    }, MAX_RENDER_TIMEOUT_MS);
    await this.promise;
    clearTimeout(this.renderTimeout);
    clearTimeout(this.updateTimeout);
  }
}

export default OfflineOutput;
