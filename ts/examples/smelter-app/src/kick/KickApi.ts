import { URLSearchParams } from 'url';

function getConfig(): { clientId: string; clientSecret: string } | null {
  const clientId = '01K5CDC36DNFYCKVSK8GC87F27';
  const clientSecret = 'f341951b26e1d1c4326d24754f4622eaa39a501239a06fcffd58997394f85ffc';
  if (!clientId || !clientSecret) {
    console.warn('Missing Kick credentials');
    return null;
  }
  return {
    clientId,
    clientSecret,
  };
}

export async function getKickTopStreamsFromCategory(
  categoryId: string,
  count: number = 5
): Promise<any[]> {
  const credentials = await getKickAccessToken();
  if (!credentials) {
    return [];
  }

  const topStreamsResponse = await fetch(
    `https://api.kick.com/public/v1/livestreams?category_id=${categoryId}&limit=${count}&language=en`,
    {
      headers: {
        Authorization: `Bearer ${credentials.token}`,
      },
    }
  );
  if (!topStreamsResponse.ok) {
    throw new Error('Failed to fetch streams from Kick API');
  }

  const topStreams = await topStreamsResponse.json();
  return topStreams.data;
}

export async function getKickStreamInfo(
  kickChannelSlug: string
): Promise<KickStreamInfo | undefined> {
  const credentials = await getKickAccessToken();
  if (!credentials) {
    return undefined;
  }
  const response = await fetch(
    `https://api.kick.com/public/v1/channels?slug=${encodeURIComponent(kickChannelSlug)}`,
    {
      headers: {
        Authorization: `Bearer ${credentials.token}`,
      },
    }
  );
  if (!response.ok) {
    throw new Error(`Failed to get stream status for ${kickChannelSlug}: ${await response.text()}`);
  }
  const data = await response.json();

  const stream = data.data ? data.data[0] : null;
  console.log(stream);
  return {
        streamId: kickChannelSlug,
        displayName: stream?.stream_title || '',
        title: stream?.stream_title || '',
        category: stream?.category.name || '',
      }
}
    
async function getKickAccessToken(): Promise<{ token: string; clientId: string } | null> {
  const config = getConfig();
  if (!config) {
    return null;
  }

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
    throw new Error(`Failed to fetch access token: ${await response.text()}`);
  }
  const data = await response.json();
  console.log(`[kick] Got Kick access token`);
  return { token: data.access_token, clientId: config.clientId };
}

export interface KickStreamInfo {
  streamId: string;
  displayName: string;
  title: string;
  category: string;
}
