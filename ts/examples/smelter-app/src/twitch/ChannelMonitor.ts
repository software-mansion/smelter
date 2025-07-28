import type { TwitchStreamInfo } from './TwitchApi';
import { getStreamInfo, getTopStreamsFromCategory } from './TwitchApi';
import { sleep } from '../utils';

const CATEGORY_ID_EA_SPORTS_FC_25 = '2011938005';
// const CATEGORY_ID_ANIMALS = '272263131';
//const categoryIdMap = {
//  'NBA 2K25': '2068583461',
//  'F1 25': '93798731',
//  'EA Sports UFC 5': '1628434805',
//  'TEKKEN 8': '538054672',
//  Chess: '743',
//  Sports: '518203',
//} as const;

const CATEGORIES = [CATEGORY_ID_EA_SPORTS_FC_25];
const STREAMS_PER_CATEGORY = 5;

class TwitchChannelSuggestionsMonitor {
  private topStreams: TwitchStreamInfo[] = [];

  public async monitor() {
    while (true) {
      try {
        await this.refreshCategoryInfo(CATEGORIES);
        await sleep(60_00);
      } catch (err) {
        console.log('Failed to refresh Twitch channel information', err);
      }
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
    const streamInfo = await getStreamInfo(channelId);
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
      try {
        const streamInfo = await getStreamInfo(this.channelId);
        if (streamInfo) {
          this.streamInfo = streamInfo;
          this.isStreamLive = true;
          this.onUpdateFn?.(streamInfo, this.isStreamLive);
        } else {
          this.isStreamLive = false;
          return;
        }
        await sleep(60_00);
      } catch (err) {
        console.log('Failed to refresh Twitch channel information', err);
      }
    }
  }
}

async function getTopStreams(categoryId: string): Promise<TwitchStreamInfo[]> {
  const streamIds = await getTopStreamsFromCategory(categoryId, STREAMS_PER_CATEGORY);
  return await Promise.all(
    streamIds
      .map(async streamId => {
        return (await getStreamInfo(streamId))!;
      })
      .filter(stream => !!stream)
  );
}

export const TwitchChannelSuggestions = new TwitchChannelSuggestionsMonitor();
