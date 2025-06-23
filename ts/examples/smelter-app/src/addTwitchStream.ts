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

  let isStreaming = await isStreamLive(streamId);
  console.log(`Is \"${streamId}\" streaming: ${isStreaming}`);

  let streams = await getTopStreamsFromCategory('FC 25');
  let top1StreamId = streams[0];

  try {
    const streamlinkOutput = await spawn(
      'streamlink',
      ['--stream-url', `https://www.twitch.tv/${top1StreamId}`, 'best'],
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

async function getTopStreamsFromCategory(category: string): Promise<string[]> {
  const CLIENT_ID = process.env.DEMO_CLIENT_ID;
  const CLIENT_SECRET = process.env.DEMO_CLIENT_SECRET;

  if (!CLIENT_ID || !CLIENT_SECRET) {
    throw new Error('Both DEMO_CLIENT_ID and DEMO_CLIENT_SECRET environment variables must be set');
  }
  const token = await getTwitchAccessToken(CLIENT_ID, CLIENT_SECRET);

  const categoryResponse = await fetch(
    `https://api.twitch.tv/helix/search/categories?query=${encodeURIComponent(category)}`,
    {
      headers: {
        'Client-ID': `${CLIENT_ID}`,
        Authorization: `Bearer ${token}`,
      },
    }
  );

  const categoryData = await categoryResponse.json();
  const categoryId = categoryData.data?.[0]?.id;
  if (!categoryId) throw new Error('Could not find provided category');

  const top5StreamsResponse = await fetch(
    `https://api.twitch.tv/helix/streams?game_id=${encodeURIComponent(categoryId)}&first=5`,
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

async function isStreamLive(username: string) {
  const CLIENT_ID = process.env.DEMO_CLIENT_ID;
  const CLIENT_SECRET = process.env.DEMO_CLIENT_SECRET;
  if (!CLIENT_ID || !CLIENT_SECRET) {
    throw new Error('Both DEMO_CLIENT_ID and DEMO_CLIENT_SECRET environment variables must be set');
  }
  const token = await getTwitchAccessToken(CLIENT_ID, CLIENT_SECRET);

  const response = await fetch(
    `https://api.twitch.tv/helix/streams?user_login=${encodeURIComponent(username)}`,
    {
      headers: {
        'Client-ID': CLIENT_ID,
        Authorization: `Bearer ${token}`,
      },
    }
  );
  if (!response.ok) {
    throw new Error(`Failed to get stream status for ${username}`);
  }
  const data = await response.json();
  return data.data && data.data.length > 0;
}
