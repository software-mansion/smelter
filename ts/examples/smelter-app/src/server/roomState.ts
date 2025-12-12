import fs from 'fs-extra';
import path from 'node:path';
import { SmelterInstance, type RegisterSmelterInputOptions, type SmelterOutput } from '../smelter';
import { hlsUrlForKickChannel, hlsUrlForTwitchChannel } from '../streamlink';
import { TwitchChannelMonitor } from '../twitch/TwitchChannelMonitor';
import { sleep } from '../utils';
import type { InputConfig, Layout } from '../app/store';
import mp4SuggestionsMonitor from '../mp4/mp4SuggestionMonitor';
import { KickChannelMonitor } from '../kick/KickChannelMonitor';
import type { ShaderConfig } from '../shaders/shaders';
import { WhipInputMonitor } from '../whip/WhipInputMonitor';

export type RoomInputState = {
  inputId: string;
  type: 'local-mp4' | 'twitch-channel' | 'kick-channel' | 'whip';
  status: 'disconnected' | 'pending' | 'connected';
  volume: number;
  showTitle: boolean;
  shaders: ShaderConfig[];
  metadata: {
    title: string;
    description: string;
  };
} & TypeSpecificState;

type TypeSpecificState =
  | { type: 'local-mp4'; mp4FilePath: string }
  | { type: 'twitch-channel'; channelId: string; hlsUrl: string; monitor: TwitchChannelMonitor }
  | { type: 'kick-channel'; channelId: string; hlsUrl: string; monitor: KickChannelMonitor }
  | { type: 'whip'; whipUrl: string; monitor: WhipInputMonitor };

type UpdateInputOptions = {
  volume: number;
  showTitle: boolean;
  shaders: ShaderConfig[];
};

export type RegisterInputOptions =
  | {
      type: 'twitch-channel';
      channelId: string;
    }
  | {
      type: 'kick-channel';
      channelId: string;
    }
  | {
      type: 'whip';
      username: string;
    }
  | {
      type: 'local-mp4';
      source: {
        fileName?: string;
        url?: string;
      };
    };

export class RoomState {
  private inputs: RoomInputState[];
  private layout: Layout = 'picture-in-picture';
  public idPrefix: string;

  private mp4sDir: string;
  private mp4Files: string[];
  private output: SmelterOutput;

  public lastReadTimestamp: number;
  public creationTimestamp: number;

  public pendingDelete?: boolean;

  public constructor(idPrefix: string, output: SmelterOutput, initInputs: RegisterInputOptions[]) {
    this.mp4sDir = path.join(process.cwd(), 'mp4s');
    this.mp4Files = mp4SuggestionsMonitor.mp4Files;
    this.inputs = [];
    this.idPrefix = idPrefix;
    this.output = output;

    this.lastReadTimestamp = Date.now();
    this.creationTimestamp = Date.now();

    void (async () => {
      await this.getInitialInputState(idPrefix, initInputs);
      const realThis = this;
      for (let i = 0; i < realThis.inputs.length; i++) {
        const maybeInput = realThis.inputs[i];
        if (maybeInput) {
          await this.connectInput(maybeInput.inputId);
        }
      }
    })();
  }

  private async getInitialInputState(
    idPrefix: string,
    initInputs: RegisterInputOptions[]
  ): Promise<void> {
    if (initInputs.length > 0) {
      for (const input of initInputs) {
        await this.addNewInput(input);
      }
    } else {
      if (this.mp4Files.length > 0) {
        const randomIndex = Math.floor(Math.random() * this.mp4Files.length);
        for (let i = 0; i < 2; i++) {
          const randomMp4 = this.mp4Files[(randomIndex + i) % this.mp4Files.length];
          const mp4FilePath = path.join(this.mp4sDir, randomMp4);

          this.inputs.push({
            inputId: `${idPrefix}::local::sample_streamer::${i}`,
            type: 'local-mp4',
            status: 'disconnected',
            showTitle: false,
            shaders: [],
            metadata: {
              title: `[MP4] ${formatMp4Name(randomMp4)}`,
              description: '[Static source] AI Generated',
            },
            mp4FilePath,
            volume: 0,
          });
        }
      }
    }
  }

  public getWhepUrl(): string {
    return this.output.url;
  }

  public getState(): [RoomInputState[], Layout] {
    this.lastReadTimestamp = Date.now();
    return [this.inputs, this.layout];
  }
  public getInputs(): RoomInputState[] {
    return this.inputs;
  }

