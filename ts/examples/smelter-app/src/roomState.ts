import { getStreamInfo } from './TwitchApi';
import fs from 'node:fs';
import path from 'node:path';
import type { SmelterOutput } from './smelter';
import type { StoreApi } from 'zustand';
import type { RoomStore } from './store';
import { createRoomStore } from './store';

export type RoomInputState = {
  inputType: 'local-mp4' | 'live-managed' | 'live-manual';
  status: 'disconnected' | 'pending' | 'connected';
  metadata: {
    label: string;
    description: string;
  };
  removable: boolean;
} & TypeSpecificState;

type TypeSpecificState =
  | { inputType: 'local-mp4'; mp4: { filePath: string } }
  | { inputType: 'live-managed'; twitchChannelId: string };

export type RegisterInputOptions =
  | {
      type: 'twitch-channel';
      twitchChannelId: string;
    }
  | {
      type: 'kick-channel';
      twitchChannelId: string;
    };

export class RoomState {
  private inputs: Record<string, RoomInputState>;
  private idPrefix: string;
  private smelterOutput: SmelterOutput;
  private store: StoreApi<RoomStore>;

  public constructor(idPrefix: string, output: SmelterOutput) {
    this.inputs = getInitialInputState(idPrefix);
    this.idPrefix = idPrefix;
    this.smelterOutput = output;
    this.store = createRoomStore();
  }

  public getWhepUrl(): string {
    return this.smelterOutput.url;
  }

  public getState(): Record<string, RoomInputState> {
    return this.inputs;
  }

  public async registerNewInput(opts: RegisterInputOptions) {
    if (opts.type === 'twitch-channel') {
      const inputId = inputIdForTwitchInput(this.idPrefix, opts.twitchChannelId);
      if (this.inputs[inputId]) {
        throw new Error(`Input for Twitch channel ${opts.twitchChannelId} already exists.`);
      }
      const streamInfo = await getStreamInfo(opts.twitchChannelId);
      if (!streamInfo) {
        throw new Error(`Unable to find live streams for ${opts.twitchChannelId}`);
      }
      if (this.inputs[inputId]) {
        throw new Error(`Input for Twitch channel ${opts.twitchChannelId} already exists.`);
      }
      this.inputs[inputId] = {
        inputType: `live-managed`,
        status: 'disconnected',
        metadata: {
          label: `[Twitch.tv/${streamInfo.category}] ${streamInfo.displayName}`,
          description: streamInfo.title,
        },
        removable: true,
        twitchChannelId: opts.twitchChannelId,
      };
    } else if (opts.type === 'kick-channel') {
      throw new Error('Add kick support');
    }
  }

  public async connectInput(inputId: string) {
    const input = this.getInput(inputId);
    if (input.status !== 'disconnected') {
      return;
    }
    input.status = 'pending';
    this.smelterOutput
  }

  public async disconnectInput(inputId: string) {}

  private getInput(inputId: string): RoomInputState {
    const input = this.inputs[inputId];
    if (this.inputs[inputId]) {
      throw new Error(`Input for Twitch channel ${opts.twitchChannelId} already exists.`);
    }
    return input;
  }
}

function inputIdForTwitchInput(idPrefix: string, twitchChannelId: string): string {
  return `${idPrefix}::twitch::${twitchChannelId} `;
}

function getInitialInputState(idPrefix: string): Record<string, RoomInputState> {
  const inputs: Record<string, RoomInputState> = {};
  const fc25filePath = path.join(process.cwd(), `fc_25_gameplay.mp4`);
  if (fs.existsSync(fc25filePath)) {
    inputs[`${idPrefix}::local::fc_25_gameplay`] = {
      inputType: 'local-mp4',
      status: 'disconnected',
      metadata: {
        label: '[MP4] FC 25 Gameplay',
        description: '[Static source] EA Sports FC 25 Gameplay',
      },
      removable: true,
      mp4: {
        filePath: fc25filePath,
      },
    };
  }

  const nbaFilePath = path.join(process.cwd(), `fc_25_gameplay.mp4`);
  if (fs.existsSync(path.join(process.cwd(), `nba_gameplay.mp4`))) {
    inputs[`${idPrefix}::local::nba_gameplay`] = {
      inputType: 'local-mp4',
      status: 'disconnected',
      metadata: {
        label: '[MP4] NBA 2K25 Gameplay',
        description: '[Static source] NBA 2K25 Gameplay',
      },
      removable: true,
      mp4: {
        filePath: nbaFilePath,
      },
    };
  }
  return inputs;
}
