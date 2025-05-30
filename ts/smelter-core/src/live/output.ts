import type { RegisterMp4Input } from '@swmansion/smelter';
import { _smelterInternals } from '@swmansion/smelter';
import type { ReactElement } from 'react';
import { createElement } from 'react';
import type { ApiClient, Api } from '../api';
import Renderer from '../renderer';
import type { RegisterOutput } from '../api/output';
import { intoAudioInputsConfiguration } from '../api/output';
import { ThrottledFunction } from '../utils';
import { OutputRootComponent } from '../rootComponent';
import type { Logger } from 'pino';
import type { ImageRef } from '../api/image';

type AudioContext = _smelterInternals.AudioContext;
type LiveTimeContext = _smelterInternals.LiveTimeContext;
type LiveInputStreamStore<Id> = _smelterInternals.LiveInputStreamStore<Id>;
type SmelterOutputContext = _smelterInternals.SmelterOutputContext;

class Output {
  api: ApiClient;
  outputId: string;
  audioContext: AudioContext;
  timeContext: LiveTimeContext;
  internalInputStreamStore: LiveInputStreamStore<number>;
  logger: Logger;

  shouldUpdateWhenReady: boolean = false;
  throttledUpdate: ThrottledFunction;

  supportsAudio: boolean;
  supportsVideo: boolean;

  renderer: Renderer;

  constructor(
    outputId: string,
    root: ReactElement,
    registerRequest: RegisterOutput,
    api: ApiClient,
    store: LiveInputStreamStore<string>,
    startTimestamp: number | undefined,
    logger: Logger
  ) {
    this.api = api;
    this.logger = logger;
    this.outputId = outputId;
    this.shouldUpdateWhenReady = false;
    this.throttledUpdate = new ThrottledFunction(
      async () => {
        this.shouldUpdateWhenReady = true;
      },
      {
        timeoutMs: 30,
        logger: this.logger,
      }
    );

    this.supportsAudio = 'audio' in registerRequest && !!registerRequest.audio;
    this.supportsVideo = 'video' in registerRequest && !!registerRequest.video;

    const onUpdate = () => this.throttledUpdate.scheduleCall();
    this.audioContext = new _smelterInternals.AudioContext(onUpdate);
    this.timeContext = new _smelterInternals.LiveTimeContext();
    this.internalInputStreamStore = new _smelterInternals.LiveInputStreamStore(this.logger);
    if (startTimestamp !== undefined) {
      this.timeContext.initClock(startTimestamp);
    }

    const rootElement = createElement(OutputRootComponent, {
      outputContext: new OutputContext(this, this.outputId, store),
      outputRoot: root,
      childrenLifetimeContext: new _smelterInternals.ChildrenLifetimeContext(() => {}),
    });

    this.renderer = new Renderer({
      rootElement,
      onUpdate,
      idPrefix: `${outputId}-`,
      logger: logger.child({ element: 'react-renderer' }),
    });
  }

  public scene(): { video?: Api.VideoScene; audio?: Api.AudioScene } {
    const audio = this.supportsAudio
      ? intoAudioInputsConfiguration(this.audioContext.getAudioConfig())
      : undefined;
    const video = this.supportsVideo ? { root: this.renderer.scene() } : undefined;
    return {
      audio,
      video,
    };
  }

  public async close(): Promise<void> {
    this.throttledUpdate.setFn(async () => {});
    this.renderer.stop();
    await this.throttledUpdate.waitForPendingCalls();
  }

  public async ready() {
    this.throttledUpdate.setFn(async () => {
      await this.api.updateScene(this.outputId, this.scene());
    });
    if (this.shouldUpdateWhenReady) {
      this.throttledUpdate.scheduleCall();
    }
  }

  public initClock(timestamp: number) {
    this.timeContext.initClock(timestamp);
  }

  public inputStreamStore(): LiveInputStreamStore<number> {
    return this.internalInputStreamStore;
  }
}

class OutputContext implements SmelterOutputContext {
  public readonly globalInputStreamStore: _smelterInternals.InputStreamStore<string>;
  public readonly internalInputStreamStore: _smelterInternals.InputStreamStore<number>;
  public readonly audioContext: _smelterInternals.AudioContext;
  public readonly timeContext: _smelterInternals.TimeContext;
  public readonly outputId: string;
  public readonly logger: Logger;
  private output: Output;

  constructor(output: Output, outputId: string, store: _smelterInternals.InputStreamStore<string>) {
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
    return await this.output.internalInputStreamStore.runBlocking(async updateStore => {
      const inputRef = {
        type: 'output-specific-input',
        outputId: this.outputId,
        id: inputId,
      } as const;
      const { video_duration_ms: videoDurationMs, audio_duration_ms: audioDurationMs } =
        await this.output.api.registerInput(inputRef, {
          type: 'mp4',
          ...registerRequest,
        });
      updateStore({
        type: 'add_input',
        input: {
          inputId: inputId,
          offsetMs: registerRequest.offsetMs,
          audioDurationMs,
          videoDurationMs,
        },
      });
      return {
        audioDurationMs,
        videoDurationMs,
      };
    });
  }

  public async unregisterMp4Input(inputId: number): Promise<void> {
    await this.output.api.unregisterInput(
      {
        type: 'output-specific-input',
        outputId: this.outputId,
        id: inputId,
      },
      {}
    );
  }
  public async registerImage(imageId: number, imageSpec: any) {
    const imageRef = {
      type: 'output-specific-image',
      outputId: this.outputId,
      id: imageId,
    } as const satisfies ImageRef;

    await this.output.api.registerImage(imageRef, {
      url: imageSpec.url,
    });
  }
  public async unregisterImage(imageId: number) {
    await this.output.api.unregisterImage(
      {
        type: 'output-specific-image',
        outputId: this.outputId,
        id: imageId,
      },
      {}
    );
  }
}

export default Output;
