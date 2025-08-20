import { URLSearchParams } from 'url';

function getConfig(): { clientId: string; clientSecret: string } | null {
  const clientId = process.env.TWITCH_CLIENT_ID;
  const clientSecret = process.env.TWITCH_CLIENT_SECRET;
  if (!clientId || !clientSecret) {
    console.warn('Missing twitch credentials');
    return null;
  }
  return {
    clientId,
    clientSecret,
  };
}

export async function getTopStreamsFromCategory(
  categoryId: string,
  count: number = 2
): Promise<string[]> {
  const credentials = await getTwitchAccessToken();
  if (!credentials) {
    return [];
  }

  const topStreamsResponse = await fetch(
    `https://api.twitch.tv/helix/streams?game_id=${encodeURIComponent(categoryId)}&language=en&first=${count}`,
    {
      headers: {
        'Client-ID': credentials.clientId,
        Authorization: `Bearer ${credentials.token}`,
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
  const credentials = await getTwitchAccessToken();
  if (!credentials) {
    return undefined;
  }
  const response = await fetch(
    `https://api.twitch.tv/helix/streams?user_login=${encodeURIComponent(twitchChannelId)}`,
    {
      headers: {
        'Client-ID': credentials.clientId,
        Authorization: `Bearer ${credentials.token}`,
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

async function getTwitchAccessToken(): Promise<{ token: string; clientId: string } | null> {
  const config = getConfig();
  if (!config) {
    return null;
  }

  const response = await fetch('https://id.twitch.tv/oauth2/token', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams({
      client_id: `${config.clientId}`,
      client_secret: `${config.clientSecret}`,
      grant_type: 'client_credentials',
    }),
  });
  if (!response.ok) {
    throw new Error(`Failed to fetch access token: ${await response.text()}`);
  }
  const data = await response.json();
  return { token: data.access_token, clientId: config.clientId };
}

export interface TwitchStreamInfo {
  streamId: string;
  displayName: string;
  title: string;
  category: string;
}