  public async addNewWhipInput(username: string) {
    const inputId = `${this.idPrefix}::whip::${Date.now()}`;
    const monitor = await WhipInputMonitor.startMonitor(username);
    monitor.touch();
    this.inputs.push({
      inputId,
      type: 'whip',
      status: 'disconnected',
      showTitle: false,
      shaders: [],
      monitor: monitor,
      metadata: {
        title: `[Camera] ${username}`,
        description: `Whip Input for ${username}`,
      },
      volume: 0,
      whipUrl: '',
    });

    return inputId;
  }

  public async addNewInput(opts: RegisterInputOptions) {
    if (opts.type === 'whip') {
      const inputId = await this.addNewWhipInput(opts.username);
      return inputId;
    } else if (opts.type === 'twitch-channel') {
      const inputId = inputIdForTwitchInput(this.idPrefix, opts.channelId);
      if (this.inputs.find(input => input.inputId === inputId)) {
        throw new Error(`Input for Twitch channel ${opts.channelId} already exists.`);
      }

      const hlsUrl = await hlsUrlForTwitchChannel(opts.channelId);
      const monitor = await TwitchChannelMonitor.startMonitor(opts.channelId);

      const inputState: RoomInputState = {
        inputId,
        type: `twitch-channel`,
        status: 'disconnected',
        showTitle: false,
        shaders: [],
        metadata: {
          title: '', // will be populated on update
          description: '',
        },
        volume: 0,
        channelId: opts.channelId,
        hlsUrl,
        monitor,
      };
      monitor.onUpdate((streamInfo, _isLive) => {
        inputState.metadata.title = `[Twitch.tv/${streamInfo.category}] ${streamInfo.displayName}`;
        inputState.metadata.description = streamInfo.title;
        this.updateStoreWithState();
      });
      this.inputs.push(inputState);
      return inputId;
    } else if (opts.type === 'kick-channel') {
      const inputId = inputIdForKickInput(this.idPrefix, opts.channelId);
      if (this.inputs.find(input => input.inputId === inputId)) {
        throw new Error(`Input for Kick channel ${opts.channelId} already exists.`);
      }

      const hlsUrl = await hlsUrlForKickChannel(opts.channelId);
      const monitor = await KickChannelMonitor.startMonitor(opts.channelId);

      const inputState: RoomInputState = {
        inputId,
        type: `kick-channel`,
        status: 'disconnected',
        showTitle: false,
        metadata: {
          title: '', // will be populated on update
          description: '',
        },
        shaders: [],
        volume: 0,
        channelId: opts.channelId,
        hlsUrl,
        monitor,
      };

      monitor.onUpdate((streamInfo, _isLive) => {
        inputState.metadata.title = `[Kick.com] ${streamInfo.displayName}`;
        inputState.metadata.description = streamInfo.title;
        this.updateStoreWithState();
      });

      this.inputs.push(inputState);
      return inputId;
    } else if (opts.type === 'local-mp4' && opts.source.fileName) {
      console.log('Adding local mp4');
      let mp4Path = path.join(process.cwd(), 'mp4s', opts.source.fileName);
      let mp4Name = opts.source.fileName;
      const inputId = `${this.idPrefix}::local::sample_streamer::${Date.now()}`;

      if (await fs.pathExists(mp4Path)) {
        this.inputs.push({
          inputId,
          type: 'local-mp4',
          status: 'disconnected',
          showTitle: false,
          shaders: [],
          metadata: {
            title: `[MP4] ${formatMp4Name(mp4Name)}`,
            description: '[Static source] AI Generated',
          },
          mp4FilePath: mp4Path,
          volume: 0,
        });
      }

      return inputId;
    }
  }

  public async removeInput(inputId: string): Promise<void> {
    const input = this.getInput(inputId);
    this.inputs = this.inputs.filter(input => input.inputId !== inputId);
    this.updateStoreWithState();
    if (input.type === 'twitch-channel' || input.type === 'kick-channel') {
      input.monitor.stop();
    }

    while (input.status === 'pending') {
      await sleep(500);
    }
    if (input.status === 'connected') {
      try {
        await SmelterInstance.unregisterInput(inputId);
      } catch (err: any) {
        console.log(err, 'Failed to unregister when removing input.');
      }
      input.status = 'disconnected';
    }
  }

  public async connectInput(inputId: string): Promise<string> {
    const input = this.getInput(inputId);
    if (input.status !== 'disconnected') {
      return '';
    }
    input.status = 'pending';
    const options = registerOptionsFromInput(input);
    let response = '';
    try {
      const res = await SmelterInstance.registerInput(inputId, options);
      response = res;
    } catch (err: any) {
      response = err.body?.url;
      input.status = 'disconnected';
      throw err;
    }
    input.status = 'connected';
    this.updateStoreWithState();
    return response;
  }

