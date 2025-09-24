import type { TwitchStreamInfo } from './TwitchApi';
import { getTwitchStreamInfo, getTopStreamsFromCategory } from './TwitchApi';
import { sleep } from '../utils';

const CATEGORY_ID_EA_SPORTS_FC_25 = '2011938005';
const CATEGORIES = [CATEGORY_ID_EA_SPORTS_FC_25];
const STREAMS_PER_CATEGORY = 5;

class TwitchChannelSuggestionsMonitor {
  private topStreams: TwitchStreamInfo[] = [];

  public async monitor() {
    while (true) {
      try {
        console.log(`[twitch] Refresh category info.`);
        await this.refreshCategoryInfo(CATEGORIES);
      } catch (err) {
        console.log('Failed to refresh Twitch channel information', err);
      }
      await sleep(60_000);
    }
  }

  public getTopStreams(): TwitchStreamInfo[] {
    return this.topStreams;
  }

  private async refreshCategoryInfo(categories: string[]): Promise<void> {
    const streamsByCategory = await Promise.all(
      categories.map(async categoryId => await getTopStreams(categoryId))
    );
    const streams = streamsByCategory.flat();
    this.topStreams = streams;
  }
}

export class TwitchChannelMonitor {
  private channelId: string;
  private streamInfo: TwitchStreamInfo;
  private isStreamLive: boolean = true;
  private shouldStop = false;
  private onUpdateFn?: (streamInfo: TwitchStreamInfo, isLive: boolean) => void;

  private constructor(channelId: string, streamInfo: TwitchStreamInfo) {
    this.channelId = channelId;
    this.streamInfo = streamInfo;
    void this.monitor();
  }

  public static async startMonitor(channelId: string): Promise<TwitchChannelMonitor> {
    const streamInfo = await getTwitchStreamInfo(channelId);
    if (!streamInfo) {
      throw new Error(`Unable to find live streams for ${channelId}`);
    }
    return new TwitchChannelMonitor(channelId, streamInfo);
  }

  public stop() {
    this.shouldStop = true;
  }

  public isLive(): boolean {
    return this.isStreamLive;
  }

  public onUpdate(onUpdateFn: (streamInfo: TwitchStreamInfo, isLive: boolean) => void): void {
    this.onUpdateFn = onUpdateFn;
    onUpdateFn(this.streamInfo, this.isStreamLive);
  }

  private async monitor() {
    while (!this.shouldStop) {
      console.log(`[twitch] Check stream state ${this.channelId}`);
      try {
        const streamInfo = await getTwitchStreamInfo(this.channelId);
        if (streamInfo) {
          this.streamInfo = streamInfo;
          this.isStreamLive = true;
          this.onUpdateFn?.(streamInfo, this.isStreamLive);
        } else {
          this.isStreamLive = false;
          return;
        }
        await sleep(20_000);
      } catch (err) {
        console.log('Failed to refresh Twitch channel information', err);
      }
    }
  }
}

async function getTopStreams(categoryId: string): Promise<TwitchStreamInfo[]> {
  console.log('[twitch] Got Twitch top streams');

  const streamIds = await getTopStreamsFromCategory(categoryId, STREAMS_PER_CATEGORY);
  return await Promise.all(
    streamIds
      .map(async streamId => {
        return (await getTwitchStreamInfo(streamId))!;
      })
      .filter(stream => !!stream)
  );
}

export const TwitchChannelSuggestions = new TwitchChannelSuggestionsMonitor();
