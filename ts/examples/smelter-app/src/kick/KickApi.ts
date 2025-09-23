import { URLSearchParams } from 'url';

const kickAuth = {
  token: null as string | null,
  clientId: null as string | null,
  tokenPromise: null as Promise<void> | null,
};

function getConfig(): { clientId: string; clientSecret: string } | null {
  const clientId = process.env.KICK_CLIENT_ID;
  const clientSecret = process.env.KICK_CLIENT_SECRET;
  if (!clientId || !clientSecret) {
    console.warn('Missing Kick credentials');
    return null;
  }
  return {
    clientId,
    clientSecret,
  };
}

async function kickFetch(
  input: RequestInfo,
  init: RequestInit = {},
  retry = true
): Promise<Response> {
  if (!kickAuth.token) {
    await refreshKickToken();
  }
  const headers = new Headers(init.headers || {});
  if (kickAuth.token) {
    headers.set('Authorization', `Bearer ${kickAuth.token}`);
  }
  let response = await fetch(input, { ...init, headers });
  if (response.status === 401 && retry) {
    await refreshKickToken(true);
    headers.set('Authorization', `Bearer ${kickAuth.token}`);
    response = await fetch(input, { ...init, headers });
  }
  return response;
}

async function refreshKickToken(force = false): Promise<void> {
  if (kickAuth.tokenPromise && !force) {
    await kickAuth.tokenPromise;
    return;
  }
  const config = getConfig();
  if (!config) {
    kickAuth.token = null;
    kickAuth.clientId = null;
    return;
  }
  kickAuth.tokenPromise = (async () => {
    const response = await fetch('https://id.kick.com/oauth/token', {
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
      kickAuth.token = null;
      kickAuth.clientId = null;
      throw new Error(`Failed to fetch access token: ${await response.text()}`);
    }
    const data = await response.json();
    kickAuth.token = data.access_token;
    kickAuth.clientId = config.clientId;
    console.log(`[kick] Got Kick access token`);
  })();
  await kickAuth.tokenPromise;
  kickAuth.tokenPromise = null;
}

export async function getKickTopStreamsFromCategory(
  categoryId: string,
  count: number = 5
): Promise<any[]> {
  const response = await kickFetch(
    `https://api.kick.com/public/v1/livestreams?category_id=${categoryId}&limit=${count}&language=en`
  );
  if (!response.ok) {
    throw new Error('Failed to fetch streams from Kick API');
  }
  const topStreams = await response.json();
  return topStreams.data;
}

export async function getKickStreamInfo(
  kickChannelSlug: string
): Promise<KickStreamInfo | undefined> {
  const response = await kickFetch(
    `https://api.kick.com/public/v1/channels?slug=${encodeURIComponent(kickChannelSlug)}`
  );
  if (!response.ok) {
    throw new Error(`Failed to get stream status for ${kickChannelSlug}: ${await response.text()}`);
  }
  const data = await response.json();

  const stream = data.data ? data.data[0] : null;
  return {
    streamId: kickChannelSlug,
    displayName: stream?.stream_title || '',
    title: stream?.stream_title || '',
    category: stream?.category.name || '',
  };
}

export interface KickStreamInfo {
  streamId: string;
  displayName: string;
  title: string;
  category: string;
}