  public async ackWhipInput(inputId: string): Promise<void> {
    const input = this.getInput(inputId);
    if (input.type !== 'whip') {
      throw new Error('Input is not a Whip input');
    }
    input.monitor.touch();
  }

  public async disconnectInput(inputId: string) {
    const input = this.getInput(inputId);
    if (input.status === 'disconnected') {
      return;
    }
    input.status = 'pending';
    this.updateStoreWithState();
    try {
      await SmelterInstance.unregisterInput(inputId);
    } finally {
      input.status = 'disconnected';
      this.updateStoreWithState();
    }
  }

  public async removeStaleWhipInputs(staleTtlMs: number): Promise<void> {
    const now = Date.now();
    for (const input of this.getInputs()) {
      if (input.type === 'whip') {
        const last = input.monitor.getLastAckTimestamp() || 0;
        if (now - last > staleTtlMs) {
          try {
            console.log('[monitor] Removing stale WHIP input', { inputId: input.inputId });
            await this.removeInput(input.inputId);
          } catch (err: any) {
            console.log(err, 'Failed to remove stale WHIP input');
          }
        }
      }
    }
  }

  public async updateInput(inputId: string, options: Partial<UpdateInputOptions>) {
    const input = this.getInput(inputId);
    input.volume = options.volume ?? input.volume;
    input.shaders = options.shaders ?? input.shaders;
    input.showTitle = options.showTitle ?? input.showTitle;
    this.updateStoreWithState();
  }

  public reorderInputs(inputOrder: string[]) {
    const inputIdSet = new Set(this.inputs.map(input => input.inputId));
    const inputs: RoomInputState[] = [];
    for (const inputId of inputOrder) {
      const input = this.inputs.find(input => input.inputId === inputId);
      if (input) {
        inputs.push(input);
        inputIdSet.delete(inputId);
      }
    }
    for (const inputId of inputIdSet) {
      const input = this.inputs.find(input => input.inputId === inputId);
      if (input) {
        inputs.push(input);
      }
    }

    this.inputs = inputs;
    this.updateStoreWithState();
  }

  public updateLayout(layout: Layout) {
    this.layout = layout;
    this.updateStoreWithState();
  }

  public async deleteRoom() {
    const inputs = this.inputs;
    this.inputs = [];
    for (const input of inputs) {
      if (input.type === 'twitch-channel' || input.type === 'kick-channel') {
        input.monitor.stop();
      }
      try {
        await SmelterInstance.unregisterInput(input.inputId);
      } catch (err: any) {
        console.error('Failed to remove input when removing the room.', err?.body ?? err);
      }
    }

    try {
      await SmelterInstance.unregisterOutput(this.output.id);
    } catch (err: any) {
      console.error('Failed to remove output', err?.body ?? err);
    }
  }

  private updateStoreWithState() {
    const inputs: InputConfig[] = this.inputs
      .filter(input => input.status === 'connected')
      .map(input => ({
        inputId: input.inputId,
        title: input.metadata.title,
        description: input.metadata.description,
        showTitle: input.showTitle,
        volume: input.volume,
        shaders: input.shaders,
      }));
    this.output.store.getState().updateState(inputs, this.layout);
  }

  private getInput(inputId: string): RoomInputState {
    const input = this.inputs.find(input => input.inputId === inputId);
    if (!input) {
      throw new Error(`Input ${inputId} not found`);
    }
    return input;
  }
}

function registerOptionsFromInput(input: RoomInputState): RegisterSmelterInputOptions {
  if (input.type === 'local-mp4') {
    return { type: 'mp4', filePath: input.mp4FilePath };
  } else if (input.type === 'twitch-channel' || input.type === 'kick-channel') {
    return { type: 'hls', url: input.hlsUrl };
  } else if (input.type === 'whip') {
    return { type: 'whip', url: input.whipUrl };
  } else {
    throw Error('Unknown type');
  }
}

function inputIdForTwitchInput(idPrefix: string, twitchChannelId: string): string {
  return `${idPrefix}::twitch::${twitchChannelId}`;
}

function inputIdForKickInput(idPrefix: string, kickChannelId: string): string {
  return `${idPrefix}::kick::${kickChannelId}`;
}

function formatMp4Name(fileName: string): string {
  const fileNameWithoutExt = fileName.replace(/\.mp4$/i, '');
  return fileNameWithoutExt
    .split(/[_\- ]+/)
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}
