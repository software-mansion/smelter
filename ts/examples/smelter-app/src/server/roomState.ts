import fs from 'fs-extra';
import path from 'node:path';
import { SmelterInstance, type RegisterSmelterInputOptions, type SmelterOutput } from '../smelter';
import { hlsUrlForKickChannel, hlsUrlForTwitchChannel } from '../streamlink';
import { TwitchChannelMonitor } from '../twitch/TwitchChannelMonitor';
import { sleep } from '../utils';
import type { InputConfig, Layout } from '../app/store';
import mp4SuggestionsMonitor from '../mp4/mp4SuggestionMonitor';
import { KickChannelMonitor, KickChannelSuggestions } from '../kick/KickChannelMonitor';
import type { ShaderConfig } from '../shaders/shaders';

export type RoomInitType = 'twitch' | 'kick' | 'mp4';

export type RoomInputState = {
  inputId: string;
  type: 'local-mp4' | 'twitch-channel' | 'kick-channel';
  status: 'disconnected' | 'pending' | 'connected';
  volume: number;
  shaders: ShaderConfig[];
  metadata: {
    title: string;
    description: string;
  };
} & TypeSpecificState;

type TypeSpecificState =
  | { type: 'local-mp4'; mp4FilePath: string }
  | { type: 'twitch-channel'; channelId: string; hlsUrl: string; monitor: TwitchChannelMonitor }
  | { type: 'kick-channel'; channelId: string; hlsUrl: string; monitor: KickChannelMonitor };

type UpdateInputOptions = {
  volume: number;
  shaders: ShaderConfig[];
};

export type RegisterInputOptions =
  | {
      type: 'twitch-channel';
      twitchChannelId: string;
    }
  | {
      type: 'kick-channel';
      kickChannelId: string;
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
  private layout: Layout = 'grid';
  private idPrefix: string;

  private mp4sDir: string;
  private mp4Files: string[];
  private output: SmelterOutput;

  public lastReadTimestamp: number;
  public creationTimestamp: number;

  public pendingDelete?: boolean;

  public constructor(idPrefix: string, output: SmelterOutput, initType: RoomInitType) {
    this.mp4sDir = path.join(process.cwd(), 'mp4s');
    this.mp4Files = mp4SuggestionsMonitor.mp4Files;
    this.inputs = [];
    this.idPrefix = idPrefix;
    this.output = output;

    this.lastReadTimestamp = Date.now();
    this.creationTimestamp = Date.now();
    const realThis = this;

    void (async () => {
      await this.getInitialInputState(idPrefix, initType);
      for (let i = 0; i < realThis.inputs.length; i++) {
        const maybeInput = realThis.inputs[i];
        if (maybeInput) {
          await this.connectInput(maybeInput.inputId);
        }
      }
    })();
  }

  private async getInitialInputState(idPrefix: string, initType: RoomInitType): Promise<void> {
    if (initType === 'kick') {
      const topStreams = KickChannelSuggestions.getTopStreams();
      for (let i = 0; i < Math.min(2, topStreams.length); i++) {
        const stream = topStreams[i];
        await this.addNewInput({
          type: 'kick-channel',
          kickChannelId: stream.streamId,
        });
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

  public async addNewInput(opts: RegisterInputOptions) {
    if (opts.type === 'twitch-channel') {
      const inputId = inputIdForTwitchInput(this.idPrefix, opts.twitchChannelId);
      if (this.inputs.find(input => input.inputId === inputId)) {
        throw new Error(`Input for Twitch channel ${opts.twitchChannelId} already exists.`);
      }

      const hlsUrl = await hlsUrlForTwitchChannel(opts.twitchChannelId);
      const monitor = await TwitchChannelMonitor.startMonitor(opts.twitchChannelId);

      const inputState: RoomInputState = {
        inputId,
        type: `twitch-channel`,
        status: 'disconnected',
        shaders: [],
        metadata: {
          title: '', // will be populated on update
          description: '',
        },
        volume: 0,
        channelId: opts.twitchChannelId,
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
      const inputId = inputIdForKickInput(this.idPrefix, opts.kickChannelId);
      if (this.inputs.find(input => input.inputId === inputId)) {
        throw new Error(`Input for Kick channel ${opts.kickChannelId} already exists.`);
      }

      const hlsUrl = await hlsUrlForKickChannel(opts.kickChannelId);
      const monitor = await KickChannelMonitor.startMonitor(opts.kickChannelId);

      const inputState: RoomInputState = {
        inputId,
        type: `kick-channel`,
        status: 'disconnected',
        metadata: {
          title: '', // will be populated on update
          description: '',
        },
        shaders: [],
        volume: 0,
        channelId: opts.kickChannelId,
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

      if (await fs.exists(mp4Path)) {
        this.inputs.push({
          inputId,
          type: 'local-mp4',
          status: 'disconnected',
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

  public async connectInput(inputId: string) {
    const input = this.getInput(inputId);
    if (input.status !== 'disconnected') {
      return;
    }
    input.status = 'pending';
    const options = registerOptionsFromInput(input);
    try {
      await SmelterInstance.registerInput(inputId, options);
    } catch (err: any) {
      input.status = 'disconnected';
      throw err;
    }
    input.status = 'connected';
    this.updateStoreWithState();
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

  public async updateInput(inputId: string, options: Partial<UpdateInputOptions>) {
    const input = this.getInput(inputId);
    input.volume = options.volume ?? input.volume;
    input.shaders = options.shaders ?? input.shaders;
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
  } else if (['twitch-channel', 'kick-channel'].includes(input.type)) {
    return { type: 'hls', url: input.hlsUrl };
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
