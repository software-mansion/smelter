import { SmelterInstance } from './smelter';
import { store } from './store';
import { spawn } from './utils';

export async function addTwitchStream(streamId: string): Promise<void> {
  let state = store.getState();
  if (state.availableStreams.filter(stream => stream.id == streamId).length === 0) {
    throw new Error(`Unknown streamId: ${streamId}`);
  }

  if (state.connectedStreamIds.filter(id => id === streamId).length > 0) {
    throw new Error('Already connected stream.');
  }
  state.addStream(streamId);

  try {
    const streamlinkOutput = await spawn(
      'streamlink',
      ['--stream-url', `https://www.twitch.tv/${streamId}`, 'best'],
      {
        stdio: 'pipe',
      }
    );
    const hlsPlaylistUrl = streamlinkOutput.stdout.trim();

    await SmelterInstance.registerInput(streamId, {
      type: 'hls',
      url: hlsPlaylistUrl,
    });
  } catch (err: any) {
    console.log(err.stdout, err.stderr, err);
    state.removeStream(streamId);
    throw err;
  }
}

// List of categories https://gist.github.com/basilbenny1002/7a0c2096b691048f5de8c100afcbe1a7
const categoryIdMap = {
  'EA Sports FC 25': '2011938005',
  'NBA 2K25': '2068583461',
  'F1 25': '93798731',
  'EA Sports UFC 5': '1628434805',
  'TEKKEN 8': '538054672',
  Chess: '743',
  Sports: '518203',
};

export async function addTwitchStreamByCategory(
  categoryName: keyof typeof categoryIdMap
): Promise<void> {
  let state = store.getState();

  const categoryId = categoryIdMap[categoryName];
  if (!categoryId) {
    throw new Error(`Invalid category name provided: ${categoryName}`);
  }

  let streams = await getTopStreamsFromCategory(categoryId);
  if (streams.length === 0) {
    throw new Error('No streams available in the specified category.');
  }

  const top1StreamId = streams[0];

  if (state.connectedStreamIds.includes(top1StreamId)) {
    throw new Error('Stream is already connected.');
  }

  state.addStream(top1StreamId);
  console.log(`Stream \"${top1StreamId}\" successfully added`);

  let streamData = await getStreamInfo(top1StreamId);
  console.log(`Is \"${top1StreamId}\" streaming: ${streamData.isLive}`);
  console.log(streamData);

  try {
    const streamlinkOutput = await spawn(
      'streamlink',
      ['--stream-url', `https://www.twitch.tv/${top1StreamId}`, 'best'],
      {
        stdio: 'pipe',
      }
    );
    const hlsPlaylistUrl = streamlinkOutput.stdout.trim();

    await SmelterInstance.registerInput(top1StreamId, {
      type: 'hls',
      url: hlsPlaylistUrl,
    });
  } catch (err: any) {
    console.log(err.stdout, err.stderr, err);
    state.removeStream(top1StreamId);
    throw err;
  }
}

async function getTopStreamsFromCategory(categoryId: string): Promise<string[]> {
  const CLIENT_ID = process.env.DEMO_CLIENT_ID;
  const CLIENT_SECRET = process.env.DEMO_CLIENT_SECRET;

  if (!CLIENT_ID || !CLIENT_SECRET) {
    throw new Error('Both DEMO_CLIENT_ID and DEMO_CLIENT_SECRET environment variables must be set');
  }
  const token = await getTwitchAccessToken(CLIENT_ID, CLIENT_SECRET);

  const top5StreamsResponse = await fetch(
    `https://api.twitch.tv/helix/streams?game_id=${encodeURIComponent(categoryId)}&language=en&first=5`,
    {
      headers: {
        'Client-ID': `${CLIENT_ID}`,
        Authorization: `Bearer ${token}`,
      },
    }
  );
  if (!top5StreamsResponse.ok) {
    throw new Error('Failed to fetch streams from Twitch API');
  }
  const top5Streams = await top5StreamsResponse.json();
  const top5UsersLogins = top5Streams.data.map((s: any) => s.user_login);

  return top5UsersLogins;
}

async function getTwitchAccessToken(client_id: string, client_secret: string): Promise<string> {
  const url = new URL('https://id.twitch.tv/oauth2/token');
  url.searchParams.append('client_id', client_id);
  url.searchParams.append('client_secret', client_secret);
  url.searchParams.append('grant_type', 'client_credentials');

  const response = await fetch(url, {
    method: 'POST',
  });
  if (!response.ok) {
    throw new Error(`Failed to fetch access token: ${response.statusText}`);
  }
  const data = await response.json();
  return data.access_token;
}

interface StreamInfo {
  isLive: boolean;
  user_name: string;
  title?: string;
  description?: string;
  viewerCount?: number;
}

async function getStreamInfo(user_login: string): Promise<StreamInfo> {
  const CLIENT_ID = process.env.DEMO_CLIENT_ID;
  const CLIENT_SECRET = process.env.DEMO_CLIENT_SECRET;
  if (!CLIENT_ID || !CLIENT_SECRET) {
    throw new Error('Both DEMO_CLIENT_ID and DEMO_CLIENT_SECRET environment variables must be set');
  }
  const token = await getTwitchAccessToken(CLIENT_ID, CLIENT_SECRET);
  const response = await fetch(
    `https://api.twitch.tv/helix/streams?user_login=${encodeURIComponent(user_login)}`,
    {
      headers: {
        'Client-ID': `${CLIENT_ID}`,
        Authorization: `Bearer ${token}`,
      },
    }
  );
  if (!response.ok) {
    throw new Error(`Failed to get stream status for ${user_login}: ${response.statusText}`);
  }
  const data = await response.json();
  const stream = data.data ? data.data[0] : null;

  return {
    isLive: !!stream,
    user_name: stream?.user_name,
    title: stream?.title,
    viewerCount: stream?.viewer_count,
  };
}
