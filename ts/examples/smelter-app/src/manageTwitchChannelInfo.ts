import { store } from './store';
import type { TwitchStreamInfo } from './TwitchApi';
import { getStreamInfo, getTopStreamsFromCategory } from './TwitchApi';
import { sleep } from './utils';

const CATEGORY_ID_EA_SPORTS_FC_25 = '2011938005';
// const CATEGORY_ID_ANIMALS = '272263131';

const CATEGORIES = [CATEGORY_ID_EA_SPORTS_FC_25];
const STREAMS_PER_CATEGORY = 2;

export async function manageTwitchChannelInfo() {
  void startCategoryRefreshIntervalLoop();
  void startStreamInfoRefreshIntervalLoop();
}

async function startCategoryRefreshIntervalLoop() {
  while (true) {
    try {
      await refreshCategoryInfo(CATEGORIES);
      await sleep(60_000);
    } catch (err) {
      console.log('Failed to refresh Twitch channel information', err);
    }
  }
}

async function startStreamInfoRefreshIntervalLoop() {
  while (true) {
    try {
      let streamIds = store
        .getState()
        .availableStreams.filter(stream => stream.type !== 'static')
        .map(stream => stream.id);
      for (const streamId of streamIds) {
        await refreshStreamInfo(streamId);
      }
      await sleep(60_000);
    } catch (err) {
      console.log('Failed to refresh Twitch channel information', err);
    }
  }
}

//const categoryIdMap = {
//  'NBA 2K25': '2068583461',
//  'F1 25': '93798731',
//  'EA Sports UFC 5': '1628434805',
//  'TEKKEN 8': '538054672',
//  Chess: '743',
//  Sports: '518203',
//} as const;

async function refreshCategoryInfo(categories: string[]): Promise<void> {
  const streamsByCategory = await Promise.all(
    categories.map(async categoryId => await getTopStreams(categoryId))
  );
  const streams = streamsByCategory.flat();
  store.getState().refreshAvailableStreams(streams);
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

async function refreshStreamInfo(streamId: string): Promise<void> {
  let result = await getStreamInfo(streamId);
  if (result) {
    store.getState().updateStreamInfo(result);
  } else {
    store.getState().markStreamOffline(streamId);
  }
}
