import type { KickStreamInfo } from '../kick/KickApi';
import { getKickStreamInfo, getKickTopStreamsFromCategory } from '../kick/KickApi';
import { sleep } from '../utils';

const CHOSEN_KICK_CATEGORY = '5'; // Gaming: LOL
const KICK_CATEGORIES = [CHOSEN_KICK_CATEGORY];
const KICK_STREAMS_PER_CATEGORY = 10;

class KickChannelSuggestionsMonitor {
  private topStreams: KickStreamInfo[] = [];

  public async monitor() {
    while (true) {
      try {
        console.log('[kick] Refresh category info.');
        await this.refreshCategoryInfo(KICK_CATEGORIES);
      } catch (err) {
        console.log('[kick] Failed to refresh channel information', err);
      }
      await sleep(60_000);
    }
  }

  public getTopStreams(): KickStreamInfo[] {
    return this.topStreams;
  }

  private async refreshCategoryInfo(categories: string[]): Promise<void> {
    const streamsByCategory = await Promise.all(
      categories.map(async categoryId => await getKickTopStreams(categoryId))
    );
    const streams = streamsByCategory.flat();
    this.topStreams = streams;
  }
}

export class KickChannelMonitor {
  private channelId: string;
  private streamInfo: KickStreamInfo;
  private isStreamLive: boolean = true;
  private shouldStop = false;
  private onUpdateFn?: (streamInfo: KickStreamInfo, isLive: boolean) => void;

  private constructor(channelId: string, streamInfo: KickStreamInfo) {
    this.channelId = channelId;
    this.streamInfo = streamInfo;
    void this.monitor();
  }

  public static async startMonitor(channelId: string): Promise<KickChannelMonitor> {
    const streamInfo = await getKickStreamInfo(channelId);
    if (!streamInfo) {
      throw new Error(`Unable to find live streams for ${channelId}`);
    }
    return new KickChannelMonitor(channelId, streamInfo);
  }

  public stop() {
    this.shouldStop = true;
  }

  public isLive(): boolean {
    return this.isStreamLive;
  }

  public onUpdate(onUpdateFn: (streamInfo: KickStreamInfo, isLive: boolean) => void): void {
    this.onUpdateFn = onUpdateFn;
    onUpdateFn(this.streamInfo, this.isStreamLive);
  }

  private async monitor() {
    while (!this.shouldStop) {
      console.log(`[kick] Check stream state ${this.channelId}`);
      try {
        const streamInfo = await getKickStreamInfo(this.channelId);
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
        console.log('[kick] Failed to refresh Kick channel information', err);
      }
    }
  }
}

async function getKickTopStreams(categoryId: string): Promise<KickStreamInfo[]> {
  const topStreams = await getKickTopStreamsFromCategory(categoryId, KICK_STREAMS_PER_CATEGORY);
  console.log('[kick] Got Kick top streams');

  return topStreams.map(stream => ({
    streamId: `${stream.slug}`,
    displayName: stream.stream_title,
    title: stream.stream_title,
    category: stream.category.name,
  }));
}

export const KickChannelSuggestions = new KickChannelSuggestionsMonitor();
