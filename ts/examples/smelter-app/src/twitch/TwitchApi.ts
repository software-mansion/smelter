import { URLSearchParams } from 'url';

const twitchAuth = {
  token: null as string | null,
  clientId: null as string | null,
  tokenPromise: null as Promise<void> | null,
};

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

async function twitchFetch(
  input: RequestInfo,
  init: RequestInit = {},
  retry = true
): Promise<Response> {
  if (!twitchAuth.token) {
    await refreshTwitchToken();
  }
  const headers = new Headers(init.headers || {});
  if (twitchAuth.token && twitchAuth.clientId) {
    headers.set('Client-ID', twitchAuth.clientId);
    headers.set('Authorization', `Bearer ${twitchAuth.token}`);
  }
  let response = await fetch(input, { ...init, headers });
  if (response.status === 401 && retry) {
    await refreshTwitchToken(true);
    if (twitchAuth.token && twitchAuth.clientId) {
      headers.set('Client-ID', twitchAuth.clientId);
      headers.set('Authorization', `Bearer ${twitchAuth.token}`);
    }
    response = await fetch(input, { ...init, headers });
  }
  return response;
}

async function refreshTwitchToken(force = false): Promise<void> {
  if (twitchAuth.tokenPromise && !force) {
    await twitchAuth.tokenPromise;
    return;
  }
  const config = getConfig();
  if (!config) {
    twitchAuth.token = null;
    twitchAuth.clientId = null;
    return;
  }
  twitchAuth.tokenPromise = (async () => {
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
      twitchAuth.token = null;
      twitchAuth.clientId = null;
      throw new Error(`Failed to fetch access token: ${await response.text()}`);
    }
    const data = await response.json();
    twitchAuth.token = data.access_token;
    twitchAuth.clientId = config.clientId;
    console.log(`[twitch] Got Twitch access token`);
  })();
  await twitchAuth.tokenPromise;
  twitchAuth.tokenPromise = null;
}

export async function getTopStreamsFromCategory(
  categoryId: string,
  count: number = 2
): Promise<string[]> {
  const response = await twitchFetch(
    `https://api.twitch.tv/helix/streams?game_id=${encodeURIComponent(categoryId)}&language=en&first=${count}`
  );
  if (!response.ok) {
    throw new Error('Failed to fetch streams from Twitch API');
  }
  const topStreams = await response.json();
  const topUsersLogins = topStreams.data.map((s: any) => s.user_login);
  return topUsersLogins;
}

export async function getTwitchStreamInfo(
  twitchChannelId: string
): Promise<TwitchStreamInfo | undefined> {
  const response = await twitchFetch(
    `https://api.twitch.tv/helix/streams?user_login=${encodeURIComponent(twitchChannelId)}`
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

export interface TwitchStreamInfo {
  streamId: string;
  displayName: string;
  title: string;
  category: string;
}
