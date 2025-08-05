import type { Renderers } from '@swmansion/smelter';
import { _smelterInternals } from '@swmansion/smelter';
import type { RegisterInputResponse } from '../api';
import { ApiClient } from '../api';
import type { SmelterManager } from '../smelterManager';
import type { RegisterOutput } from '../api/output';
import { intoRegisterOutput } from '../api/output';
import type { RegisterInput } from '../api/input';
import { intoRegisterInput } from '../api/input';
import { intoRegisterImage } from '../api/renderer';
import OfflineOutput from './output';
import { SmelterEventType, parseEvent } from '../event';
import type { ReactElement } from 'react';
import type { Logger } from 'pino';
import type { ImageRef } from '../api/image';

/**
 * Offline rendering only supports one output, so we can just pick any value to use
 * as an output ID.
 */
export const OFFLINE_OUTPUT_ID = 'offline_output';

export class OfflineSmelter {
  public readonly manager: SmelterManager;
  private api: ApiClient;
  private store: _smelterInternals.OfflineInputStreamStore<string>;
  private renderStarted: boolean = false;
  /**
   * Start and end timestamp of an inputs (if known).
   */
  private inputTimestamps: number[] = [];
  private logger: Logger;

  public constructor(manager: SmelterManager, logger: Logger) {
    this.manager = manager;
    this.api = new ApiClient(this.manager);
    this.store = new _smelterInternals.OfflineInputStreamStore();
    this.logger = logger;
  }

  public async init(): Promise<void> {
    this.checkNotStarted();
    await this.manager.setupInstance({
      aheadOfTimeProcessing: true,
      logger: this.logger.child({ element: 'connection-manager' }),
    });
  }

  public async render(root: ReactElement, request: RegisterOutput, durationMs?: number) {
    this.checkNotStarted();
    this.renderStarted = true;

    const output = new OfflineOutput(root, request, this.api, this.store, this.logger, durationMs);
    for (const inputTimestamp of this.inputTimestamps) {
      output.timeContext.addTimestamp({ timestamp: inputTimestamp });
    }
    const apiRequest = intoRegisterOutput(request, output.scene());
    await this.api.registerOutput(OFFLINE_OUTPUT_ID, apiRequest);
    await output.scheduleAllUpdates();
    // at this point all scene update requests should already be delivered

    if (durationMs) {
      await this.api.unregisterOutput(OFFLINE_OUTPUT_ID, { schedule_time_ms: durationMs });
    }

    const renderPromise = new Promise<void>((res, _rej) => {
      this.manager.registerEventListener(rawEvent => {
        const event = parseEvent(rawEvent, this.logger);
        if (
          event &&
          event.type === SmelterEventType.OUTPUT_DONE &&
          event.outputId === OFFLINE_OUTPUT_ID
        ) {
          res();
        }
      });
    });

    await this.api.start();

    await renderPromise;
    await this.manager.terminate();
  }

  public async registerInput(
    inputId: string,
    request: RegisterInput
  ): Promise<RegisterInputResponse> {
    this.checkNotStarted();
    this.logger.info({ inputId, type: request.type }, 'Register new input');

    const inputRef = { type: 'global', id: inputId } as const;
    const result = await this.api.registerInput(inputRef, intoRegisterInput(inputId, request));

    const offsetMs = 'offsetMs' in request && request.offsetMs ? request.offsetMs : 0;

    if (request.type === 'mp4' && request.loop) {
      this.store.addInput({
        inputId,
        offsetMs: request.offsetMs ?? 0,
        videoDurationMs: Infinity,
        audioDurationMs: Infinity,
      });
    } else {
      this.store.addInput({
        inputId,
        offsetMs: offsetMs ?? 0,
        videoDurationMs: result.video_duration_ms,
        audioDurationMs: result.audio_duration_ms,
      });
      if (offsetMs) {
        this.inputTimestamps.push(offsetMs);
      }
      if (result.video_duration_ms) {
        this.inputTimestamps.push((offsetMs ?? 0) + result.video_duration_ms);
      }
      if (result.audio_duration_ms) {
        this.inputTimestamps.push((offsetMs ?? 0) + result.audio_duration_ms);
      }
    }
    return result;
  }

  public async registerShader(
    shaderId: string,
    request: Renderers.RegisterShader
  ): Promise<object> {
    this.checkNotStarted();
    this.logger.info({ shaderId }, 'Register shader');
    return this.api.registerShader(shaderId, request);
  }

  public async registerImage(imageId: string, request: Renderers.RegisterImage): Promise<object> {
    this.checkNotStarted();
    this.logger.info({ imageId }, 'Register image');
    const imageRef = { type: 'global', id: imageId } as const satisfies ImageRef;

    return this.api.registerImage(imageRef, intoRegisterImage(request));
  }

  private checkNotStarted() {
    if (this.renderStarted) {
      throw new Error('Render was already started.');
    }
  }
}
