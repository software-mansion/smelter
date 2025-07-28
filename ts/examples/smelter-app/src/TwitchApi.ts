import assert from 'assert';
import { URLSearchParams } from 'url';

function getConfig(): { clientId: string; clientSecret: string } {
  assert(process.env.TWITCH_CLIENT_ID, 'missing TWITCH_CLIENT_ID');
  assert(process.env.TWITCH_CLIENT_SECRET, 'missing TWITCH_CLIENT_SECRET');
  return {
    clientId: process.env.TWITCH_CLIENT_ID,
    clientSecret: process.env.TWITCH_CLIENT_SECRET,
  };
}

export async function getTopStreamsFromCategory(
  categoryId: string,
  count: number = 2
): Promise<string[]> {
  const { token, clientId } = await getTwitchAccessToken();

  const topStreamsResponse = await fetch(
    `https://api.twitch.tv/helix/streams?game_id=${encodeURIComponent(categoryId)}&language=en&first=${count}`,
    {
      headers: {
        'Client-ID': clientId,
        Authorization: `Bearer ${token}`,
      },
    }
  );
  if (!topStreamsResponse.ok) {
    throw new Error('Failed to fetch streams from Twitch API');
  }
  const top5Streams = await topStreamsResponse.json();
  const top5UsersLogins = top5Streams.data.map((s: any) => s.user_login);

  return top5UsersLogins;
}

export async function getStreamInfo(
  twitchChannelId: string
): Promise<TwitchStreamInfo | undefined> {
  const { token, clientId } = await getTwitchAccessToken();
  const response = await fetch(
    `https://api.twitch.tv/helix/streams?user_login=${encodeURIComponent(twitchChannelId)}`,
    {
      headers: {
        'Client-ID': clientId,
        Authorization: `Bearer ${token}`,
      },
    }
  );
  if (!response.ok) {
    throw new Error(`Failed to get stream status for ${twitchChannelId}: ${await response.text()}`);
  }
  const data = await response.json();
  const stream = data.data ? data.data[0] : null;

  return stream
    ? {
        streamId: twitchChannelId,
        displayName: stream.user_name ?? '',
        title: stream.title ?? stream?.user_name ?? '',
        category: stream.game_name ?? '',
      }
    : undefined;
}

async function getTwitchAccessToken(): Promise<{ token: string; clientId: string }> {
  const { clientId, clientSecret } = getConfig();

  const response = await fetch('https://id.twitch.tv/oauth2/token', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams({
      client_id: `${clientId}`,
      client_secret: `${clientSecret}`,
      grant_type: 'client_credentials',
    }),
  });
  if (!response.ok) {
    throw new Error(`Failed to fetch access token: ${await response.text()}`);
  }
  const data = await response.json();
  return { token: data.access_token, clientId };
}

export interface TwitchStreamInfo {
  streamId: string;
  displayName: string;
  title: string;
  category: string;
}
